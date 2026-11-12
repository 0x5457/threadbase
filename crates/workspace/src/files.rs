//! Read-only workspace file browsing.

use crate::path::{ensure_within, PathError};
use std::path::Path;
use std::time::UNIX_EPOCH;
use threadbase_shared::{FileContentDto, FileEntryDto};

const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("path is not a directory")]
    NotADirectory,
    #[error("path is not a file")]
    NotAFile,
    #[error("file is too large to display")]
    TooLarge,
}

pub fn list(root: &Path, relative: &str) -> Result<Vec<FileEntryDto>, FileError> {
    let relative = if relative.trim().is_empty() {
        "."
    } else {
        relative.trim_start_matches('/')
    };
    let directory = ensure_within(root, Path::new(relative))?;
    if !directory.is_dir() {
        return Err(FileError::NotADirectory);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if ignored(&name) {
            continue;
        }
        let metadata = entry.metadata()?;
        let path = entry.path();
        let path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_millis()).ok());
        entries.push(FileEntryDto {
            name,
            path,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified_at,
        });
    }
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn read(root: &Path, relative: &str) -> Result<FileContentDto, FileError> {
    let path = ensure_within(root, Path::new(relative.trim_start_matches('/')))?;
    if !path.is_file() {
        return Err(FileError::NotAFile);
    }
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(FileError::TooLarge);
    }
    let bytes = std::fs::read(&path)?;
    let truncated = bytes.contains(&0);
    let content = if truncated {
        "(binary file)".to_owned()
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    Ok(FileContentDto {
        path: relative.to_owned(),
        content,
        truncated,
        language: language_for(&path).map(ToOwned::to_owned),
    })
}

fn ignored(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".next" | ".DS_Store"
    )
}

fn language_for(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
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

/// Recursively finds files whose path contains `query` (case-insensitive).
/// Hidden directories and build outputs are skipped, and the scan is capped so
/// a huge repository cannot stall the composer.
pub fn search(root: &Path, query: &str, limit: usize) -> Result<Vec<String>, FileError> {
    let needle = query.trim().to_lowercase();
    let mut results = Vec::new();
    let mut visited = 0usize;
    collect_files(root, root, &needle, limit, &mut visited, 0, &mut results)?;
    results.sort_by(|left, right| {
        rank(left, &needle)
            .cmp(&rank(right, &needle))
            .then_with(|| left.len().cmp(&right.len()))
            .then_with(|| left.cmp(right))
    });
    results.truncate(limit);
    Ok(results)
}

fn rank(path: &str, needle: &str) -> u8 {
    if needle.is_empty() {
        return 2;
    }
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    if name.starts_with(needle) {
        0
    } else if name.contains(needle) {
        1
    } else {
        2
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    needle: &str,
    limit: usize,
    visited: &mut usize,
    depth: usize,
    results: &mut Vec<String>,
) -> Result<(), FileError> {
    if results.len() >= limit || *visited > 20_000 || depth > 16 {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(directory)?.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if results.len() >= limit || *visited > 20_000 {
            return Ok(());
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || ignored(&name) {
            continue;
        }
        *visited += 1;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, needle, limit, visited, depth + 1, results)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if needle.is_empty() || relative.to_lowercase().contains(needle) {
            results.push(relative);
        }
    }
    Ok(())
}
