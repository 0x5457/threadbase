//! Real-adapter contract smoke. Ignored by default; requires Codex login.
//!
//! Run with:
//!   cargo test -p threadbase --test real_codex -- --ignored --nocapture

use sqlx::SqlitePool;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::app::AppState;
use threadbase_server::auth::CurrentUser;
use threadbase_server::{auth, db, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest};
use threadbase_shared::ThreadEvent;
use tokio::time::{sleep, Duration};

fn init_repo(path: &Path) {
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "contract@example.com"],
        vec!["config", "user.name", "Contract"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    std::fs::write(path.join("README.md"), "contract smoke\n").unwrap();
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["commit", "-m", "initial"])
        .status()
        .unwrap()
        .success());
}

async fn wait_for(
    pool: &SqlitePool,
    thread_id: &str,
    predicate: impl Fn(&ThreadEvent) -> bool,
) -> Option<ThreadEvent> {
    for _ in 0..240 {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT payload_json FROM events WHERE thread_id = ? ORDER BY seq ASC")
                .bind(thread_id)
                .fetch_all(pool)
                .await
                .unwrap();
        for (json,) in rows {
            let event: ThreadEvent = serde_json::from_str(&json).unwrap();
            if predicate(&event) {
                return Some(event);
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    None
}

#[tokio::test]
#[ignore = "requires codex-acp and a logged-in Codex account"]
async fn real_codex_streams_a_reply() {
    std::env::remove_var("THREADBASE_ACP_COMMAND");
    let repo = TempDir::new().unwrap();
    init_repo(repo.path());
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
    let state = AppState::new(
        threadbase_server::config::Config {
            data_dir: data.path().to_path_buf(),
            db_path: data.path().join("threadbase.db"),
            bind: "127.0.0.1:7777".parse().unwrap(),
            ..threadbase_server::config::Config::default()
        },
        pool,
    );
    let project = projects::create(
        &state.pool,
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
        &state.pool,
        &state.projection,
        data.path(),
        &current,
        project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Real Codex smoke".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
            worktree: None,
        },
    )
    .await
    .unwrap();
    state
        .agents
        .ensure_started(thread.id.as_str())
        .await
        .expect("codex-acp should start");
    state
        .agents
        .send(
            thread.id.as_str(),
            &current,
            "Reply with exactly the single word: pong",
            None,
        )
        .await
        .expect("prompt should be accepted");
    let ended = wait_for(&state.pool, thread.id.as_str(), |event| {
        matches!(
            event,
            ThreadEvent::TurnEnded { .. } | ThreadEvent::TurnError { .. }
        )
    })
    .await
    .expect("real Codex turn should finish");
    eprintln!("real codex terminal event: {ended:?}");
    assert!(
        matches!(ended, ThreadEvent::TurnEnded { .. }),
        "Codex turn failed: {ended:?}"
    );
    let messages: Vec<String> = sqlx::query_scalar(
        "SELECT payload_json FROM events WHERE thread_id = ? AND kind = 'message_completed'",
    )
    .bind(thread.id.as_str())
    .fetch_all(&state.pool)
    .await
    .unwrap();
    eprintln!("real codex messages: {messages:?}");
    assert!(!messages.is_empty(), "Codex produced no completed message");
}
