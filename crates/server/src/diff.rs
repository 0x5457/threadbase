//! Git diff baselines, per-file review, Seen state and merge-back.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::threads::ThreadRow;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use threadbase_core::state::ReadRules;
use threadbase_shared::{
    now_ms, ApiError, BaselineDto, BaselineKind, ChangeStats, ChangesDto, DiffHunkDto, DiffLineDto,
    DiffLineKind, FileChangeDto, FileDiffDto, FileStatus, MergeBackResponse,
};
use threadbase_workspace::git;
use threadbase_workspace::path::ensure_within;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ResolvedBaseline {
    pub reference: String,
    pub baseline: BaselineDto,
    pub label: String,
}

pub async fn stats(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
) -> Result<ChangeStats, ServerError> {
    Ok(list(
        pool,
        row,
        user,
        &BaselineDto {
            kind: BaselineKind::BaseBranch,
            commit: None,
        },
    )
    .await?
    .stats)
}

pub async fn list(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
    baseline: &BaselineDto,
) -> Result<ChangesDto, ServerError> {
    let worktree = worktree(row)?;
    let resolved = resolve_baseline(&worktree, row, baseline).await?;
    let numstat = git_stdout(&worktree, &["diff", "--numstat", &resolved.reference]).await?;
    let name_status =
        git_stdout(&worktree, &["diff", "--name-status", &resolved.reference]).await?;
    let untracked = git_stdout(&worktree, &["ls-files", "--others", "--exclude-standard"])
        .await
        .unwrap_or_default();
    let mut files: BTreeMap<String, FileChangeDto> = BTreeMap::new();
    for line in name_status.lines() {
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or_default();
        let first = parts.next().unwrap_or_default().to_owned();
        let second = parts.next().map(ToOwned::to_owned);
        let (path, old_path, status) = if status.starts_with('R') {
            let new_path = second.unwrap_or_default();
            (new_path, Some(first), FileStatus::Renamed)
        } else {
            (first, None, parse_status(status))
        };
        files.entry(path.clone()).or_insert(FileChangeDto {
            path,
            old_path,
            status,
            additions: 0,
            deletions: 0,
            binary: false,
            seen: false,
        });
    }
    for line in numstat.lines() {
        let mut parts = line.splitn(3, '\t');
        let added = parts.next().unwrap_or("0");
        let deleted = parts.next().unwrap_or("0");
        let path = parts.next().unwrap_or_default().to_owned();
        if path.is_empty() {
            continue;
        }
        let binary = added == "-" || deleted == "-";
        let entry = files.entry(path.clone()).or_insert(FileChangeDto {
            path: path.clone(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 0,
            deletions: 0,
            binary: false,
            seen: false,
        });
        entry.additions = added.parse().unwrap_or(0);
        entry.deletions = deleted.parse().unwrap_or(0);
        entry.binary = binary;
    }
    for path in untracked.lines().filter(|line| !line.trim().is_empty()) {
        let full = ensure_within(&worktree, Path::new(path)).map_err(|_| {
            ServerError(ApiError::new(
                threadbase_shared::ErrorCode::PathOutsideWorkspace,
                "file path is outside the thread worktree",
            ))
        })?;
        let (binary, additions) = file_line_stats(&full).await;
        files.entry(path.to_owned()).or_insert(FileChangeDto {
            path: path.to_owned(),
            old_path: None,
            status: FileStatus::Untracked,
            additions,
            deletions: 0,
            binary,
            seen: false,
        });
    }
    let seen_map = seen_hashes(pool, row, user).await?;
    let mut seen_count = 0;
    for file in files.values_mut() {
        let current = current_hash(&worktree, &file.path).await;
        match seen_map.get(&file.path) {
            Some(stored) if ReadRules::seen(Some(stored), &current) => {
                file.seen = true;
                seen_count += 1;
            }
            Some(_) => {
                sqlx::query(
                    "DELETE FROM seen_files WHERE thread_id = ? AND user_id = ? AND path = ?",
                )
                .bind(&row.id)
                .bind(user.user.id.as_str())
                .bind(&file.path)
                .execute(pool)
                .await?;
            }
            None => {}
        }
    }
    let mut additions = 0;
    let mut deletions = 0;
    for file in files.values() {
        additions += file.additions;
        deletions += file.deletions;
    }
    let total = u32::try_from(files.len()).unwrap_or(0);
    let stats = ChangeStats {
        files: total,
        additions,
        deletions,
        seen: seen_count,
        total,
    };
    Ok(ChangesDto {
        baseline: resolved.baseline,
        base_label: resolved.label,
        files: files.into_values().collect(),
        stats,
        commit: row.base_sha.clone(),
    })
}

pub async fn file_diff(
    row: &ThreadRow,
    baseline: &BaselineDto,
    path: &str,
) -> Result<FileDiffDto, ServerError> {
    let worktree = worktree(row)?;
    let resolved = resolve_baseline(&worktree, row, baseline).await?;
    let full = ensure_within(&worktree, Path::new(path)).map_err(|_| {
        ServerError(ApiError::new(
            threadbase_shared::ErrorCode::PathOutsideWorkspace,
            "file path is outside the thread worktree",
        ))
    })?;
    let untracked = git_stdout(&worktree, &["ls-files", "--others", "--exclude-standard"])
        .await
        .unwrap_or_default()
        .lines()
        .any(|line| line == path);
    if untracked {
        let content = tokio::fs::read(&full).await.unwrap_or_default();
        let binary = content.contains(&0);
        let hunks = if binary {
            Vec::new()
        } else {
            let text = String::from_utf8_lossy(&content);
            let lines: Vec<DiffLineDto> = text
                .lines()
                .enumerate()
                .map(|(index, line)| DiffLineDto {
                    kind: DiffLineKind::Add,
                    old_line: None,
                    new_line: Some(u32::try_from(index + 1).unwrap_or(u32::MAX)),
                    content: line.to_owned(),
                })
                .collect();
            if lines.is_empty() {
                Vec::new()
            } else {
                let context_hash = hash_text(&format!("untracked:{path}"));
                vec![DiffHunkDto {
                    id: "h1".to_owned(),
                    header: format!("@@ -0,0 +1,{} @@", lines.len()),
                    context_hash,
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: u32::try_from(lines.len()).unwrap_or(u32::MAX),
                    lines,
                }]
            }
        };
        return Ok(FileDiffDto {
            path: path.to_owned(),
            old_path: None,
            status: FileStatus::Untracked,
            language: language_for(path).map(ToOwned::to_owned),
            binary,
            hunks,
        });
    }
    let patch = git_stdout(
        &worktree,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-U3",
            &resolved.reference,
            "--",
            path,
        ],
    )
    .await?;
    let binary = patch.contains("Binary files") || is_binary_file(&full).await;
    let hunks = if binary {
        Vec::new()
    } else {
        parse_hunks(path, &patch)
    };
    let status = file_status(&worktree, &resolved.reference, path).await;
    Ok(FileDiffDto {
        path: path.to_owned(),
        old_path: None,
        status,
        language: language_for(path).map(ToOwned::to_owned),
        binary,
        hunks,
    })
}

/// Reset the thread worktree to its base commit. This is the explicit
/// "revert conversation to cursor" action: it discards every agent edit in
/// the worktree (tracked and untracked) without touching the project.
pub async fn revert_to_base(row: &ThreadRow) -> Result<(), ServerError> {
    let worktree = worktree(row)?;
    let reference = row
        .base_sha
        .clone()
        .unwrap_or_else(|| row.base_branch.clone());
    let reset = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["reset", "--hard", &reference])
        .output()
        .await
        .map_err(|error| ServerError::internal(format!("could not run git reset: {error}")))?;
    if !reset.status.success() {
        return Err(ServerError(ApiError::conflict(format!(
            "could not revert worktree: {}",
            String::from_utf8_lossy(&reset.stderr).trim()
        ))));
    }
    let clean = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["clean", "-fd"])
        .output()
        .await
        .map_err(|error| ServerError::internal(format!("could not run git clean: {error}")))?;
    if !clean.status.success() {
        return Err(ServerError(ApiError::conflict(format!(
            "could not clean worktree: {}",
            String::from_utf8_lossy(&clean.stderr).trim()
        ))));
    }
    Ok(())
}

pub async fn mark_seen(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
    paths: &[String],
    seen: bool,
) -> Result<(), ServerError> {
    let worktree = worktree(row)?;
    for path in paths {
        ensure_within(&worktree, Path::new(path)).map_err(|_| {
            ServerError(ApiError::new(
                threadbase_shared::ErrorCode::PathOutsideWorkspace,
                "file path is outside the thread worktree",
            ))
        })?;
        if seen {
            let hash = current_hash(&worktree, path).await;
            sqlx::query(
                "INSERT INTO seen_files(thread_id, user_id, path, content_hash, seen_at)
                 VALUES(?, ?, ?, ?, ?)
                 ON CONFLICT(thread_id, user_id, path) DO UPDATE SET
                    content_hash = excluded.content_hash, seen_at = excluded.seen_at",
            )
            .bind(&row.id)
            .bind(user.user.id.as_str())
            .bind(path)
            .bind(hash)
            .bind(now_ms())
            .execute(pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM seen_files WHERE thread_id = ? AND user_id = ? AND path = ?")
                .bind(&row.id)
                .bind(user.user.id.as_str())
                .bind(path)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn merge_back(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
    commit: bool,
    message: Option<&str>,
) -> Result<MergeBackResponse, ServerError> {
    let worktree = worktree(row)?;
    let project = crate::projects::row(pool, &row.project_id).await?;
    let project_root = PathBuf::from(&project.path);
    let changes = list(
        pool,
        row,
        user,
        &BaselineDto {
            kind: BaselineKind::BaseBranch,
            commit: None,
        },
    )
    .await?;
    let patch = git::worktree_patch(&worktree, row.base_sha.as_deref())
        .await
        .map_err(|error| {
            ServerError(ApiError::validation(format!(
                "could not build patch: {error}"
            )))
        })?;
    git::apply_patch(&project_root, &patch)
        .await
        .map_err(|error| {
            ServerError(ApiError::conflict(format!(
                "changes could not be applied cleanly: {error}"
            )))
        })?;
    let committed = if commit {
        let message = message
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Apply thread changes");
        git::commit_all(&project_root, message)
            .await
            .map_err(|error| ServerError(ApiError::conflict(format!("commit failed: {error}"))))?;
        true
    } else {
        false
    };
    let files: Vec<String> = changes.files.into_iter().map(|file| file.path).collect();
    crate::auth::audit(
        pool,
        Some(user.user.id.as_str()),
        "thread.merged_back",
        Some("thread"),
        Some(&row.id),
        Some(if committed {
            "committed"
        } else {
            "working_tree"
        }),
        None,
    )
    .await;
    Ok(MergeBackResponse {
        committed,
        commit: if committed {
            git::head_sha(&project_root).await.ok()
        } else {
            None
        },
        files,
    })
}

async fn resolve_baseline(
    worktree: &Path,
    row: &ThreadRow,
    requested: &BaselineDto,
) -> Result<ResolvedBaseline, ServerError> {
    let (reference, label) = match requested.kind {
        BaselineKind::BaseBranch => {
            let reference = row
                .base_sha
                .clone()
                .unwrap_or_else(|| row.base_branch.clone());
            (reference, row.base_branch.clone())
        }
        BaselineKind::LastCommit => ("HEAD".to_owned(), "Last commit".to_owned()),
        BaselineKind::LastTurn => {
            let reference = row
                .last_turn_sha
                .clone()
                .unwrap_or_else(|| "HEAD".to_owned());
            (reference, "Last turn".to_owned())
        }
        BaselineKind::Commit => {
            let commit = requested.commit.clone().ok_or_else(|| {
                ServerError(ApiError::validation(
                    "commit is required for a commit baseline",
                ))
            })?;
            let short: String = commit.chars().take(8).collect();
            (commit.clone(), short)
        }
    };
    git::rev_parse(worktree, &reference)
        .await
        .map_err(|_| ServerError(ApiError::validation("baseline commit was not found")))?;
    Ok(ResolvedBaseline {
        reference,
        baseline: BaselineDto {
            kind: requested.kind,
            commit: requested.commit.clone(),
        },
        label,
    })
}

async fn seen_hashes(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
) -> Result<BTreeMap<String, String>, ServerError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT path, content_hash FROM seen_files WHERE thread_id = ? AND user_id = ?",
    )
    .bind(&row.id)
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

async fn current_hash(worktree: &Path, path: &str) -> String {
    match tokio::fs::read(worktree.join(path)).await {
        Ok(bytes) => hash_bytes(&bytes),
        Err(_) => "deleted".to_owned(),
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

async fn file_line_stats(path: &Path) -> (bool, u32) {
    let Ok(content) = tokio::fs::read(path).await else {
        return (false, 0);
    };
    if content.contains(&0) {
        return (true, 0);
    }
    let lines = content.iter().filter(|byte| **byte == b'\n').count();
    (false, u32::try_from(lines).unwrap_or(0))
}

async fn is_binary_file(path: &Path) -> bool {
    let Ok(content) = tokio::fs::read(path).await else {
        return false;
    };
    content.contains(&0)
}

async fn file_status(worktree: &Path, reference: &str, path: &str) -> FileStatus {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", "--name-status", reference, "--", path])
        .output()
        .await;
    let Ok(output) = output else {
        return FileStatus::Modified;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let status = text.split('\t').next().unwrap_or("M");
    if status.starts_with('R') {
        FileStatus::Renamed
    } else {
        parse_status(status)
    }
}

fn parse_status(value: &str) -> FileStatus {
    match value.chars().next().unwrap_or('M') {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        '?' => FileStatus::Untracked,
        _ => FileStatus::Modified,
    }
}

fn parse_hunks(path: &str, patch: &str) -> Vec<DiffHunkDto> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunkDto> = None;
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    for line in patch.lines() {
        if line.starts_with("@@") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let Some((old_start, old_lines, new_start, new_lines)) = parse_hunk_header(line) else {
                continue;
            };
            old_line = old_start;
            new_line = new_start;
            current = Some(DiffHunkDto {
                id: format!("h{}", hunks.len() + 1),
                header: line.to_owned(),
                context_hash: String::new(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        if line.starts_with("\\ No newline") {
            continue;
        }
        let (kind, content) = if let Some(rest) = line.strip_prefix('+') {
            (DiffLineKind::Add, rest)
        } else if let Some(rest) = line.strip_prefix('-') {
            (DiffLineKind::Delete, rest)
        } else if let Some(rest) = line.strip_prefix(' ') {
            (DiffLineKind::Context, rest)
        } else {
            continue;
        };
        let (old, new) = match kind {
            DiffLineKind::Add => {
                let value = Some(new_line);
                new_line += 1;
                (None, value)
            }
            DiffLineKind::Delete => {
                let value = Some(old_line);
                old_line += 1;
                (value, None)
            }
            DiffLineKind::Context => {
                let old = Some(old_line);
                let new = Some(new_line);
                old_line += 1;
                new_line += 1;
                (old, new)
            }
        };
        hunk.lines.push(DiffLineDto {
            kind,
            old_line: old,
            new_line: new,
            content: content.to_owned(),
        });
    }
    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }
    for hunk in &mut hunks {
        let mut value = format!("{path}:{}", hunk.header);
        for line in &hunk.lines {
            value.push('\n');
            value.push_str(&line.content);
        }
        hunk.context_hash = hash_text(&value)[..16].to_owned();
    }
    hunks
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let line = line.strip_prefix("@@")?;
    let line = line.split("@@").next()?.trim();
    let mut parts = line.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let (old_start, old_lines) = parse_range(old)?;
    let (new_start, new_lines) = parse_range(new)?;
    Some((old_start, old_lines, new_start, new_lines))
}

fn parse_range(value: &str) -> Option<(u32, u32)> {
    match value.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((value.parse().ok()?, 1)),
    }
}

fn language_for(path: &str) -> Option<&'static str> {
    let extension = Path::new(path).extension()?.to_str()?;
    Some(match extension {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "json" => "json",
        "md" => "markdown",
        "css" => "css",
        "html" => "html",
        "py" => "python",
        "go" => "go",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shell",
        _ => return None,
    })
}

fn worktree(row: &ThreadRow) -> Result<PathBuf, ServerError> {
    row.worktree_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .ok_or_else(|| ServerError(ApiError::conflict("thread worktree is not available")))
}

async fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, ServerError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await
        .map_err(|error| ServerError::internal(format!("git failed: {error}")))?;
    if !output.status.success() {
        return Err(ServerError(ApiError::validation(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
