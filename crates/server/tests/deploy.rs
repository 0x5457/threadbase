//! Idle-window deploys: when the machine counts as idle, and how a staged
//! binary replaces the running one.

use sqlx::SqlitePool;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::config::Config;
use threadbase_server::{auth, db, deploy, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest};
use threadbase_shared::{now_ms, ThreadStatus};

fn policy() -> threadbase_core::permissions::PermissionPolicy {
    threadbase_core::permissions::PermissionPolicy::default()
}

struct Fixture {
    _repo: TempDir,
    _data: TempDir,
    pool: SqlitePool,
    thread_id: String,
    user_id: String,
}

async fn fixture() -> Fixture {
    let repo = TempDir::new().unwrap();
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "test@example.com"]);
    git(repo.path(), &["config", "user.name", "Test"]);
    std::fs::write(repo.path().join("README.md"), "one\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "initial"]);
    let data = TempDir::new().unwrap();
    let pool = db::connect_memory().await.unwrap();
    let user = auth::ensure_identity_user(
        &pool,
        &auth::Identity::github(1, "ada", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let current = auth::CurrentUser {
        user: user.clone(),
        method: "github",
    };
    let state = threadbase_server::app::AppState::new(
        Config {
            data_dir: data.path().to_path_buf(),
            db_path: data.path().join("threadbase.db"),
            ..Config::default()
        },
        pool.clone(),
    );
    let project = projects::create(
        &pool,
        &current,
        &CreateProjectRequest {
            path: repo.path().to_string_lossy().to_string(),
            name: None,
            create: false,
        },
    )
    .await
    .unwrap();
    let thread = threads::create(
        &pool,
        &state.projection,
        data.path(),
        &current,
        project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Deploy target".into()),
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
    .unwrap();
    Fixture {
        _repo: repo,
        _data: data,
        pool,
        thread_id: thread.id.0.clone(),
        user_id: user.id.0.clone(),
    }
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

#[tokio::test]
async fn an_idle_workspace_is_not_busy() {
    let fixture = fixture().await;
    assert!(!deploy::busy(&fixture.pool).await.unwrap());
}

#[tokio::test]
async fn a_running_turn_blocks_a_deploy() {
    let fixture = fixture().await;
    sqlx::query("UPDATE threads SET status = 'running' WHERE id = ?")
        .bind(&fixture.thread_id)
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(deploy::busy(&fixture.pool).await.unwrap());
}

/// A pending approval is work waiting on a person; a restart would cancel it, so
/// it counts as busy even though no turn is running.
#[tokio::test]
async fn an_unanswered_request_blocks_a_deploy() {
    let fixture = fixture().await;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO approvals(id, thread_id, agent, title, kind, paths_json, options_json,
            status, created_at)
         VALUES('a1', ?, 'codex', 'Run tests', 'command', '[]', '[]', 'pending', ?)",
    )
    .bind(&fixture.thread_id)
    .bind(now)
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(deploy::busy(&fixture.pool).await.unwrap());

    sqlx::query("UPDATE approvals SET status = 'approved' WHERE id = 'a1'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(!deploy::busy(&fixture.pool).await.unwrap());
}

#[tokio::test]
async fn queued_input_and_live_subagents_block_a_deploy() {
    let fixture = fixture().await;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position,
            created_at, updated_at)
         VALUES('q1', ?, ?, 'waiting', 'queued', 0, ?, ?)",
    )
    .bind(&fixture.thread_id)
    .bind(&fixture.user_id)
    .bind(now)
    .bind(now)
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(deploy::busy(&fixture.pool).await.unwrap());
    sqlx::query("DELETE FROM queued_inputs WHERE id = 'q1'")
        .execute(&fixture.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO subagents(id, thread_id, title, status, can_cancel, last_seq,
            created_at, updated_at)
         VALUES('s1', ?, 'reviewer', 'running', 1, 0, ?, ?)",
    )
    .bind(&fixture.thread_id)
    .bind(now)
    .bind(now)
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(deploy::busy(&fixture.pool).await.unwrap());
}

#[tokio::test]
async fn a_settled_or_archived_thread_does_not_block() {
    let fixture = fixture().await;
    sqlx::query("UPDATE threads SET status = ? WHERE id = ?")
        .bind(ThreadStatus::Archived.as_str())
        .bind(&fixture.thread_id)
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(!deploy::busy(&fixture.pool).await.unwrap());
}

#[tokio::test]
async fn installing_keeps_the_previous_binary_and_consumes_the_staged_one() {
    let dir = TempDir::new().unwrap();
    let staged = dir.path().join("incoming");
    std::fs::create_dir_all(&staged).unwrap();
    let incoming = staged.join("threadbase");
    std::fs::write(&incoming, b"new binary").unwrap();
    std::fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o755)).unwrap();
    let target = dir.path().join("threadbase");
    std::fs::write(&target, b"old binary").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let backups = dir.path().join("backups");

    let backup = deploy::install(&incoming, &target, &backups).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"new binary");
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755,
        "the installed binary stays executable"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), b"old binary");
    assert!(
        !staged.join("threadbase").exists(),
        "the staged file is consumed so it cannot install twice"
    );
    assert!(staged.join("threadbase.installed").exists());
}

#[tokio::test]
async fn a_non_executable_or_missing_file_is_not_staged() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("threadbase");
    assert!(!deploy::is_staged(&path).await, "missing file");
    std::fs::write(&path, b"not runnable").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!deploy::is_staged(&path).await, "not executable");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(deploy::is_staged(&path).await);
}
