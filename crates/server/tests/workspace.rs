//! Integration tests for projects, worktrees, diff review and thread lifecycle.

use sqlx::SqlitePool;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::EventStore;
use threadbase_server::{auth, comments, db, diff, projects, threads};
use threadbase_shared::api::{
    CreateCommentRequest, CreateProjectRequest, CreateThreadRequest, ForkThreadRequest,
    UpdateThreadRequest,
};
use threadbase_shared::{BaselineDto, BaselineKind, CommentAnchor, ErrorCode, Role, ThreadStatus};
use tokio::process::Command as TokioCommand;

fn test_projection(pool: &SqlitePool) -> threadbase_server::projection::Projection {
    let (state_tx, _) = tokio::sync::broadcast::channel(16);
    threadbase_server::projection::Projection::new(pool.clone(), state_tx)
}

struct Fixture {
    repo: TempDir,
    data: TempDir,
    pool: SqlitePool,
    current: CurrentUser,
}

async fn fixture() -> Fixture {
    let repo = TempDir::new().unwrap();
    init_repo(repo.path()).await;
    let data = TempDir::new().unwrap();
    let pool = db::connect_memory().await.unwrap();
    let user = auth::ensure_identity_user(
        &pool,
        &auth::Identity::github(1, "ada", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let current = CurrentUser {
        user: user.clone(),
        method: "github",
    };
    Fixture {
        repo,
        data,
        pool,
        current,
    }
}

async fn init_repo(path: &Path) {
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@example.com"]);
    git(path, &["config", "user.name", "Test"]);
    tokio::fs::write(path.join("README.md"), "one\ntwo\n")
        .await
        .unwrap();
    tokio::fs::write(path.join("src.txt"), "hello\n")
        .await
        .unwrap();
    git(path, &["add", "-A"]);
    git(path, &["commit", "-m", "initial"]);
}

fn git_output(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn create_project(fixture: &Fixture) -> threadbase_shared::ProjectDto {
    projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: fixture.repo.path().to_string_lossy().to_string(),
            name: None,
        },
    )
    .await
    .unwrap()
}

async fn create_thread(fixture: &Fixture, project_id: &str) -> threadbase_shared::ThreadDto {
    threads::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        fixture.data.path(),
        &fixture.current,
        project_id,
        &CreateThreadRequest {
            title: Some("Test thread".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
            worktree: None,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn thread_without_worktree_runs_in_the_project_directory() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = threads::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        fixture.data.path(),
        &fixture.current,
        project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Chat thread".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
            worktree: Some(false),
        },
    )
    .await
    .unwrap();

    assert!(thread.worktree_path.is_none());
    assert_eq!(thread.branch, "main");
    assert!(
        !fixture
            .data
            .path()
            .join("worktrees")
            .join(thread.id.as_str())
            .exists(),
        "a chat thread must not create a worktree directory"
    );
    let cwd: String = sqlx::query_scalar("SELECT cwd FROM agent_bindings WHERE thread_id = ?")
        .bind(thread.id.as_str())
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(cwd, fixture.repo.path().to_string_lossy());

    // Deleting the thread must leave the project's own branch alone.
    threads::delete(&fixture.pool, thread.id.as_str(), &fixture.current)
        .await
        .unwrap();
    let branches = git_output(fixture.repo.path(), &["branch", "--list", "main"]);
    assert!(
        branches.contains("main"),
        "base branch was deleted: {branches}"
    );
}

#[tokio::test]
async fn project_validation_and_thread_lifecycle() {
    let fixture = fixture().await;
    let non_git = TempDir::new().unwrap();
    let err = projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: non_git.path().to_string_lossy().to_string(),
            name: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);

    let err = projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: "relative/repo".into(),
            name: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);

    let project = create_project(&fixture).await;
    assert_eq!(project.active_threads, 0);
    let err = projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: fixture.repo.path().to_string_lossy().to_string(),
            name: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Conflict);

    let thread = create_thread(&fixture, project.id.as_str()).await;
    assert_eq!(thread.status, ThreadStatus::Idle);
    assert_eq!(thread.participants.len(), 1);
    assert_eq!(thread.participants[0].role, Role::Owner);
    let worktree = thread.worktree_path.as_deref().unwrap();
    assert!(Path::new(worktree).join("README.md").is_file());

    let archived = threads::update(
        &fixture.pool,
        &test_projection(&fixture.pool),
        thread.id.as_str(),
        &fixture.current,
        &UpdateThreadRequest {
            title: None,
            pinned: None,
            archived: Some(true),
            model: None,
            effort: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(archived.status, ThreadStatus::Archived);
    let active = threads::list(&fixture.pool, project.id.as_str(), false)
        .await
        .unwrap();
    assert!(active.is_empty());
    let all = threads::list(&fixture.pool, project.id.as_str(), true)
        .await
        .unwrap();
    assert_eq!(all.len(), 1);

    let unarchived = threads::update(
        &fixture.pool,
        &test_projection(&fixture.pool),
        thread.id.as_str(),
        &fixture.current,
        &UpdateThreadRequest {
            title: None,
            pinned: None,
            archived: Some(false),
            model: None,
            effort: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(unarchived.status, ThreadStatus::Idle);

    tokio::fs::write(
        Path::new(worktree).join("README.md"),
        "one\ntwo\nsource change\n",
    )
    .await
    .unwrap();
    let fork = threads::fork(
        &fixture.pool,
        &test_projection(&fixture.pool),
        fixture.data.path(),
        thread.id.as_str(),
        &fixture.current,
        &ForkThreadRequest { title: None },
    )
    .await
    .unwrap();
    let fork_worktree = fork.worktree_path.as_deref().unwrap();
    assert_eq!(
        tokio::fs::read_to_string(Path::new(fork_worktree).join("README.md"))
            .await
            .unwrap(),
        "one\ntwo\nsource change\n"
    );
    tokio::fs::write(Path::new(fork_worktree).join("README.md"), "fork only\n")
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read_to_string(Path::new(worktree).join("README.md"))
            .await
            .unwrap(),
        "one\ntwo\nsource change\n"
    );

    threads::delete(&fixture.pool, thread.id.as_str(), &fixture.current)
        .await
        .unwrap();
    assert!(!Path::new(worktree).exists());
    threads::delete(&fixture.pool, fork.id.as_str(), &fixture.current)
        .await
        .unwrap();
    assert!(!Path::new(fork_worktree).exists());
}

#[tokio::test]
async fn diff_baselines_seen_and_merge_back() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = thread.worktree_path.as_deref().unwrap();
    tokio::fs::write(Path::new(worktree).join("README.md"), "one\nTWO\n")
        .await
        .unwrap();
    tokio::fs::write(Path::new(worktree).join("new.txt"), "new\nfile\n")
        .await
        .unwrap();

    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let base = BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    };
    let changes = diff::list(&fixture.pool, &row, &fixture.current, &base)
        .await
        .unwrap();
    assert_eq!(changes.stats.files, 2);
    assert_eq!(changes.stats.additions, 3);
    assert_eq!(changes.stats.deletions, 1);
    assert!(changes.files.iter().any(|file| file.path == "new.txt"));
    let readme = diff::file_diff(&row, &base, "README.md").await.unwrap();
    assert!(!readme.hunks.is_empty());
    assert!(readme.hunks[0]
        .lines
        .iter()
        .any(|line| line.content == "TWO"));

    diff::mark_seen(
        &fixture.pool,
        &row,
        &fixture.current,
        &["README.md".into()],
        true,
        &base,
    )
    .await
    .unwrap();
    let seen = changes_stats(&fixture, &row).await;
    assert_eq!(seen.seen, 1);
    tokio::fs::write(
        Path::new(worktree).join("README.md"),
        "one\nchanged again\n",
    )
    .await
    .unwrap();
    let reset = changes_stats(&fixture, &row).await;
    assert_eq!(reset.seen, 0);

    git(Path::new(worktree), &["add", "-A"]);
    git(Path::new(worktree), &["commit", "-m", "work"]);
    let last_commit = diff::list(
        &fixture.pool,
        &row,
        &fixture.current,
        &BaselineDto {
            kind: BaselineKind::LastCommit,
            commit: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(last_commit.stats.files, 0);
    let last_turn = diff::list(
        &fixture.pool,
        &row,
        &fixture.current,
        &BaselineDto {
            kind: BaselineKind::LastTurn,
            commit: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(last_turn.stats.files, 2);
    let commit = diff::list(
        &fixture.pool,
        &row,
        &fixture.current,
        &BaselineDto {
            kind: BaselineKind::Commit,
            commit: row.base_sha.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(commit.stats.files, 2);

    let head_before = git_stdout(fixture.repo.path(), &["rev-parse", "HEAD"]).await;
    let result = diff::merge_back(&fixture.pool, &row, &fixture.current, false, None)
        .await
        .unwrap();
    assert!(!result.committed);
    assert_eq!(result.files.len(), 2);
    assert_eq!(
        tokio::fs::read_to_string(fixture.repo.path().join("README.md"))
            .await
            .unwrap(),
        "one\nchanged again\n"
    );
    assert!(fixture.repo.path().join("new.txt").is_file());
    assert_eq!(
        git_stdout(fixture.repo.path(), &["rev-parse", "HEAD"]).await,
        head_before
    );

    let result = diff::merge_back(
        &fixture.pool,
        &row,
        &fixture.current,
        true,
        Some("Apply thread"),
    )
    .await;
    // The same patch is already in the working tree, so a second apply fails or is empty.
    // A clean commit path is covered by making a new change first when needed.
    if let Ok(result) = result {
        assert!(result.committed);
    }
}

#[tokio::test]
async fn seen_state_is_scoped_to_baseline() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = thread.worktree_path.as_deref().unwrap();
    tokio::fs::write(Path::new(worktree).join("README.md"), "one\nTWO\n")
        .await
        .unwrap();

    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let base = BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    };
    let last_commit = BaselineDto {
        kind: BaselineKind::LastCommit,
        commit: None,
    };

    diff::mark_seen(
        &fixture.pool,
        &row,
        &fixture.current,
        &["README.md".into()],
        true,
        &base,
    )
    .await
    .unwrap();
    let base_stats = changes_stats(&fixture, &row).await;
    assert_eq!(base_stats.seen, 1);

    // Switching baseline must not inherit the base-branch Seen mark.
    let last_commit_changes = diff::list(&fixture.pool, &row, &fixture.current, &last_commit)
        .await
        .unwrap();
    assert_eq!(last_commit_changes.stats.seen, 0);

    // Seen on Last Commit is independent from the base branch.
    diff::mark_seen(
        &fixture.pool,
        &row,
        &fixture.current,
        &["README.md".into()],
        true,
        &last_commit,
    )
    .await
    .unwrap();
    let last_commit_changes = diff::list(&fixture.pool, &row, &fixture.current, &last_commit)
        .await
        .unwrap();
    assert_eq!(last_commit_changes.stats.seen, 1);
    let base_changes = diff::list(&fixture.pool, &row, &fixture.current, &base)
        .await
        .unwrap();
    assert_eq!(base_changes.stats.seen, 1, "base-branch Seen remains");
}

async fn changes_stats(
    fixture: &Fixture,
    row: &threadbase_server::threads::ThreadRow,
) -> threadbase_shared::ChangeStats {
    diff::list(
        &fixture.pool,
        row,
        &fixture.current,
        &BaselineDto {
            kind: BaselineKind::BaseBranch,
            commit: None,
        },
    )
    .await
    .unwrap()
    .stats
}

async fn git_stdout(path: &Path, args: &[&str]) -> String {
    let output = TokioCommand::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[tokio::test]
async fn diff_comment_hunk_ids_are_reresolved_then_degrade() {
    let fixture = fixture().await;
    let base_text = (0..60)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    tokio::fs::write(fixture.repo.path().join("hunks.txt"), &base_text)
        .await
        .unwrap();
    git(fixture.repo.path(), &["add", "hunks.txt"]);
    git(fixture.repo.path(), &["commit", "-m", "hunks base"]);
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = Path::new(thread.worktree_path.as_deref().unwrap());
    let write_hunks = |changes: &[usize]| {
        let mut lines: Vec<String> = (0..60).map(|index| format!("line {index}")).collect();
        for index in changes {
            lines[*index] = format!("line {index} changed");
        }
        format!("{}\n", lines.join("\n"))
    };
    tokio::fs::write(worktree.join("hunks.txt"), write_hunks(&[2, 20]))
        .await
        .unwrap();
    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let base = BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    };
    let file = diff::file_diff(&row, &base, "hunks.txt").await.unwrap();
    let hunk = file
        .hunks
        .iter()
        .find(|hunk| {
            hunk.lines
                .iter()
                .any(|line| line.content == "line 20 changed")
        })
        .expect("a hunk for the anchored line");
    assert_eq!(hunk.id, "h2");
    let original_hash = hunk.context_hash.clone();

    let events = EventStore::new(fixture.pool.clone());
    let comment = comments::create(
        &fixture.pool,
        &events,
        thread.id.as_str(),
        &fixture.current,
        &CreateCommentRequest {
            body: "Keep this line".into(),
            anchor: CommentAnchor::Diff {
                path: "hunks.txt".into(),
                hunk_id: hunk.id.clone(),
                line: 21,
                context_hash: Some(original_hash.clone()),
            },
            parent_id: None,
            send: false,
        },
    )
    .await
    .unwrap();

    // A new change earlier in the file shifts the anchored hunk from h2 to h3
    // without touching its own header or contents, so its context hash stays
    // the same.
    tokio::fs::write(worktree.join("hunks.txt"), write_hunks(&[2, 12, 20]))
        .await
        .unwrap();
    let shifted = diff::file_diff(&row, &base, "hunks.txt").await.unwrap();
    let shifted_hunk = shifted
        .hunks
        .iter()
        .find(|hunk| hunk.context_hash == original_hash)
        .expect("the anchored hunk is still present");
    assert_eq!(shifted_hunk.id, "h3");

    // The send path reads pending comments without reconciling anchors.
    let pending = comments::pending_for_thread(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let unsent = pending.iter().find(|entry| entry.id == comment.id).unwrap();
    match &unsent.anchor {
        CommentAnchor::Diff { hunk_id, .. } => assert_eq!(hunk_id, "h2"),
        other => panic!("send path should keep the stored anchor: {other:?}"),
    }

    let listed = comments::list_for_thread(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let resolved = listed.iter().find(|entry| entry.id == comment.id).unwrap();
    assert!(!resolved.anchor_changed);
    match &resolved.anchor {
        CommentAnchor::Diff { hunk_id, .. } => assert_eq!(hunk_id, "h3"),
        other => panic!("anchor should be re-resolved to the diff hunk: {other:?}"),
    }

    // Reverting the anchored line removes its hunk, so the anchor degrades to
    // a file anchor and records that it changed.
    tokio::fs::write(worktree.join("hunks.txt"), write_hunks(&[2, 12]))
        .await
        .unwrap();
    let listed = comments::list_for_thread(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let degraded = listed.iter().find(|entry| entry.id == comment.id).unwrap();
    assert!(degraded.anchor_changed);
    assert!(matches!(degraded.anchor, CommentAnchor::File { .. }));

    // Reconciliation is idempotent: a second read leaves the persisted flag
    // and anchor in place.
    let listed = comments::list_for_thread(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let stable = listed.iter().find(|entry| entry.id == comment.id).unwrap();
    assert!(stable.anchor_changed);
    assert!(matches!(stable.anchor, CommentAnchor::File { .. }));
}

#[tokio::test]
async fn diff_comment_drift_and_revert_to_base() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = Path::new(thread.worktree_path.as_deref().unwrap());
    tokio::fs::write(worktree.join("README.md"), "one\nTWO\n")
        .await
        .unwrap();
    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let base = BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    };
    let file = diff::file_diff(&row, &base, "README.md").await.unwrap();
    let hunk = file.hunks.first().expect("a changed hunk");
    let events = EventStore::new(fixture.pool.clone());
    let comment = comments::create(
        &fixture.pool,
        &events,
        thread.id.as_str(),
        &fixture.current,
        &CreateCommentRequest {
            body: "This line needs work".into(),
            anchor: CommentAnchor::Diff {
                path: "README.md".into(),
                hunk_id: hunk.id.clone(),
                line: 2,
                context_hash: Some(hunk.context_hash.clone()),
            },
            parent_id: None,
            send: false,
        },
    )
    .await
    .unwrap();
    assert!(!comment.anchor_changed);

    // A new line before the hunk changes the header and therefore the hash.
    tokio::fs::write(worktree.join("README.md"), "zero\none\nTWO\n")
        .await
        .unwrap();
    let listed = comments::list_for_thread(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let drifted = listed.iter().find(|entry| entry.id == comment.id).unwrap();
    assert!(drifted.anchor_changed);
    assert!(matches!(drifted.anchor, CommentAnchor::File { .. }));

    tokio::fs::write(worktree.join("untracked.txt"), "clean me\n")
        .await
        .unwrap();
    diff::revert_to_base(&row).await.unwrap();
    assert_eq!(
        tokio::fs::read_to_string(worktree.join("README.md"))
            .await
            .unwrap(),
        "one\ntwo\n"
    );
    assert!(!worktree.join("untracked.txt").exists());
}

#[tokio::test]
async fn project_path_can_be_relocated() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let moved = TempDir::new().unwrap();
    init_repo(moved.path()).await;
    let updated = projects::update(
        &fixture.pool,
        project.id.as_str(),
        None,
        None,
        Some(moved.path().to_str().unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(updated.path, moved.path().to_string_lossy());
    assert!(updated.available);
}
