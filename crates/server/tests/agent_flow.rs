//! End-to-end ACP flow against the scripted fake agent (no model credentials).

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::EventStore;
use threadbase_server::state::AppState;
use threadbase_server::{agents, auth, db, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest, SetupRequest};
use threadbase_shared::{ApprovalDecision, ElicitationAction, ThreadEvent};
use tokio::time::{sleep, Duration};

struct Fixture {
    repo: TempDir,
    data: TempDir,
    state: AppState,
    user: CurrentUser,
    project: threadbase_shared::ProjectDto,
    thread: threadbase_shared::ThreadDto,
}

fn fixture_binary() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new("cargo")
        .current_dir(&root)
        .args([
            "build",
            "-p",
            "threadbase-agent",
            "--bin",
            "threadbase_acp_fixture",
        ])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let path = root.join("target/debug/threadbase_acp_fixture");
    path.is_file().then_some(path)
}

async fn fixture() -> Option<Fixture> {
    let binary = fixture_binary()?;
    std::env::set_var("THREADBASE_ACP_COMMAND", &binary);
    let repo = TempDir::new().unwrap();
    init_repo(repo.path());
    let data = TempDir::new().unwrap();
    let journal_path = data.path().join("journal.log");
    std::env::set_var("FAKE_AGENT_JOURNAL", &journal_path);
    let pool = db::connect_memory().await.unwrap();
    let user = auth::setup_admin(
        &pool,
        &SetupRequest {
            username: "ada".into(),
            password: "correct horse battery".into(),
            display_name: "Ada".into(),
        },
    )
    .await
    .unwrap();
    let current = CurrentUser {
        user: user.clone(),
        method: "session",
    };
    let state = AppState::new(
        threadbase_server::config::Config {
            data_dir: data.path().to_path_buf(),
            db_path: data.path().join("threadbase.db"),
            bind: "127.0.0.1:7777".parse().unwrap(),
            public_origin: None,
            log_json: false,
        },
        pool,
    );
    let project = projects::create(
        &state.pool,
        &current,
        &CreateProjectRequest {
            path: repo.path().to_string_lossy().to_string(),
            name: None,
        },
    )
    .await
    .unwrap();
    let thread = threads::create(
        &state.pool,
        data.path(),
        &current,
        project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Agent flow".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
        },
    )
    .await
    .unwrap();
    Some(Fixture {
        repo,
        data,
        state,
        user: current,
        project,
        thread,
    })
}

fn init_repo(path: &Path) {
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    std::fs::write(path.join("README.md"), "hello\n").unwrap();
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
) -> ThreadEvent {
    for _ in 0..100 {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT payload_json FROM events WHERE thread_id = ? ORDER BY seq ASC")
                .bind(thread_id)
                .fetch_all(pool)
                .await
                .unwrap();
        for (json,) in rows {
            let event: ThreadEvent = serde_json::from_str(&json).unwrap();
            if predicate(&event) {
                return event;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for event");
}

#[tokio::test]
async fn fake_agent_streams_tools_and_permissions() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();
    let response = fixture
        .state
        .agents
        .send(fixture.thread.id.as_str(), &fixture.user, "stream", None)
        .await
        .unwrap();
    assert_eq!(response.delivery, threadbase_shared::Delivery::Injected);
    eprintln!("waiting for stream tool call");
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::ToolCall { .. })
    })
    .await;
    eprintln!("got stream tool call");
    eprintln!("waiting for stream turn end");
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    eprintln!("got stream turn end");

    fixture
        .state
        .agents
        .send(
            fixture.thread.id.as_str(),
            &fixture.user,
            "permission",
            None,
        )
        .await
        .unwrap();
    eprintln!("waiting for permission");
    let approval = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::PermissionRequested { .. })
    })
    .await;
    eprintln!("got permission");
    let ThreadEvent::PermissionRequested { approval } = approval else {
        unreachable!();
    };
    fixture
        .state
        .agents
        .decide(
            fixture.thread.id.as_str(),
            approval.id.as_str(),
            ApprovalDecision::AllowThread,
        )
        .await
        .unwrap();
    threadbase_server::approvals::decide(
        &fixture.state.pool,
        &fixture.state.events,
        approval.id.as_str(),
        &fixture.user,
        ApprovalDecision::AllowThread,
        None,
    )
    .await
    .unwrap();
    eprintln!("waiting for stream turn end");
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    eprintln!("got stream turn end");

    fixture
        .state
        .agents
        .send(fixture.thread.id.as_str(), &fixture.user, "elicit", None)
        .await
        .unwrap();
    eprintln!("waiting for elicit");
    let question = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::ElicitationRequested { .. })
    })
    .await;
    eprintln!("got elicit");
    let ThreadEvent::ElicitationRequested { elicitation } = question else {
        unreachable!();
    };
    fixture
        .state
        .agents
        .respond_elicitation(
            fixture.thread.id.as_str(),
            elicitation.id.as_str(),
            ElicitationAction::Accept,
            Some(serde_json::json!({"environment":"staging"})),
        )
        .await
        .unwrap();
    threadbase_server::approvals::respond_elicitation(
        &fixture.state.pool,
        &fixture.state.events,
        elicitation.id.as_str(),
        &fixture.user,
        ElicitationAction::Accept,
        Some(serde_json::json!({"environment":"staging"})),
    )
    .await
    .unwrap();
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    // A standing thread rule must auto-allow the next matching request without a UI decision.
    let rules = threadbase_server::approvals::list_rules(&fixture.state.pool)
        .await
        .unwrap();
    assert!(!rules.is_empty(), "AllowThread should persist a rule");
    let matched = threadbase_server::approvals::match_rule(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        "codex",
        "file_write",
        Some("echo hi"),
        None,
        false,
    )
    .await
    .unwrap();
    eprintln!("rule match: {matched:?}; rules={rules:?}");
    fixture
        .state
        .agents
        .send(
            fixture.thread.id.as_str(),
            &fixture.user,
            "permission",
            None,
        )
        .await
        .unwrap();
    for _ in 0..100 {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
        )
        .bind(fixture.thread.id.as_str())
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
        let resolved: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'allowed'",
        )
        .bind(fixture.thread.id.as_str())
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
        if pending == 0 && resolved >= 2 {
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        pending, 0,
        "standing rule should auto-allow the next request"
    );

    // A thread created with an initial prompt must actually start the agent and run it.
    let second = threads::create(
        &fixture.state.pool,
        fixture.data.path(),
        &fixture.user,
        fixture.project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Initial prompt".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: Some("stream".into()),
        },
    )
    .await
    .unwrap();
    fixture
        .state
        .agents
        .ensure_started(second.id.as_str())
        .await
        .unwrap();
    fixture.state.agents.kick_queue(second.id.as_str());
    wait_for(&fixture.state.pool, second.id.as_str(), |event| {
        matches!(event, ThreadEvent::ToolCall { .. })
    })
    .await;
    eprintln!("initial prompt started the agent");

    eprintln!(
        "journal path: {}",
        fixture.data.path().join("journal.log").display()
    );
    let _ = std::fs::copy(
        fixture.data.path().join("journal.log"),
        "/tmp/agent_flow_journal.log",
    );
    let _ = (&fixture.repo, &fixture.data, &fixture.project);
    let _ = EventStore::new(fixture.state.pool.clone());
    let _ = agents::AgentManager::new(
        fixture.state.pool.clone(),
        EventStore::new(fixture.state.pool.clone()),
    );
}

#[tokio::test]
async fn viewer_cannot_unblock_a_tool_through_the_http_route() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let viewer = auth::create_local_user(
        &fixture.state.pool,
        &threadbase_shared::api::CreateUserRequest {
            username: "viewer".into(),
            password: "correct horse battery".into(),
            display_name: "Viewer".into(),
            is_admin: false,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, 'viewer', ?)",
    )
    .bind(fixture.thread.id.as_str())
    .bind(viewer.id.as_str())
    .bind(threadbase_shared::now_ms())
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let token = auth::issue_session(&fixture.state.pool, viewer.id.as_str(), "127.0.0.1", None)
        .await
        .unwrap();
    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();
    fixture
        .state
        .agents
        .send(
            fixture.thread.id.as_str(),
            &fixture.user,
            "permission",
            None,
        )
        .await
        .unwrap();
    let approval = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::PermissionRequested { .. })
    })
    .await;
    let ThreadEvent::PermissionRequested { approval } = approval else {
        unreachable!();
    };

    let app = threadbase_server::routes::router(fixture.state.clone());
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/approvals/{}/decide", approval.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={token}"))
        .body(Body::from(r#"{"decision":"allow_once","note":null}"#))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    sleep(Duration::from_millis(800)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM approvals WHERE id = ?")
        .bind(approval.id.as_str())
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    let completed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'tool_call_update'
         AND payload_json LIKE '%\"status\":\"completed\"%'",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(completed, 0, "viewer must not unblock the agent tool");
}

#[tokio::test]
async fn cancelling_a_turn_cancels_pending_elicitations() {
    let Some(fixture) = fixture().await else {
        return;
    };
    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO elicitations(id, thread_id, agent, message, mode, schema_json, blocking, status, created_at)
         VALUES('elic-cancel', ?, 'codex', 'Continue?', 'form', '{}', 1, 'pending', ?)",
    )
    .bind(fixture.thread.id.as_str())
    .bind(threadbase_shared::now_ms())
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    fixture
        .state
        .agents
        .cancel(fixture.thread.id.as_str())
        .await
        .unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM elicitations WHERE id = 'elic-cancel'")
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(status, "cancelled");
    let resolved: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'elicitation_resolved'",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(resolved, 1);
}

#[tokio::test]
async fn failed_session_restore_emits_a_context_reset_notice() {
    let Some(fixture) = fixture().await else {
        return;
    };
    sqlx::query("UPDATE agent_bindings SET session_id = 'expired' WHERE thread_id = ?")
        .bind(fixture.thread.id.as_str())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let manager =
        agents::AgentManager::new(fixture.state.pool.clone(), fixture.state.events.clone());
    manager
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();
    let notice: Option<String> = sqlx::query_scalar(
        "SELECT payload_json FROM events WHERE thread_id = ? AND kind = 'system_notice'
         AND payload_json LIKE '%context was reset%' LIMIT 1",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_optional(&fixture.state.pool)
    .await
    .unwrap();
    assert!(notice.is_some());
}

#[tokio::test]
async fn turn_start_snapshots_the_worktree_for_last_turn_baseline() {
    let Some(fixture) = fixture().await else {
        return;
    };
    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();
    let worktree = PathBuf::from(
        threads::load_row(&fixture.state.pool, fixture.thread.id.as_str())
            .await
            .unwrap()
            .worktree_path
            .unwrap(),
    );
    std::fs::write(worktree.join("README.md"), "changed before turn\n").unwrap();
    let base = threads::load_row(&fixture.state.pool, fixture.thread.id.as_str())
        .await
        .unwrap()
        .base_sha
        .unwrap();
    fixture
        .state
        .agents
        .send(
            fixture.thread.id.as_str(),
            &fixture.user,
            "stream",
            Some(threadbase_shared::api::DeliveryMode::Auto),
        )
        .await
        .unwrap();
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    let row = threads::load_row(&fixture.state.pool, fixture.thread.id.as_str())
        .await
        .unwrap();
    let head = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    assert_eq!(row.last_turn_sha.as_deref(), Some(head.as_str()));
    assert_ne!(row.last_turn_sha.as_deref(), Some(base.as_str()));
}
