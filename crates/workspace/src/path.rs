//! Path containment for every file operation exposed through the API.

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
    let normalized = normalize(&joined)?;
    let resolved = resolve_existing_ancestor(&normalized)?;
    if resolved.starts_with(&root) {
        Ok(normalized)
    } else {
        Err(PathError::Outside(normalized.display().to_string()))
    }
}

/// The lexical component walk rejects `..` that would climb above the join
/// point. Existing ancestors are canonicalized so symlinks cannot escape.
fn normalize(path: &Path) -> Result<PathBuf, PathError> {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return Err(PathError::Outside(path.display().to_string()));
                }
            }
            Component::Normal(part) => output.push(part),
        }
    }
    Ok(output)
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

/// Render a user supplied relative path without allowing hidden traversal.
#[must_use]
pub fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
    fn non_existing_files_resolve_through_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = ensure_within(&root, Path::new("new-dir/new-file.txt")).unwrap();
        assert_eq!(path, root.join("new-dir/new-file.txt"));
    }
}
