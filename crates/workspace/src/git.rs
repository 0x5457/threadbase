//! Git and worktree operations. The system `git` binary is the source of
//! truth; Threadbase never reimplements git object handling.

use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotARepository(String),
    #[error("invalid branch name: {0}")]
    InvalidBranch(String),
    #[error("git command failed: {command}\n{stderr}")]
    Command { command: String, stderr: String },
    #[error("git operation failed: {0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInfo {
    pub root: PathBuf,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
    pub head_sha: String,
    pub clean: bool,
}

async fn git(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(GitError::Command {
            command: format!("git {} {}", cwd.display(), args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

async fn git_raw(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(GitError::Command {
            command: format!("git {} {}", cwd.display(), args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

async fn git_allow_failure(cwd: &Path, args: &[&str]) -> Result<Option<String>, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ))
    } else {
        Ok(None)
    }
}

/// Inspect a directory and return repository metadata used by the project picker.
pub async fn inspect(path: &Path) -> Result<GitInfo, GitError> {
    let path = path
        .canonicalize()
        .map_err(|_| GitError::NotARepository(path.display().to_string()))?;
    let root = git(&path, &["rev-parse", "--show-toplevel"])
        .await
        .map_err(|_| GitError::NotARepository(path.display().to_string()))?;
    let root = PathBuf::from(root);
    let remote_url = git_allow_failure(&root, &["remote", "get-url", "origin"])
        .await?
        .filter(|value| !value.is_empty());
    let branch = git_allow_failure(&root, &["branch", "--show-current"])
        .await?
        .filter(|value| !value.is_empty());
    let head_sha = git(&root, &["rev-parse", "HEAD"]).await?;
    let status = git(&root, &["status", "--porcelain"]).await?;
    Ok(GitInfo {
        root,
        remote_url,
        branch,
        head_sha,
        clean: status.is_empty(),
    })
}

pub async fn validate_branch(root: &Path, branch: &str) -> Result<(), GitError> {
    let output = git_allow_failure(root, &["check-ref-format", "--branch", branch]).await?;
    if output.is_some() {
        Ok(())
    } else {
        Err(GitError::InvalidBranch(branch.to_owned()))
    }
}

pub async fn branches(root: &Path) -> Result<Vec<String>, GitError> {
    let output = git(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .await?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub async fn remote_branch(root: &Path) -> Result<Option<String>, GitError> {
    let head = git_allow_failure(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .await?;
    Ok(head.filter(|value| !value.is_empty()))
}

pub async fn head_sha(root: &Path) -> Result<String, GitError> {
    git(root, &["rev-parse", "HEAD"]).await
}

pub async fn rev_parse(root: &Path, reference: &str) -> Result<String, GitError> {
    git(root, &["rev-parse", "--verify", reference]).await
}

/// Create a thread worktree from `base_ref` on a new branch.
pub async fn create_worktree(
    project_root: &Path,
    worktree_path: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<(), GitError> {
    validate_branch(project_root, branch).await?;
    rev_parse(project_root, base_ref).await?;
    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let worktree = worktree_path.to_string_lossy().to_string();
    let result = git(
        project_root,
        &["worktree", "add", "-b", branch, &worktree, base_ref],
    )
    .await;
    if result.is_err() {
        let _ = git(project_root, &["worktree", "remove", "--force", &worktree]).await;
        let _ = tokio::fs::remove_dir_all(worktree_path).await;
    }
    result.map(|_| ())
}

pub async fn remove_worktree(
    project_root: &Path,
    worktree_path: &Path,
    force: bool,
) -> Result<(), GitError> {
    let worktree = worktree_path.to_string_lossy().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&worktree);
    git(project_root, &args).await?;
    if worktree_path.exists() {
        tokio::fs::remove_dir_all(worktree_path).await?;
    }
    Ok(())
}

pub async fn delete_branch(project_root: &Path, branch: &str, force: bool) -> Result<(), GitError> {
    let flag = if force { "-D" } else { "-d" };
    git(project_root, &["branch", flag, branch])
        .await
        .map(|_| ())
}

pub async fn add_all(root: &Path) -> Result<(), GitError> {
    git(root, &["add", "-A"]).await.map(|_| ())
}

pub async fn reset_index(root: &Path) -> Result<(), GitError> {
    git(root, &["reset"]).await.map(|_| ())
}

/// Produce a binary patch from `base_sha` to the current worktree content,
/// including untracked files. The worktree index is left as it was.
pub async fn patch_from_base(worktree: &Path, base_sha: &str) -> Result<String, GitError> {
    add_all(worktree).await?;
    let patch = git_raw(
        worktree,
        &["diff", "--cached", "--binary", "--no-color", base_sha],
    )
    .await;
    let reset = reset_index(worktree).await;
    match (patch, reset) {
        (Ok(patch), Ok(())) => Ok(String::from_utf8_lossy(&patch).to_string()),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

/// Patch containing committed changes since `base_sha` plus uncommitted work.
pub async fn worktree_patch(worktree: &Path, base_sha: Option<&str>) -> Result<String, GitError> {
    add_all(worktree).await?;
    let mut args = vec!["diff", "--cached", "--binary", "--no-color"];
    if let Some(base) = base_sha {
        args.push(base);
    } else {
        args.push("HEAD");
    }
    let patch = git_raw(worktree, &args).await;
    let reset = reset_index(worktree).await;
    match (patch, reset) {
        (Ok(patch), Ok(())) => Ok(String::from_utf8_lossy(&patch).to_string()),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

pub async fn commit_all(root: &Path, message: &str) -> Result<String, GitError> {
    add_all(root).await?;
    git(root, &["commit", "-m", message]).await?;
    head_sha(root).await
}

/// Apply a worktree patch to the project root without staging or committing.
pub async fn apply_patch(project_root: &Path, patch: &str) -> Result<(), GitError> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["apply", "--3way", "--whitespace=nowarn", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(patch.as_bytes()).await?;
        drop(stdin);
    }
    let output = child.wait_with_output().await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitError::Message(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        git(path, &["init", "-b", "main"]).await.unwrap();
        git(path, &["config", "user.email", "test@example.com"])
            .await
            .unwrap();
        git(path, &["config", "user.name", "Test"]).await.unwrap();
        tokio::fs::write(path.join("README.md"), "hello\n")
            .await
            .unwrap();
        git(path, &["add", "-A"]).await.unwrap();
        git(path, &["commit", "-m", "initial"]).await.unwrap();
        dir
    }

    #[tokio::test]
    async fn inspects_repository() {
        let dir = init_repo().await;
        let info = inspect(dir.path()).await.unwrap();
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert!(info.clean);
        assert!(!info.head_sha.is_empty());
    }

    #[tokio::test]
    async fn creates_and_removes_worktree() {
        let dir = init_repo().await;
        let worktree = dir.path().join("wt");
        create_worktree(dir.path(), &worktree, "thread/test", "main")
            .await
            .unwrap();
        assert!(worktree.join("README.md").exists());
        remove_worktree(dir.path(), &worktree, true).await.unwrap();
        assert!(!worktree.exists());
    }

    #[tokio::test]
    async fn rejects_invalid_branch() {
        let dir = init_repo().await;
        let result =
            create_worktree(dir.path(), &dir.path().join("wt"), "bad branch", "main").await;
        assert!(matches!(result, Err(GitError::InvalidBranch(_))));
    }
}
