//! Git diff baselines, per-file review, Seen state and merge-back.

mod history;
mod parse;

pub use history::history;

use parse::{
    file_line_stats, hash_bytes, hash_text, is_binary_bytes, is_binary_file, parse_hunks,
    parse_status,
};

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::threads::ThreadRow;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap};
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

/// A worktree can contain thousands of untracked build artifacts. The panel
/// is virtualized, but shipping an unbounded file list every poll is still a
/// waste; stats keep counting everything.
const MAX_CHANGE_FILES: usize = 1_000;

pub async fn list(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
    baseline: &BaselineDto,
) -> Result<ChangesDto, ServerError> {
    let worktree = worktree(row)?;
    let resolved = resolve_baseline(&worktree, row, baseline).await?;
    let mut files = collect_files(&worktree, &resolved.reference, None).await?;
    let seen_map = seen_hashes(pool, row, user, &resolved.reference).await?;
    let mut seen_count = 0;
    for file in files.values_mut() {
        // Hashing a changed file is only needed when the user has actually
        // marked it seen; the common case has an empty seen map.
        let Some(stored) = seen_map.get(&file.path) else {
            continue;
        };
        let current = current_hash(&worktree, &file.path).await;
        if ReadRules::seen(Some(stored), &current) {
            file.seen = true;
            seen_count += 1;
        } else {
            sqlx::query(
                "DELETE FROM seen_files WHERE thread_id = ? AND user_id = ? AND path = ? AND baseline = ?",
            )
            .bind(&row.id)
            .bind(user.user.id.as_str())
            .bind(&file.path)
            .bind(&resolved.reference)
            .execute(pool)
            .await?;
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
    let truncated = files.len() > MAX_CHANGE_FILES;
    Ok(ChangesDto {
        baseline: resolved.baseline,
        base_label: resolved.label,
        files: files.into_values().take(MAX_CHANGE_FILES).collect(),
        stats,
        commit: row.base_sha.clone(),
        truncated,
    })
}

/// Changed files between two revisions, or from `from` to the working tree
/// when `to` is `None`. Untracked files only exist on the working-tree side, so
/// commit ranges never look at `ls-files --others`.
async fn collect_files(
    worktree: &Path,
    from: &str,
    to: Option<&str>,
) -> Result<BTreeMap<String, FileChangeDto>, ServerError> {
    let mut numstat_args = vec!["diff", "--numstat", from];
    let mut status_args = vec!["diff", "--name-status", from];
    if let Some(to) = to {
        numstat_args.push(to);
        status_args.push(to);
    }
    let numstat = git_stdout(worktree, &numstat_args).await?;
    let name_status = git_stdout(worktree, &status_args).await?;
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
    if to.is_none() {
        let untracked = git_stdout(worktree, &["ls-files", "--others", "--exclude-standard"])
            .await
            .unwrap_or_default();
        for path in untracked.lines().filter(|line| !line.trim().is_empty()) {
            let full = ensure_within(worktree, Path::new(path)).map_err(|_| {
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
    }
    Ok(files)
}

fn stats_of(files: &BTreeMap<String, FileChangeDto>) -> ChangeStats {
    let mut additions = 0;
    let mut deletions = 0;
    for file in files.values() {
        additions += file.additions;
        deletions += file.deletions;
    }
    ChangeStats {
        files: u32::try_from(files.len()).unwrap_or(0),
        additions,
        deletions,
        seen: 0,
        total: u32::try_from(files.len()).unwrap_or(0),
    }
}

/// The two refs a turn spans: its own checkpoint, and the next one (the worktree
/// for the newest turn).
pub async fn turn_span(
    pool: &SqlitePool,
    row: &ThreadRow,
    turn_id: &str,
) -> Result<(String, Option<String>), ServerError> {
    let start: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT sha, created_at, rowid FROM checkpoints WHERE thread_id = ? AND turn_id = ?",
    )
    .bind(&row.id)
    .bind(turn_id)
    .fetch_optional(pool)
    .await?;
    let Some((from, created_at, rowid)) = start else {
        return Err(ServerError(ApiError::conflict(
            "this turn has no worktree checkpoint",
        )));
    };
    let next: Option<String> = sqlx::query_scalar(
        "SELECT sha FROM checkpoints
          WHERE thread_id = ? AND (created_at > ? OR (created_at = ? AND rowid > ?))
          ORDER BY created_at ASC, rowid ASC LIMIT 1",
    )
    .bind(&row.id)
    .bind(created_at)
    .bind(created_at)
    .bind(rowid)
    .fetch_optional(pool)
    .await?;
    Ok((from, next))
}

/// What one turn changed, from its checkpoint to the next one (or the worktree
/// when it is the latest turn). Uncommitted edits of the newest turn are
/// included because the span ends at the worktree.
pub async fn turn_changes(
    pool: &SqlitePool,
    row: &ThreadRow,
    turn_id: &str,
) -> Result<ChangesDto, ServerError> {
    let worktree = worktree(row)?;
    let (from, to) = turn_span(pool, row, turn_id).await?;
    let files = collect_files(&worktree, &from, to.as_deref()).await?;
    let stats = stats_of(&files);
    let truncated = files.len() > MAX_CHANGE_FILES;
    Ok(ChangesDto {
        baseline: BaselineDto {
            kind: BaselineKind::Turn,
            commit: Some(from),
        },
        base_label: "this turn".to_owned(),
        files: files.into_values().take(MAX_CHANGE_FILES).collect(),
        stats,
        commit: None,
        truncated,
    })
}

pub async fn turn_file_diff(
    pool: &SqlitePool,
    row: &ThreadRow,
    turn_id: &str,
    path: &str,
) -> Result<FileDiffDto, ServerError> {
    let worktree = worktree(row)?;
    let (from, to) = turn_span(pool, row, turn_id).await?;
    file_diff_between(&worktree, &from, to.as_deref(), path).await
}

/// A commit's own changes: first parent to the commit. Root commits diff against
/// the empty tree.
pub async fn commit_changes(row: &ThreadRow, requested: &str) -> Result<ChangesDto, ServerError> {
    let worktree = worktree(row)?;
    let sha = resolve_commit(&worktree, requested).await?;
    let parent = parent_reference(&worktree, &sha).await;
    let files = collect_files(&worktree, &parent, Some(&sha)).await?;
    let stats = stats_of(&files);
    let truncated = files.len() > MAX_CHANGE_FILES;
    let short: String = sha.chars().take(8).collect();
    Ok(ChangesDto {
        baseline: BaselineDto {
            kind: BaselineKind::Commit,
            commit: Some(sha.clone()),
        },
        base_label: short,
        files: files.into_values().take(MAX_CHANGE_FILES).collect(),
        stats,
        commit: Some(sha),
        truncated,
    })
}

pub async fn commit_file_diff(
    row: &ThreadRow,
    requested: &str,
    path: &str,
) -> Result<FileDiffDto, ServerError> {
    let worktree = worktree(row)?;
    let sha = resolve_commit(&worktree, requested).await?;
    let parent = parent_reference(&worktree, &sha).await;
    let full = ensure_within(&worktree, Path::new(path)).map_err(|_| {
        ServerError(ApiError::new(
            threadbase_shared::ErrorCode::PathOutsideWorkspace,
            "file path is outside the thread worktree",
        ))
    })?;
    let patch = git_stdout(
        &worktree,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-U3",
            &parent,
            &sha,
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
    let status = file_status_between(&worktree, &parent, Some(&sha), path).await;
    Ok(FileDiffDto {
        path: path.to_owned(),
        old_path: None,
        status,
        binary,
        hunks,
    })
}

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

async fn parent_reference(worktree: &Path, sha: &str) -> String {
    git_stdout(worktree, &["rev-parse", &format!("{sha}^")])
        .await
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|_| EMPTY_TREE.to_owned())
}

/// Commit shas arrive in a URL, so only accept a hex object name before
/// handing it to git. Full and abbreviated shas both work.
async fn resolve_commit(worktree: &Path, requested: &str) -> Result<String, ServerError> {
    let valid =
        (7..=64).contains(&requested.len()) && requested.chars().all(|ch| ch.is_ascii_hexdigit());
    if !valid {
        return Err(ServerError(ApiError::validation("invalid commit id")));
    }
    git::rev_parse(worktree, requested)
        .await
        .map_err(|_| ServerError(ApiError::validation("commit was not found")))
}

pub async fn file_diff(
    row: &ThreadRow,
    baseline: &BaselineDto,
    path: &str,
) -> Result<FileDiffDto, ServerError> {
    let worktree = worktree(row)?;
    let resolved = resolve_baseline(&worktree, row, baseline).await?;
    file_diff_between(&worktree, &resolved.reference, None, path).await
}

/// Diff one file between two refs. `to = None` means the worktree, which is the
/// only case where an untracked file can appear.
async fn file_diff_between(
    worktree: &Path,
    from: &str,
    to: Option<&str>,
    path: &str,
) -> Result<FileDiffDto, ServerError> {
    let worktree = worktree.to_path_buf();
    let full = ensure_within(&worktree, Path::new(path)).map_err(|_| {
        ServerError(ApiError::new(
            threadbase_shared::ErrorCode::PathOutsideWorkspace,
            "file path is outside the thread worktree",
        ))
    })?;
    let untracked = to.is_none()
        && git_stdout(&worktree, &["ls-files", "--others", "--exclude-standard"])
            .await
            .unwrap_or_default()
            .lines()
            .any(|line| line == path);
    if untracked {
        let content = tokio::fs::read(&full).await.unwrap_or_default();
        return Ok(FileDiffDto {
            path: path.to_owned(),
            old_path: None,
            status: FileStatus::Untracked,
            binary: is_binary_bytes(&content),
            hunks: untracked_hunks(path, &content),
        });
    }
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-U3", from];
    if let Some(to) = to {
        args.push(to);
    }
    args.push("--");
    args.push(path);
    let patch = git_stdout(&worktree, &args).await?;
    let binary = patch.contains("Binary files") || is_binary_file(&full).await;
    let hunks = if binary {
        Vec::new()
    } else {
        parse_hunks(path, &patch)
    };
    let status = file_status(&worktree, from, path).await;
    Ok(FileDiffDto {
        path: path.to_owned(),
        old_path: None,
        status,
        binary,
        hunks,
    })
}

/// Untracked files have no base version, so they are presented as one synthetic
/// hunk. Anchor reconciliation reuses this definition to keep the same context
/// hash and hunk id that `file_diff` exposes to clients.
fn untracked_hunks(path: &str, content: &[u8]) -> Vec<DiffHunkDto> {
    if is_binary_bytes(content) {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(content);
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
        return Vec::new();
    }
    vec![DiffHunkDto {
        id: "h1".to_owned(),
        header: format!("@@ -0,0 +1,{} @@", lines.len()),
        context_hash: hash_text(&format!("untracked:{path}")),
        old_start: 0,
        old_lines: 0,
        new_start: 1,
        new_lines: u32::try_from(lines.len()).unwrap_or(u32::MAX),
        lines,
    }]
}

/// The worktree and base-branch reference used to reconcile diff comment
/// anchors. Both come from the thread row, so resolving this costs no git
/// subprocess; `list_for_thread` builds it once for all anchored paths.
pub(crate) struct AnchorBase {
    worktree: PathBuf,
    reference: String,
}

pub(crate) fn anchor_base(row: &ThreadRow) -> Result<AnchorBase, ServerError> {
    Ok(AnchorBase {
        worktree: worktree(row)?,
        reference: base_reference(row),
    })
}

/// Map each current hunk's context hash to its positional id. Comment anchors
/// only need hunk identity, so this spends one `git diff` and probes untracked
/// status only when that diff is empty.
pub(crate) async fn hunk_index(
    base: &AnchorBase,
    path: &str,
) -> Result<HashMap<String, String>, ServerError> {
    let full = ensure_within(&base.worktree, Path::new(path)).map_err(|_| {
        ServerError(ApiError::new(
            threadbase_shared::ErrorCode::PathOutsideWorkspace,
            "file path is outside the thread worktree",
        ))
    })?;
    let patch = git_stdout(
        &base.worktree,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-U3",
            &base.reference,
            "--",
            path,
        ],
    )
    .await?;
    if !patch.is_empty() {
        return Ok(hunk_index_of(parse_hunks(path, &patch)));
    }
    let untracked = git_stdout(
        &base.worktree,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .await
    .unwrap_or_default()
    .lines()
    .any(|line| line == path);
    if !untracked {
        return Ok(HashMap::new());
    }
    let content = tokio::fs::read(&full).await.unwrap_or_default();
    Ok(hunk_index_of(untracked_hunks(path, &content)))
}

fn hunk_index_of(hunks: impl IntoIterator<Item = DiffHunkDto>) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for hunk in hunks {
        index.entry(hunk.context_hash).or_insert(hunk.id);
    }
    index
}

/// Reset the thread worktree to its base commit. This is the explicit
/// "revert conversation to cursor" action: it discards every agent edit in
/// the worktree (tracked and untracked) without touching the project.
pub async fn revert_to_base(row: &ThreadRow) -> Result<(), ServerError> {
    let reference = row
        .base_sha
        .clone()
        .unwrap_or_else(|| row.base_branch.clone());
    reset_worktree(row, &reference).await
}

/// Hard-reset the thread worktree to `reference` and remove untracked files.
///
/// Shared by "revert this thread" and "rewind to a turn": both mean the same
/// thing to the filesystem, so there is one implementation of "put the worktree
/// back where it was" and one place that reports why git refused.
pub async fn reset_worktree(row: &ThreadRow, reference: &str) -> Result<(), ServerError> {
    let worktree = worktree(row)?;
    let reset = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["reset", "--hard", reference])
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
    baseline: &BaselineDto,
) -> Result<(), ServerError> {
    let worktree = worktree(row)?;
    let resolved = resolve_baseline(&worktree, row, baseline).await?;
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
                "INSERT INTO seen_files(thread_id, user_id, path, content_hash, baseline, seen_at)
                 VALUES(?, ?, ?, ?, ?, ?)
                 ON CONFLICT(thread_id, user_id, path, baseline) DO UPDATE SET
                    content_hash = excluded.content_hash, seen_at = excluded.seen_at",
            )
            .bind(&row.id)
            .bind(user.user.id.as_str())
            .bind(path)
            .bind(hash)
            .bind(&resolved.reference)
            .bind(now_ms())
            .execute(pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM seen_files WHERE thread_id = ? AND user_id = ? AND path = ? AND baseline = ?")
                .bind(&row.id)
                .bind(user.user.id.as_str())
                .bind(path)
                .bind(&resolved.reference)
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
    let branch = git::current_branch(&project_root).await.map_err(|error| {
        ServerError::internal(format!("could not read current branch: {error}"))
    })?;
    if branch != row.base_branch && branch != project.default_branch {
        return Err(ServerError(ApiError::conflict(format!(
            "check out `{}` (or `{}`) before applying this thread; the current branch is `{branch}`",
            row.base_branch, project.default_branch
        ))));
    }
    let status = git::status_porcelain(&project_root)
        .await
        .map_err(|error| {
            ServerError::internal(format!("could not inspect the worktree: {error}"))
        })?;
    if !status.trim().is_empty() {
        let preview: String = status.lines().take(6).collect::<Vec<_>>().join("\n");
        return Err(ServerError(ApiError::conflict(format!(
            "commit or stash the project changes before applying this thread:\n{preview}"
        ))));
    }
    let base_drift = match (
        row.base_sha.as_deref(),
        git::rev_parse(&project_root, &row.base_branch).await,
    ) {
        (Some(base), Ok(tip)) => tip != base,
        _ => false,
    };
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
    if patch.trim().is_empty() {
        return Ok(MergeBackResponse {
            committed: false,
            commit: None,
            files: Vec::new(),
            branch,
            base_drift,
            verified: false,
        });
    }
    let verified = if let Some(command) = project
        .verify_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        run_verify(&worktree, command).await?;
        true
    } else {
        false
    };
    let head_before = git::head_sha(&project_root)
        .await
        .map_err(|error| ServerError::internal(format!("could not read HEAD: {error}")))?;
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
        match git::commit_all(&project_root, message).await {
            Ok(_) => true,
            Err(error) => {
                // The tree was clean before the patch, so restoring HEAD is
                // safe and prevents a half-applied merge.
                let _ = git::reset_hard(&project_root, &head_before).await;
                let _ = git::clean_untracked(&project_root).await;
                return Err(ServerError(ApiError::conflict(format!(
                    "commit failed; the project was restored: {error}"
                ))));
            }
        }
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
        branch,
        base_drift,
        verified,
    })
}

/// Run the project's verify command in the thread worktree. The command is
/// deliberately run there, before the patch reaches the project tree.
async fn run_verify(worktree: &Path, command: &str) -> Result<(), ServerError> {
    let mut cmd = Command::new("sh");
    cmd.arg("-lc")
        .arg(command)
        .current_dir(worktree)
        .kill_on_drop(true);
    let output = match tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output()).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(ServerError::internal(format!(
                "could not run the verify command: {error}"
            )))
        }
        Err(_) => {
            return Err(ServerError(ApiError::conflict(
                "the verify command timed out after 5 minutes",
            )))
        }
    };
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let total = combined.chars().count();
    let combined: String = combined.chars().skip(total.saturating_sub(4000)).collect();
    Err(ServerError(ApiError::conflict(format!(
        "the verify command failed ({}):\n{}",
        output.status,
        combined.trim()
    ))))
}

async fn resolve_baseline(
    worktree: &Path,
    row: &ThreadRow,
    requested: &BaselineDto,
) -> Result<ResolvedBaseline, ServerError> {
    let (reference, label) = match requested.kind {
        BaselineKind::BaseBranch => (base_reference(row), row.base_branch.clone()),
        BaselineKind::LastCommit => ("HEAD".to_owned(), "Last commit".to_owned()),
        BaselineKind::LastTurn => {
            let reference = row
                .last_turn_sha
                .clone()
                .unwrap_or_else(|| "HEAD".to_owned());
            (reference, "Last turn".to_owned())
        }
        // A turn span is resolved by the turn endpoints: it needs both ends,
        // and a `BaselineDto` cannot carry the second one.
        BaselineKind::Turn => {
            return Err(ServerError(ApiError::validation(
                "a turn span is not a single reference; use the turn endpoint",
            )))
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

async fn file_status(worktree: &Path, reference: &str, path: &str) -> FileStatus {
    file_status_between(worktree, reference, None, path).await
}

async fn file_status_between(
    worktree: &Path,
    from: &str,
    to: Option<&str>,
    path: &str,
) -> FileStatus {
    let mut args = vec!["diff", "--name-status", from];
    if let Some(to) = to {
        args.push(to);
    }
    args.extend(["--", path]);
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(&args)
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

async fn seen_hashes(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
    baseline: &str,
) -> Result<BTreeMap<String, String>, ServerError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT path, content_hash FROM seen_files WHERE thread_id = ? AND user_id = ? AND baseline = ?",
    )
    .bind(&row.id)
    .bind(user.user.id.as_str())
    .bind(baseline)
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

fn base_reference(row: &ThreadRow) -> String {
    row.base_sha
        .clone()
        .unwrap_or_else(|| row.base_branch.clone())
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
