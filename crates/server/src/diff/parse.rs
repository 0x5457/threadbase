//! Pure diff parsing: hunks, line numbers, status and language names.

use sha2::{Digest, Sha256};
use std::path::Path;
use threadbase_shared::{DiffHunkDto, DiffLineDto, DiffLineKind, FileStatus};

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

pub(super) fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

pub(super) async fn file_line_stats(path: &Path) -> (bool, u32) {
    let Ok(content) = tokio::fs::read(path).await else {
        return (false, 0);
    };
    if content.contains(&0) {
        return (true, 0);
    }
    let lines = content.iter().filter(|byte| **byte == b'\n').count();
    (false, u32::try_from(lines).unwrap_or(0))
}

pub(super) async fn is_binary_file(path: &Path) -> bool {
    let Ok(content) = tokio::fs::read(path).await else {
        return false;
    };
    content.contains(&0)
}

pub(super) fn parse_status(value: &str) -> FileStatus {
    match value.chars().next().unwrap_or('M') {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        '?' => FileStatus::Untracked,
        _ => FileStatus::Modified,
    }
}

pub(super) fn parse_hunks(path: &str, patch: &str) -> Vec<DiffHunkDto> {
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

pub(super) fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let line = line.strip_prefix("@@")?;
    let line = line.split("@@").next()?.trim();
    let mut parts = line.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let (old_start, old_lines) = parse_range(old)?;
    let (new_start, new_lines) = parse_range(new)?;
    Some((old_start, old_lines, new_start, new_lines))
}

pub(super) fn parse_range(value: &str) -> Option<(u32, u32)> {
    match value.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((value.parse().ok()?, 1)),
    }
}

pub(super) fn language_for(path: &str) -> Option<&'static str> {
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
