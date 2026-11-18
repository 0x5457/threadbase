//! Git and worktree operations. The system `git` binary is the source of
//! truth; Threadbase never reimplements git object handling.

use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotARepository(String),
    #[error("repository has no commits yet: {0}")]
    NoCommits(String),
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

/// Subject of the commit `snapshot_worktree` writes before a turn starts.
/// History and the transcript both key off this exact string.
pub const TURN_SNAPSHOT_MESSAGE: &str = "Threadbase turn snapshot";

/// One commit as the thread history panel needs it: identity, time and the
/// stats of the change the commit itself introduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    /// Committer time in Unix seconds.
    pub at: i64,
    pub author: String,
    pub subject: String,
    pub additions: u32,
    pub deletions: u32,
    pub files: u32,
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
    // A freshly `git init`-ed directory is a repository, but its HEAD is unborn.
    // Diffs and worktrees need a commit to point at, so report that separately
    // instead of mislabelling the directory as "not a git repository".
    let head_sha = git_allow_failure(&root, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .await?
        .ok_or_else(|| GitError::NoCommits(root.display().to_string()))?;
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

pub async fn head_sha(root: &Path) -> Result<String, GitError> {
    git(root, &["rev-parse", "HEAD"]).await
}

/// Commits in `range`, newest first. `--numstat` is parsed in the same pass so
/// the history panel can show counts without one subprocess per row.
pub async fn log(root: &Path, range: &str, limit: u32) -> Result<Vec<CommitSummary>, GitError> {
    let limit = limit.max(1).to_string();
    let output = git(
        root,
        &[
            "log",
            "--no-color",
            "--format=%x1e%H%x1f%h%x1f%ct%x1f%an%x1f%s",
            "--numstat",
            "-n",
            &limit,
            range,
        ],
    )
    .await?;
    Ok(parse_log(&output))
}

/// Records start with `\x1e`; the following lines until the next record are
/// `--numstat` rows (`added\tdeleted\tpath`, `-\t-\t` for binary).
fn parse_log(text: &str) -> Vec<CommitSummary> {
    let mut commits = Vec::new();
    for record in text.split('\u{1e}').filter(|part| !part.trim().is_empty()) {
        let mut lines = record.lines();
        let Some(header) = lines.next() else {
            continue;
        };
        let mut fields = header.split('\u{1f}');
        let sha = fields.next().unwrap_or_default().to_owned();
        let short_sha = fields.next().unwrap_or_default().to_owned();
        let at = fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let author = fields.next().unwrap_or_default().to_owned();
        let subject = fields.next().unwrap_or_default().to_owned();
        let mut additions = 0u32;
        let mut deletions = 0u32;
        let mut files = 0u32;
        for line in lines {
            let mut parts = line.splitn(3, '\t');
            let added = parts.next().unwrap_or_default();
            let deleted = parts.next().unwrap_or_default();
            if parts.next().is_none() {
                continue;
            }
            files = files.saturating_add(1);
            additions = additions.saturating_add(added.parse().unwrap_or(0));
            deletions = deletions.saturating_add(deleted.parse().unwrap_or(0));
        }
        commits.push(CommitSummary {
            sha,
            short_sha,
            at,
            author,
            subject,
            additions,
            deletions,
            files,
        });
    }
    commits
}

pub async fn current_branch(root: &Path) -> Result<String, GitError> {
    git(root, &["branch", "--show-current"]).await
}

/// Porcelain status; an empty string means the tree is clean.
pub async fn status_porcelain(root: &Path) -> Result<String, GitError> {
    git(root, &["status", "--porcelain"]).await
}

/// Restore a clean tree to a known commit. Only used after Threadbase itself
/// verified the tree was clean before applying a patch.
pub async fn reset_hard(root: &Path, reference: &str) -> Result<(), GitError> {
    git(root, &["reset", "--hard", reference]).await.map(|_| ())
}

pub async fn clean_untracked(root: &Path) -> Result<(), GitError> {
    git(root, &["clean", "-fd"]).await.map(|_| ())
}

pub async fn rev_parse(root: &Path, reference: &str) -> Result<String, GitError> {
    git(root, &["rev-parse", "--verify", reference]).await
}

/// Whether `ancestor` is reachable from `descendant`. A git error reads as
/// "no": the caller uses this to refuse a destructive reset, not to prove one.
pub async fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
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

async fn add_all(root: &Path) -> Result<(), GitError> {
    git(root, &["add", "-A"]).await.map(|_| ())
}

/// Commit, falling back to a Threadbase author only when git has no identity
/// configured. Repositories owned by the user keep their own author.
async fn commit(root: &Path, message: &str) -> Result<(), GitError> {
    let mut args: Vec<&str> = Vec::new();
    let email = git_allow_failure(root, &["config", "user.email"]).await?;
    if email.is_none_or(|value| value.is_empty()) {
        args.extend([
            "-c",
            "user.name=Threadbase",
            "-c",
            "user.email=threadbase@localhost",
        ]);
    }
    args.extend(["commit", "-m", message]);
    git(root, &args).await.map(|_| ())
}

/// Initialize a repository with `main` as its default branch.
pub async fn init_repository(root: &Path) -> Result<(), GitError> {
    git(root, &["init", "-b", "main"]).await.map(|_| ())
}

pub async fn reset_index(root: &Path) -> Result<(), GitError> {
    git(root, &["reset"]).await.map(|_| ())
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

/// Every uncommitted tracked change in `root`, staged or not. Unlike
/// [`worktree_patch`] it never stages anything, so reading a dirty project
/// cannot unstage a person's work.
pub async fn worktree_state_patch(root: &Path) -> Result<String, GitError> {
    let patch = git_raw(root, &["diff", "--binary", "--no-color", "HEAD"]).await?;
    Ok(String::from_utf8_lossy(&patch).to_string())
}

/// Copy the files git does not track. [`worktree_state_patch`] covers tracked
/// paths only, and a fresh file an agent wrote is work a fork should carry.
pub async fn copy_untracked(source: &Path, target: &Path) -> Result<(), GitError> {
    let listing = git_raw(
        source,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )
    .await?;
    for raw in listing
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path =
            std::str::from_utf8(raw).map_err(|error| GitError::Message(error.to_string()))?;
        let relative = Path::new(path);
        let to = target.join(relative);
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(source.join(relative), &to).await?;
    }
    Ok(())
}

/// Snapshot the current worktree as a commit and return its SHA. If there is
/// nothing to commit, return the current HEAD. This is used at turn start so
/// "Last turn" compares against the state before the agent started editing.
pub async fn snapshot_worktree(root: &Path, message: &str) -> Result<String, GitError> {
    // The message is written by the caller; `TURN_SNAPSHOT_MESSAGE` is what
    // the history endpoint recognises as a checkpoint.
    add_all(root).await?;
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .await?;
    if status.success() {
        return head_sha(root).await;
    }
    commit(root, message).await?;
    head_sha(root).await
}

pub async fn commit_all(root: &Path, message: &str) -> Result<String, GitError> {
    add_all(root).await?;
    commit(root, message).await?;
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
    async fn reports_repository_without_commits() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]).await.unwrap();
        let error = inspect(dir.path()).await.unwrap_err();
        assert!(matches!(&error, GitError::NoCommits(_)), "{error:?}");
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

    #[test]
    fn parses_log_numstat_records() {
        let text = "\u{1e}abc123\u{1f}abc123\u{1f}1700000000\u{1f}Ada\u{1f}turn one\n\
                    4\t1\tsrc/lib.rs\n\
                    2\t0\tREADME.md\n\
                    \u{1e}def456\u{1f}def456\u{1f}1700000100\u{1f}Ada\u{1f}binary\n\
                    -\t-\tlogo.png";
        let commits = parse_log(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, "abc123");
        assert_eq!(commits[0].subject, "turn one");
        assert_eq!(
            (commits[0].additions, commits[0].deletions, commits[0].files),
            (6, 1, 2)
        );
        assert_eq!(commits[1].files, 1);
        assert_eq!(commits[1].additions, 0);
    }
}
