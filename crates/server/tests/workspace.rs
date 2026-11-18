//! Integration tests for projects, worktrees, diff review and thread lifecycle.

use sqlx::SqlitePool;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::{EventMeta, EventStore};
use threadbase_server::{auth, comments, db, diff, projects, queue, threads};
use threadbase_shared::api::{
    CreateCommentRequest, CreateProjectRequest, CreateThreadRequest, ForkThreadRequest,
    UpdateProjectRequest, UpdateThreadRequest,
};
use threadbase_shared::{
    Actor, BaselineDto, BaselineKind, CommentAnchor, CommentSide, ErrorCode, MessageId, Role,
    ThreadEvent, ThreadStatus, TurnId, UserBrief,
};
use tokio::process::Command as TokioCommand;

fn policy() -> threadbase_core::permissions::PermissionPolicy {
    threadbase_core::permissions::PermissionPolicy::default()
}

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
            create: false,
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
        &policy(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn restart_recovery_resets_interrupted_threads() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;

    // Simulate a server that died in the middle of a turn, with input queued
    // behind that turn.
    let turn_id = "turn_interrupted";
    sqlx::query("UPDATE threads SET status = 'running', current_turn_id = ? WHERE id = ?")
        .bind(turn_id)
        .bind(thread.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    queue::enqueue(
        &fixture.pool,
        queue::NewQueuedInput {
            thread_id: thread.id.as_str(),
            author_id: fixture.current.user.id.as_str(),
            text: "after the restart",
            authors_json: None,
            attachments_json: None,
            created_at: threadbase_shared::now_ms(),
        },
    )
    .await
    .unwrap();

    let events = EventStore::new(fixture.pool.clone());
    let projection = test_projection(&fixture.pool);
    let reconciled = threads::reconcile_after_restart(&fixture.pool, &events, &projection)
        .await
        .unwrap();
    assert_eq!(
        reconciled,
        vec![(thread.id.to_string(), Some(turn_id.to_owned()))]
    );

    let reloaded = threads::get(
        &fixture.pool,
        thread.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(reloaded.status, ThreadStatus::Idle);
    let current_turn: Option<String> =
        sqlx::query_scalar("SELECT current_turn_id FROM threads WHERE id = ?")
            .bind(thread.id.as_str())
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert!(current_turn.is_none());

    let (kind, payload): (String, String) = sqlx::query_as(
        "SELECT kind, payload_json FROM events WHERE thread_id = ? ORDER BY seq DESC LIMIT 1",
    )
    .bind(thread.id.as_str())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(kind, "turn_error");
    assert!(payload.contains(turn_id), "{payload}");

    // Queued input survives the reset; a second boot has nothing left to do.
    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(thread.id.as_str())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    let again = threads::reconcile_after_restart(&fixture.pool, &events, &projection)
        .await
        .unwrap();
    assert!(again.is_empty());
}

#[tokio::test]
async fn restart_recovery_resets_a_thread_waiting_for_approval() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    // The old process died while the agent was waiting for a human.
    sqlx::query(
        "UPDATE threads SET status = 'requires_action', current_turn_id = 'turn-waiting' WHERE id = ?",
    )
    .bind(thread.id.as_str())
    .execute(&fixture.pool)
    .await
    .unwrap();

    let events = EventStore::new(fixture.pool.clone());
    let projection = test_projection(&fixture.pool);
    let reconciled = threads::reconcile_after_restart(&fixture.pool, &events, &projection)
        .await
        .unwrap();
    assert_eq!(
        reconciled,
        vec![(thread.id.to_string(), Some("turn-waiting".to_owned()))]
    );
    let status: String = sqlx::query_scalar("SELECT status FROM threads WHERE id = ?")
        .bind(thread.id.as_str())
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(status, "idle");
    let kind: String =
        sqlx::query_scalar("SELECT kind FROM events WHERE thread_id = ? ORDER BY seq DESC LIMIT 1")
            .bind(thread.id.as_str())
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_eq!(kind, "turn_error");
}

#[tokio::test]
async fn restart_recovery_discards_a_turn_that_never_started() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let events = EventStore::new(fixture.pool.clone());
    let turn_id = TurnId::new();
    let brief = UserBrief::from_user(&fixture.current.user);
    events
        .append(
            &thread.id,
            Actor::user(&brief),
            ThreadEvent::TurnStarted {
                turn_id: turn_id.clone(),
                message_id: MessageId::new(),
                author: brief.clone(),
                authors: vec![brief],
                prompt: "never reached the adapter".into(),
                attachments: Vec::new(),
            },
            EventMeta::default(),
        )
        .await
        .unwrap();
    // The turn row committed, but the process died before the Prompt
    // transition, so the adapter never saw it.
    sqlx::query("UPDATE threads SET status = 'idle', current_turn_id = ? WHERE id = ?")
        .bind(turn_id.as_str())
        .bind(thread.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();

    let projection = test_projection(&fixture.pool);
    let reconciled = threads::reconcile_after_restart(&fixture.pool, &events, &projection)
        .await
        .unwrap();
    assert_eq!(reconciled, vec![(thread.id.to_string(), None)]);
    let current: Option<String> =
        sqlx::query_scalar("SELECT current_turn_id FROM threads WHERE id = ?")
            .bind(thread.id.as_str())
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert!(current.is_none());
    // The unstarted prompt is gone so the queue can re-run it without a
    // duplicate transcript row.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE thread_id = ?")
        .bind(thread.id.as_str())
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    // A second boot has nothing left to reconcile.
    assert!(
        threads::reconcile_after_restart(&fixture.pool, &events, &projection)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn creates_a_project_directory_and_repository() {
    let fixture = fixture().await;
    let parent = TempDir::new().unwrap();
    let roots = vec![parent.path().to_path_buf()];
    let target = parent.path().join("fresh-project");
    let project = projects::create_repository(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: target.to_string_lossy().to_string(),
            name: None,
            create: true,
        },
        &roots,
    )
    .await
    .unwrap();
    assert_eq!(project.name, "fresh-project");
    assert_eq!(project.default_branch, "main");
    assert_eq!(
        project.path,
        target.canonicalize().unwrap().to_string_lossy().as_ref()
    );
    assert!(target.join("README.md").is_file());
    assert_eq!(
        git_output(&target, &["rev-list", "--count", "HEAD"]).trim(),
        "1"
    );
    assert_eq!(
        git_output(&target, &["branch", "--show-current"]).trim(),
        "main"
    );
}

#[tokio::test]
async fn initializes_an_existing_empty_repository() {
    let fixture = fixture().await;
    let parent = TempDir::new().unwrap();
    let target = parent.path().join("empty-repo");
    std::fs::create_dir(&target).unwrap();
    git(&target, &["init", "-b", "main"]);

    let project = projects::create_repository(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: target.to_string_lossy().to_string(),
            name: Some("Empty Repo".into()),
            create: true,
        },
        &[parent.path().to_path_buf()],
    )
    .await
    .unwrap();
    assert_eq!(project.name, "Empty Repo");
    assert!(target.join("README.md").is_file());
    assert_eq!(
        git_output(&target, &["rev-list", "--count", "HEAD"]).trim(),
        "1"
    );
}

#[tokio::test]
async fn refuses_to_create_over_foreign_content() {
    let fixture = fixture().await;
    let parent = TempDir::new().unwrap();
    let roots = vec![parent.path().to_path_buf()];

    // Outside every root the picker can reach.
    let outside = TempDir::new().unwrap();
    let outside_target = outside.path().join("nope");
    let err = projects::create_repository(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: outside_target.to_string_lossy().to_string(),
            name: None,
            create: true,
        },
        &roots,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);
    assert!(!outside_target.exists());

    // A directory that already holds the user's files.
    let busy = parent.path().join("busy");
    std::fs::create_dir(&busy).unwrap();
    std::fs::write(busy.join("notes.txt"), "mine").unwrap();
    let err = projects::create_repository(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: busy.to_string_lossy().to_string(),
            name: None,
            create: true,
        },
        &roots,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);
    assert!(!busy.join(".git").exists());
    assert!(!busy.join("README.md").exists());
    assert!(busy.join("notes.txt").is_file());

    // An existing repository with history is added, not re-initialized.
    let existing = parent.path().join("existing");
    std::fs::create_dir(&existing).unwrap();
    init_repo(&existing).await;
    let err = projects::create_repository(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: existing.to_string_lossy().to_string(),
            name: None,
            create: true,
        },
        &roots,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);
    assert_eq!(
        git_output(&existing, &["rev-list", "--count", "HEAD"]).trim(),
        "1"
    );
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
        &policy(),
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
            create: false,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);

    // A repository without commits is still a repository; say so instead of
    // claiming the directory is not one.
    let empty = TempDir::new().unwrap();
    git(empty.path(), &["init", "-b", "main"]);
    let err = projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: empty.path().to_string_lossy().to_string(),
            name: None,
            create: false,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);
    assert!(
        err.0.message.contains("no commits"),
        "unexpected message: {}",
        err.0.message
    );

    let err = projects::create(
        &fixture.pool,
        &fixture.current,
        &CreateProjectRequest {
            path: "relative/repo".into(),
            name: None,
            create: false,
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
            create: false,
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
            auto_settle: None,
            permission_mode: None,
        },
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(archived.status, ThreadStatus::Archived);
    let active = threads::list(&fixture.pool, project.id.as_str(), false, &policy())
        .await
        .unwrap();
    assert!(active.is_empty());
    let all = threads::list(&fixture.pool, project.id.as_str(), true, &policy())
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
            auto_settle: None,
            permission_mode: None,
        },
        &policy(),
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
        &policy(),
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
    assert_eq!(result.branch, "main");
    assert!(!result.base_drift);
    assert!(!result.verified);
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

    // The first apply left the project dirty; a second apply must refuse
    // rather than commit unrelated working-tree state.
    assert!(
        diff::merge_back(&fixture.pool, &row, &fixture.current, true, None)
            .await
            .is_err()
    );
    git(fixture.repo.path(), &["reset", "--hard", &head_before]);
    git(fixture.repo.path(), &["clean", "-fd"]);
    let result = diff::merge_back(
        &fixture.pool,
        &row,
        &fixture.current,
        true,
        Some("Apply thread"),
    )
    .await
    .unwrap();
    assert!(result.committed);
    assert!(result.commit.is_some());
}

#[tokio::test]
async fn merge_back_runs_verify_before_touching_the_project() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = thread.worktree_path.as_deref().unwrap();
    tokio::fs::write(Path::new(worktree).join("README.md"), "one\nverified\n")
        .await
        .unwrap();
    sqlx::query("UPDATE projects SET verify_command = 'exit 7' WHERE id = ?")
        .bind(project.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();
    let error = diff::merge_back(&fixture.pool, &row, &fixture.current, false, None)
        .await
        .unwrap_err();
    assert!(error.0.message.contains("verify command failed"));
    assert_eq!(
        tokio::fs::read_to_string(fixture.repo.path().join("README.md"))
            .await
            .unwrap(),
        "one\ntwo\n"
    );
    assert!(git_output(fixture.repo.path(), &["status", "--porcelain"])
        .trim()
        .is_empty());

    sqlx::query("UPDATE projects SET verify_command = NULL WHERE id = ?")
        .bind(project.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let result = diff::merge_back(&fixture.pool, &row, &fixture.current, false, None)
        .await
        .unwrap();
    assert!(!result.verified);
    assert_eq!(result.files, vec!["README.md".to_owned()]);
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
        &test_projection(&fixture.pool),
        &events,
        thread.id.as_str(),
        &fixture.current,
        &CreateCommentRequest {
            body: "Keep this line".into(),
            anchor: CommentAnchor::Diff {
                path: "hunks.txt".into(),
                hunk_id: hunk.id.clone(),
                line: 21,
                side: Some(CommentSide::New),
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
        CommentAnchor::Diff { hunk_id, side, .. } => {
            assert_eq!(hunk_id, "h3");
            assert_eq!(*side, Some(CommentSide::New));
        }
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
        &test_projection(&fixture.pool),
        &events,
        thread.id.as_str(),
        &fixture.current,
        &CreateCommentRequest {
            body: "This line needs work".into(),
            anchor: CommentAnchor::Diff {
                path: "README.md".into(),
                hunk_id: hunk.id.clone(),
                line: 2,
                side: Some(CommentSide::New),
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
async fn thread_history_lists_checkpoints_and_commit_diffs() {
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let worktree = Path::new(thread.worktree_path.as_deref().unwrap());
    let row = threads::load_row(&fixture.pool, thread.id.as_str())
        .await
        .unwrap();

    let events = EventStore::new(fixture.pool.clone());
    let first_turn = TurnId::new();
    events
        .append(
            &thread.id,
            Actor::system(),
            ThreadEvent::TurnStarted {
                turn_id: first_turn.clone(),
                message_id: MessageId::new(),
                author: UserBrief::from_user(&fixture.current.user),
                authors: Vec::new(),
                prompt: "change the readme".into(),
                attachments: Vec::new(),
            },
            EventMeta::default(),
        )
        .await
        .unwrap();
    tokio::fs::write(worktree.join("README.md"), "one\nTWO\n")
        .await
        .unwrap();
    git(worktree, &["add", "-A"]);
    git(worktree, &["commit", "-m", "Threadbase turn snapshot"]);
    // The snapshot written before a turn holds the work of the turn before it,
    // so this commit is recorded as the checkpoint of the *next* turn. Turn 1's
    // own checkpoint is the untouched base — exactly what a clean worktree
    // produces, since `snapshot_worktree` returns HEAD when nothing changed.
    let snapshot = threadbase_workspace::git::head_sha(worktree).await.unwrap();
    let next_turn = TurnId::new();
    for (turn, sha, at) in [
        (
            first_turn.as_str(),
            thread.base_sha.clone().unwrap_or_default(),
            threadbase_shared::now_ms(),
        ),
        (
            next_turn.as_str(),
            snapshot.clone(),
            threadbase_shared::now_ms() + 1,
        ),
    ] {
        sqlx::query("INSERT INTO checkpoints(thread_id, turn_id, sha, created_at) VALUES(?,?,?,?)")
            .bind(thread.id.as_str())
            .bind(turn)
            .bind(sha)
            .bind(at)
            .execute(&fixture.pool)
            .await
            .unwrap();
    }
    events
        .append(
            &thread.id,
            Actor::system(),
            ThreadEvent::TurnStarted {
                turn_id: next_turn,
                message_id: MessageId::new(),
                author: UserBrief::from_user(&fixture.current.user),
                authors: Vec::new(),
                prompt: "follow up".into(),
                attachments: Vec::new(),
            },
            EventMeta::default(),
        )
        .await
        .unwrap();

    let history = diff::history(&fixture.pool, &row).await.unwrap();
    assert_eq!(history.entries.len(), 1);
    let entry = &history.entries[0];
    assert!(entry.checkpoint);
    assert_eq!(entry.turn_prompt.as_deref(), Some("change the readme"));
    assert_eq!((entry.additions, entry.deletions, entry.files), (1, 1, 1));
    assert_eq!(entry.short_sha.len(), 7);

    let commit = diff::commit_changes(&row, &entry.sha).await.unwrap();
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].path, "README.md");
    assert_eq!(commit.stats.additions, 1);
    assert_eq!(commit.stats.deletions, 1);
    let file = diff::commit_file_diff(&row, &entry.sha, "README.md")
        .await
        .unwrap();
    assert!(file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .any(|line| line.content == "TWO"));

    assert!(diff::commit_changes(&row, "not-a-sha").await.is_err());
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
        &UpdateProjectRequest {
            name: None,
            remote_url: None,
            path: Some(moved.path().to_str().unwrap().to_owned()),
            verify_command: None,
            setup_command: None,
            permission_mode: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.path, moved.path().to_string_lossy());
    assert!(updated.available);
}

#[tokio::test]
async fn placement_parks_threads_without_hiding_work() {
    use threadbase_core::state::PlacementAction;
    use threadbase_shared::{ThreadPlacement, ThreadSection};

    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let thread = create_thread(&fixture, project.id.as_str()).await;
    let projection = test_projection(&fixture.pool);
    assert_eq!(thread.placement, ThreadPlacement::Active);
    assert_eq!(thread.section, ThreadSection::Active);

    // Settling parks the thread but keeps it in the list: a shelf is a read
    // model, not a filter, so nothing disappears the way archiving hides work.
    threads::place(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        PlacementAction::Settle,
        None,
    )
    .await
    .unwrap();
    let listed = threads::list(&fixture.pool, project.id.as_str(), false, &policy())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].placement, ThreadPlacement::Settled);
    assert_eq!(listed[0].section, ThreadSection::Settled);

    // A message is a reason to come back: waking is the inverse of settling.
    threads::wake(&fixture.pool, &projection, thread.id.as_str())
        .await
        .unwrap();
    let woken = threads::get(
        &fixture.pool,
        thread.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(woken.placement, ThreadPlacement::Active);

    // A pin outranks the shelf it sits in, and settling takes the pin with it.
    threads::update(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        &fixture.current,
        &UpdateThreadRequest {
            title: None,
            pinned: Some(true),
            archived: None,
            model: None,
            effort: None,
            auto_settle: None,
            permission_mode: None,
        },
        &policy(),
    )
    .await
    .unwrap();
    let pinned = threads::get(
        &fixture.pool,
        thread.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(pinned.section, ThreadSection::Pinned);
    threads::place(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        PlacementAction::Settle,
        None,
    )
    .await
    .unwrap();
    let settled = threads::get(
        &fixture.pool,
        thread.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert!(!settled.pinned, "settling drops the pin");
    assert_eq!(settled.section, ThreadSection::Settled);

    // Snoozing records a deadline; the default is the server's, not the client's.
    let deadline = threadbase_shared::now_ms() + 90_000;
    threads::place(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        PlacementAction::Snooze,
        Some(deadline),
    )
    .await
    .unwrap();
    let snoozed = threads::get(
        &fixture.pool,
        thread.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(snoozed.placement, ThreadPlacement::Snoozed);
    assert_eq!(snoozed.snooze_until, Some(deadline));
    assert_eq!(snoozed.section, ThreadSection::Snoozed);

    // Parked work must be woken before it can be settled again, so a shelf
    // cannot swallow a thread nobody looked at.
    let err = threads::place(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        PlacementAction::Settle,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Conflict);

    // A running turn refuses to be parked: settling must never cancel work.
    sqlx::query("UPDATE threads SET placement = 'active', snooze_until = NULL WHERE id = ?")
        .bind(thread.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE threads SET status = 'running' WHERE id = ?")
        .bind(thread.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let err = threads::place(
        &fixture.pool,
        &projection,
        thread.id.as_str(),
        PlacementAction::Settle,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Conflict);
}

#[tokio::test]
async fn sweep_wakes_snoozes_and_settles_only_idle_work() {
    use threadbase_core::state::PlacementAction;
    use threadbase_shared::{now_ms, ThreadPlacement, ThreadSection};

    const DAY: i64 = 24 * 60 * 60 * 1000;
    let fixture = fixture().await;
    let project = create_project(&fixture).await;
    let projection = test_projection(&fixture.pool);
    let now = now_ms();
    let stale = now - 8 * DAY;

    let settled = create_thread(&fixture, project.id.as_str()).await;
    let opted_out = create_thread(&fixture, project.id.as_str()).await;
    let queued = create_thread(&fixture, project.id.as_str()).await;
    let snoozed = create_thread(&fixture, project.id.as_str()).await;

    for thread in [&settled, &opted_out, &queued, &snoozed] {
        sqlx::query(
            "UPDATE threads SET last_activity_at = ?, placement_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(stale)
        .bind(stale)
        .bind(stale)
        .bind(thread.id.as_str())
            .execute(&fixture.pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE threads SET auto_settle = 'never' WHERE id = ?")
        .bind(opted_out.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position,
            created_at, updated_at)
         VALUES('q1', ?, ?, 'still waiting', 'queued', 0, ?, ?)",
    )
    .bind(queued.id.as_str())
    .bind(fixture.current.user.id.as_str())
    .bind(now)
    .bind(now)
    .execute(&fixture.pool)
    .await
    .unwrap();
    threads::place(
        &fixture.pool,
        &projection,
        snoozed.id.as_str(),
        PlacementAction::Snooze,
        Some(now - 1_000),
    )
    .await
    .unwrap();

    let report = threads::sweep(&fixture.pool, &projection, 7 * DAY, now)
        .await
        .unwrap();
    assert_eq!(report.woken, 1);
    assert_eq!(report.settled, 1);

    let settled_after = threads::get(
        &fixture.pool,
        settled.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    let opted_out_after = threads::get(
        &fixture.pool,
        opted_out.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    let queued_after = threads::get(
        &fixture.pool,
        queued.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    let woken = threads::get(
        &fixture.pool,
        snoozed.id.as_str(),
        &fixture.current,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(settled_after.section, ThreadSection::Settled);
    assert_eq!(opted_out_after.section, ThreadSection::Active);
    // Queued input keeps the thread out of the shelf: it is waiting to run, so
    // it belongs in the working list rather than in "settled".
    assert_eq!(queued_after.section, ThreadSection::Working);
    assert_eq!(woken.placement, ThreadPlacement::Active);
    assert_eq!(woken.snooze_until, None);

    // A second pass is a no-op: settled work stays settled and the queue still
    // holds its thread in the active list.
    let again = threads::sweep(&fixture.pool, &projection, 7 * DAY, now)
        .await
        .unwrap();
    assert_eq!(again, threads::SweepReport::default());
}
