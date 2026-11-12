//! Read-only workspace file browsing.

use crate::path::{ensure_within, PathError};
use ignore::WalkBuilder;
use std::path::Path;
use std::time::UNIX_EPOCH;
use threadbase_shared::{FileContentDto, FileEntryDto};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_SEARCH_ENTRIES: usize = 20_000;
/// The walker counts the root as depth 0, so depth 17 is the deepest file the
/// previous directory recursion visited.
const MAX_SEARCH_DEPTH: usize = 17;
const IGNORED_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".DS_Store",
];

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("failed to walk workspace: {0}")]
    Walk(String),
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
    let truncated = content_inspector::inspect(&bytes).is_binary();
    let content = if truncated {
        "(binary file)".to_owned()
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    Ok(FileContentDto {
        path: relative.to_owned(),
        content,
        truncated,
        language: None,
    })
}

fn ignored(name: &str) -> bool {
    IGNORED_NAMES.contains(&name)
}

/// Recursively finds files whose path contains `query` (case-insensitive).
/// Hidden files, gitignored files and build outputs are skipped, and the scan
/// is capped so a huge repository cannot stall the composer.
pub fn search(root: &Path, query: &str, limit: usize) -> Result<Vec<String>, FileError> {
    let needle = query.trim().to_lowercase();
    let mut results = Vec::new();
    let mut visited = 0usize;
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .max_depth(Some(MAX_SEARCH_DEPTH))
        .filter_entry(|entry| entry.depth() == 0 || !ignored(&entry.file_name().to_string_lossy()))
        .build();
    for entry in walker {
        let entry = entry.map_err(|error| FileError::Walk(error.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        if visited >= MAX_SEARCH_ENTRIES {
            break;
        }
        visited += 1;
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if needle.is_empty() || relative.to_lowercase().contains(&needle) {
            results.push(relative);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_skips_hidden_ignored_and_gitignored_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir(root.join("generated")).unwrap();
        std::fs::write(root.join(".gitignore"), "generated\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        std::fs::write(root.join("node_modules/pkg/index.rs"), "").unwrap();
        std::fs::write(root.join(".hidden/secret.rs"), "").unwrap();
        std::fs::write(root.join(".dotfile.rs"), "").unwrap();
        std::fs::write(root.join("generated/out.rs"), "").unwrap();
        let results = search(root, "rs", 20).unwrap();
        assert_eq!(results, vec!["src/main.rs"]);
    }

    #[test]
    fn search_ranks_file_name_matches_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("deep/nested")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("deep/nested/main.rs"), "").unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        std::fs::write(root.join("src/domain.rs"), "").unwrap();
        let results = search(root, "main", 10).unwrap();
        assert_eq!(
            results,
            vec!["src/main.rs", "deep/nested/main.rs", "src/domain.rs"]
        );
    }

    #[test]
    fn read_flags_binary_content_and_leaves_language_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("bin.dat"), b"hello\0world").unwrap();
        std::fs::write(root.join("text.txt"), b"hello world").unwrap();
        let binary = read(root, "bin.dat").unwrap();
        assert!(binary.truncated);
        assert_eq!(binary.language, None);
        let text = read(root, "text.txt").unwrap();
        assert!(!text.truncated);
        assert_eq!(text.content, "hello world");
        assert_eq!(text.language, None);
    }
}
