//! Path containment for every file operation exposed through the API.

use path_clean::PathClean;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("path contains a NUL byte")]
    NulByte,
    #[error("path escapes its workspace root: {0}")]
    Outside(String),
    #[error("workspace root is unavailable: {0}")]
    RootUnavailable(String),
    #[error("failed to resolve path: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolve `candidate` against `root` and guarantee the result stays inside
/// the canonical root, including through symlinks.
pub fn ensure_within(root: &Path, candidate: &Path) -> Result<PathBuf, PathError> {
    if candidate.to_string_lossy().contains('\0') {
        return Err(PathError::NulByte);
    }
    let root = root
        .canonicalize()
        .map_err(|_| PathError::RootUnavailable(root.display().to_string()))?;
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    if climbs_above_start(candidate) {
        return Err(PathError::Outside(joined.display().to_string()));
    }
    let normalized = clean_path(&joined);
    let resolved = resolve_existing_ancestor(&normalized)?;
    if resolved.starts_with(&root) {
        Ok(normalized)
    } else {
        Err(PathError::Outside(normalized.display().to_string()))
    }
}

/// Lexically clean a path without touching the filesystem: `.` segments are
/// dropped and `..` segments are resolved, as in `path_clean::PathClean`.
/// Symlinks are resolved separately by [`ensure_within`].
pub fn clean_path(path: &Path) -> PathBuf {
    path.clean()
}

/// A path climbs above its start when a `..` prefix would pop more components
/// than were traversed, even if a later component descends back inside. A
/// rooted path cannot pop its root directory.
fn climbs_above_start(path: &Path) -> bool {
    let floor = usize::from(path.has_root());
    let mut depth = floor;
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(_) => depth += 1,
            Component::ParentDir => {
                if depth <= floor {
                    return true;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, PathError> {
    let mut current = path.to_path_buf();
    let mut suffix = Vec::new();
    loop {
        match current.canonicalize() {
            Ok(mut canonical) => {
                for part in suffix.iter().rev() {
                    canonical.push(part);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = current
                    .file_name()
                    .ok_or_else(|| PathError::Outside(path.display().to_string()))?;
                suffix.push(name.to_os_string());
                if !current.pop() {
                    return Err(PathError::Outside(path.display().to_string()));
                }
            }
            Err(error) => return Err(PathError::Io(error)),
        }
    }
}

/// Ensure a path is inside at least one of the allowed roots.
pub fn ensure_within_any(roots: &[PathBuf], candidate: &Path) -> Result<PathBuf, PathError> {
    let mut last_error = None;
    for root in roots {
        match ensure_within(root, candidate) {
            Ok(path) => return Ok(path),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| PathError::Outside(candidate.display().to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_stay_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        let path = ensure_within(&root, Path::new("src/main.rs")).unwrap();
        assert!(path.starts_with(&root));
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let error = ensure_within(&root, Path::new("../../etc/passwd")).unwrap_err();
        assert!(matches!(error, PathError::Outside(_)));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let error = ensure_within(&root, Path::new("link/secret.txt")).unwrap_err();
        assert!(matches!(error, PathError::Outside(_)));
    }

    #[test]
    fn parent_above_root_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let error = ensure_within(&root, Path::new("../root/file.txt")).unwrap_err();
        assert!(matches!(error, PathError::Outside(_)));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_ancestor_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("src/vendor")).unwrap();
        let error = ensure_within(&root, Path::new("src/vendor/lib.rs")).unwrap_err();
        assert!(matches!(error, PathError::Outside(_)));
    }

    #[test]
    fn non_existing_files_resolve_through_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = ensure_within(&root, Path::new("new-dir/new-file.txt")).unwrap();
        assert_eq!(path, root.join("new-dir/new-file.txt"));
    }
}
