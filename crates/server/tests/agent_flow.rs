//! End-to-end ACP flow against the scripted fake agent (no model credentials).

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::app::AppState;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::EventStore;
use threadbase_server::{agents, auth, db, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest};
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

/// Issues a real session cookie value for an identity created by `fixture`.
async fn session_for(fixture: &Fixture, github_id: i64) -> String {
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE identity_key = ?")
        .bind(format!("github:{github_id}"))
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    threadbase_server::auth_session::create(
        &fixture.state.pool,
        &user_id,
        "github",
        None,
        None,
        3600,
    )
    .await
    .unwrap()
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
            public_origin: None,
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

async fn max_seq(pool: &SqlitePool, thread_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?")
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn wait_for(
    pool: &SqlitePool,
    thread_id: &str,
    predicate: impl Fn(&ThreadEvent) -> bool,
) -> ThreadEvent {
    wait_for_after(pool, thread_id, 0, predicate).await
}

async fn wait_for_after(
    pool: &SqlitePool,
    thread_id: &str,
    after_seq: i64,
    predicate: impl Fn(&ThreadEvent) -> bool,
) -> ThreadEvent {
    for _ in 0..200 {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT payload_json, seq FROM events WHERE thread_id = ? AND seq > ? ORDER BY seq ASC",
        )
        .bind(thread_id)
        .bind(after_seq)
        .fetch_all(pool)
        .await
        .unwrap();
        for (json, _) in rows {
            let event: ThreadEvent = serde_json::from_str(&json).unwrap();
            if predicate(&event) {
                return event;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for event after seq {after_seq}");
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
    let before_permission_turn = max_seq(&fixture.state.pool, fixture.thread.id.as_str()).await;

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
    threadbase_server::approvals::decide(
        &fixture.state.pool,
        &fixture.state.projection,
        &fixture.state.events,
        approval.id.as_str(),
        &fixture.user,
        ApprovalDecision::AllowThread,
        None,
    )
    .await
    .unwrap();
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
    eprintln!("waiting for stream turn end");
    wait_for_after(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        before_permission_turn,
        |event| matches!(event, ThreadEvent::TurnEnded { .. }),
    )
    .await;
    eprintln!("got stream turn end");
    let before_elicit_turn = max_seq(&fixture.state.pool, fixture.thread.id.as_str()).await;

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
    threadbase_server::elicitations::respond_for_user(
        &fixture.state.pool,
        &fixture.state.projection,
        &fixture.state.events,
        elicitation.id.as_str(),
        &fixture.user,
        ElicitationAction::Accept,
        Some(serde_json::json!({"environment":"staging"})),
    )
    .await
    .unwrap();
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
    wait_for_after(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        before_elicit_turn,
        |event| matches!(event, ThreadEvent::TurnEnded { .. }),
    )
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
        &fixture.state.projection,
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
        fixture.state.projection.clone(),
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
    // A fresh GitHub account lands on `pending` access, which authz clamps to
    // viewer: no thread-level grant is needed to read, and none can raise it.
    auth::ensure_identity_user(
        &fixture.state.pool,
        &auth::Identity::github(3, "viewer", Some("Viewer"), None),
        &[],
    )
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
    let viewer_session = session_for(&fixture, 3).await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/approvals/{}/decide", approval.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={viewer_session}"))
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
    let manager = agents::AgentManager::new(
        fixture.state.pool.clone(),
        fixture.state.events.clone(),
        fixture.state.projection.clone(),
    );
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

#[tokio::test]
async fn first_turn_replaces_the_default_thread_title() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread = threads::create(
        &fixture.state.pool,
        &fixture.state.projection,
        fixture.data.path(),
        &fixture.user,
        fixture.project.id.as_str(),
        &CreateThreadRequest {
            title: Some("New thread".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
        },
    )
    .await
    .unwrap();
    fixture
        .state
        .agents
        .ensure_started(thread.id.as_str())
        .await
        .unwrap();
    fixture
        .state
        .agents
        .send(
            thread.id.as_str(),
            &fixture.user,
            "stream",
            Some(threadbase_shared::api::DeliveryMode::Auto),
        )
        .await
        .unwrap();
    wait_for(&fixture.state.pool, thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    let title: String = sqlx::query_scalar("SELECT title FROM threads WHERE id = ?")
        .bind(thread.id.as_str())
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(title, "stream");
}

#[tokio::test]
async fn inter_session_messages_are_persisted_as_thread_events() {
    let Some(fixture) = fixture().await else {
        return;
    };
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
            "agent-message",
            Some(threadbase_shared::api::DeliveryMode::Auto),
        )
        .await
        .unwrap();
    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::AgentMessage { .. })
    })
    .await;
    let ThreadEvent::AgentMessage {
        direction,
        from,
        to,
        text,
    } = event
    else {
        unreachable!();
    };
    assert!(format!("{direction:?}").eq_ignore_ascii_case("sent"));
    assert_eq!(from, "sess-1");
    assert_eq!(to, "sub-1");
    assert_eq!(text, "peer message");
}

#[tokio::test]
async fn agent_prompt_includes_state_summary_when_pending() {
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
    // Make the projection non-trivial so the summary is injected.
    threadbase_server::approvals::create(
        &fixture.state.pool,
        &fixture.state.projection,
        &fixture.state.events,
        threadbase_server::approvals::NewApproval {
            id: ulid::Ulid::new().to_string(),
            thread_id: fixture.thread.id.0.clone(),
            subagent_id: None,
            turn_id: None,
            tool_call_id: None,
            agent: "codex".to_owned(),
            title: "Write file".to_owned(),
            kind: "file_write".to_owned(),
            command: Some("echo hi".to_owned()),
            cwd: None,
            paths: Vec::new(),
            options: Vec::new(),
            input: None,
        },
        threadbase_shared::Actor::agent("codex"),
        threadbase_server::events::EventMeta::default(),
    )
    .await
    .unwrap();
    fixture
        .state
        .agents
        .send(fixture.thread.id.as_str(), &fixture.user, "stream", None)
        .await
        .unwrap();
    wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::ToolCall { .. })
    })
    .await;
    // The backend projection and the agent summary must report the same count.
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        pending, 1,
        "backend projection must report one pending approval"
    );
    let journal = std::fs::read_to_string(fixture.data.path().join("journal.log")).unwrap();
    assert!(
        journal.contains("pending_approvals=1"),
        "agent prompt should report the same pending count; journal:\n{journal}"
    );
}

#[tokio::test]
async fn draft_send_preserves_multiple_authors() {
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
    let ada = threadbase_shared::UserBrief {
        id: fixture.user.user.id.clone(),
        username: fixture.user.user.username.clone(),
        display_name: fixture.user.user.display_name.clone(),
        avatar_url: fixture.user.user.avatar_url.clone(),
        color: threadbase_shared::user_color(fixture.user.user.id.as_str()).to_owned(),
    };
    let bob = threadbase_shared::UserBrief {
        id: threadbase_shared::UserId::from("bob-id"),
        username: "bob".into(),
        display_name: "Bob".into(),
        avatar_url: None,
        color: "#0f0".into(),
    };
    let response = fixture
        .state
        .agents
        .send_with_authors(
            fixture.thread.id.as_str(),
            &fixture.user,
            "shared draft",
            None,
            &[ada, bob],
            &[],
        )
        .await
        .unwrap();
    assert_eq!(response.delivery, threadbase_shared::Delivery::Injected);
    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    match event {
        ThreadEvent::TurnStarted { authors, .. } => {
            assert_eq!(authors.len(), 2, "both draft authors must be preserved");
            assert_eq!(authors[0].display_name, "Ada");
            assert_eq!(authors[1].display_name, "Bob");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn draft_route_preserves_multiple_authors() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let bob = auth::ensure_identity_user(
        &fixture.state.pool,
        &auth::Identity::github(4, "bob", Some("Bob"), None),
        &[],
    )
    .await
    .unwrap();
    // Seed the draft with two per-author blocks via the persisted snapshot.
    let mut draft = threadbase_draft::DraftDoc::new();
    threadbase_draft::seed_text(&mut draft, fixture.user.user.id.as_str(), &["ada part"]).unwrap();
    threadbase_draft::seed_text(&mut draft, bob.id.as_str(), &["bob part"]).unwrap();
    let blob = draft.encode_full();
    sqlx::query(
        "INSERT INTO draft_snapshots(thread_id, upto_seq, state_blob, updated_at)
         VALUES(?, 0, ?, ?)",
    )
    .bind(fixture.thread.id.as_str())
    .bind(&blob)
    .bind(threadbase_shared::now_ms())
    .execute(&fixture.state.pool)
    .await
    .unwrap();

    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();

    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, 1).await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/messages", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(
            r#"{"text":"","delivery":"auto","fromDraft":true}"#,
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    match event {
        ThreadEvent::TurnStarted {
            authors, prompt, ..
        } => {
            assert_eq!(authors.len(), 2, "route must collect both block authors");
            assert_eq!(authors[0].display_name, "Ada");
            assert_eq!(authors[1].display_name, "Bob");
            assert!(prompt.contains("ada part") && prompt.contains("bob part"));
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn queued_draft_route_preserves_multiple_authors() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let bob = auth::ensure_identity_user(
        &fixture.state.pool,
        &auth::Identity::github(4, "bob", Some("Bob"), None),
        &[],
    )
    .await
    .unwrap();
    let mut draft = threadbase_draft::DraftDoc::new();
    threadbase_draft::seed_text(&mut draft, fixture.user.user.id.as_str(), &["ada part"]).unwrap();
    threadbase_draft::seed_text(&mut draft, bob.id.as_str(), &["bob part"]).unwrap();
    let blob = draft.encode_full();
    sqlx::query(
        "INSERT INTO draft_snapshots(thread_id, upto_seq, state_blob, updated_at)
         VALUES(?, 0, ?, ?)",
    )
    .bind(fixture.thread.id.as_str())
    .bind(&blob)
    .bind(threadbase_shared::now_ms())
    .execute(&fixture.state.pool)
    .await
    .unwrap();

    fixture
        .state
        .agents
        .ensure_started(fixture.thread.id.as_str())
        .await
        .unwrap();

    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, 1).await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/messages", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(
            r#"{"text":"","delivery":"queue","fromDraft":true}"#,
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The queue worker restores the stored author list when it sends.
    fixture.state.agents.kick_queue(fixture.thread.id.as_str());
    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    match event {
        ThreadEvent::TurnStarted {
            authors, prompt, ..
        } => {
            assert_eq!(authors.len(), 2, "queue must restore both block authors");
            assert_eq!(authors[0].display_name, "Ada");
            assert_eq!(authors[1].display_name, "Bob");
            assert!(prompt.contains("ada part") && prompt.contains("bob part"));
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn empty_draft_send_returns_already_sent() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

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

    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, 1).await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/messages", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(
            r#"{"text":"","delivery":"auto","fromDraft":true}"#,
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("draft already sent"), "body: {text}");
}

#[tokio::test]
async fn failed_send_retains_draft() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    // Force agent startup to fail: the route must not consume the draft.
    std::env::set_var(
        "THREADBASE_ACP_COMMAND",
        "/nonexistent/threadbase_acp_fixture",
    );

    let mut draft = threadbase_draft::DraftDoc::new();
    threadbase_draft::seed_text(&mut draft, fixture.user.user.id.as_str(), &["keep me"]).unwrap();
    let blob = draft.encode_full();
    sqlx::query(
        "INSERT INTO draft_snapshots(thread_id, upto_seq, state_blob, updated_at)
         VALUES(?, 0, ?, ?)",
    )
    .bind(fixture.thread.id.as_str())
    .bind(&blob)
    .bind(threadbase_shared::now_ms())
    .execute(&fixture.state.pool)
    .await
    .unwrap();

    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, 1).await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/messages", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(
            r#"{"text":"","delivery":"auto","fromDraft":true}"#,
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let (text, authors) = fixture
        .state
        .drafts
        .peek_draft(fixture.thread.id.as_str())
        .await
        .unwrap();
    assert_eq!(text, "keep me", "failed send must retain the draft");
    assert_eq!(authors.len(), 1);
}

#[tokio::test]
async fn queued_turn_writes_current_turn() {
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
    let ada = threadbase_shared::UserBrief {
        id: fixture.user.user.id.clone(),
        username: fixture.user.user.username.clone(),
        display_name: fixture.user.user.display_name.clone(),
        avatar_url: fixture.user.user.avatar_url.clone(),
        color: threadbase_shared::user_color(fixture.user.user.id.as_str()).to_owned(),
    };
    let response = fixture
        .state
        .agents
        .send_with_authors(
            fixture.thread.id.as_str(),
            &fixture.user,
            "permission",
            Some(threadbase_shared::DeliveryMode::Queue),
            &[ada],
            &[],
        )
        .await
        .unwrap();
    assert_eq!(response.delivery, threadbase_shared::Delivery::Queued);

    fixture.state.agents.kick_queue(fixture.thread.id.as_str());
    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    let ThreadEvent::TurnStarted { turn_id, .. } = event else {
        unreachable!();
    };
    // The queue worker writes current_turn_id after appending TurnStarted;
    // poll briefly to avoid a read-before-write race.
    let mut stored: Option<String> = None;
    for _ in 0..40 {
        stored = sqlx::query_scalar::<_, Option<String>>(
            "SELECT current_turn_id FROM threads WHERE id = ?",
        )
        .bind(fixture.thread.id.as_str())
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
        if stored.as_deref() == Some(turn_id.as_str()) {
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        stored.as_deref(),
        Some(turn_id.as_str()),
        "queued turn must persist current_turn_id"
    );
}

#[tokio::test]
async fn snapshot_revisions_are_monotonic() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let first = fixture
        .state
        .projection
        .snapshot(fixture.thread.id.as_str())
        .await
        .unwrap();
    let second = fixture
        .state
        .projection
        .snapshot(fixture.thread.id.as_str())
        .await
        .unwrap();
    assert!(
        second.revision > first.revision,
        "snapshot revisions must be strictly increasing ({} -> {})",
        first.revision,
        second.revision
    );
}

#[tokio::test]
async fn pasted_images_upload_serve_and_reach_the_agent() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

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
    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, 1).await;
    let png: &[u8] = b"\x89PNG\r\n\x1a\nimage-bytes";

    let mut upload = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/attachments", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "image/png")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(png.to_vec()))
        .unwrap();
    upload
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.clone().oneshot(upload).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let attachment: threadbase_shared::AttachmentDto = serde_json::from_slice(&body).unwrap();
    assert_eq!(attachment.mime_type, "image/png");
    assert_eq!(attachment.byte_size, i64::try_from(png.len()).unwrap());

    let mut read = Request::builder()
        .method("GET")
        .uri(format!("/api/attachments/{}", attachment.id))
        .header("host", "localhost")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::empty())
        .unwrap();
    read.extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.clone().oneshot(read).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let served = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(served.as_ref(), png);

    // The prompt must carry the image block to the agent, and the turn event
    // must remember the attachment so a reload still renders it.
    let after = max_seq(&fixture.state.pool, fixture.thread.id.as_str()).await;
    let author = threadbase_server::agents::user_brief(&fixture.user);
    let attachments = threadbase_server::attachments::resolve(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        &[attachment.id],
    )
    .await
    .unwrap();
    fixture
        .state
        .agents
        .send_with_authors(
            fixture.thread.id.as_str(),
            &fixture.user,
            "image",
            None,
            &[author],
            &attachments,
        )
        .await
        .unwrap();

    let started = wait_for_after(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        after,
        |event| matches!(event, ThreadEvent::TurnStarted { .. }),
    )
    .await;
    let ThreadEvent::TurnStarted { attachments, .. } = started else {
        unreachable!();
    };
    assert_eq!(attachments.len(), 1);

    let reply = wait_for_after(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        after,
        |event| matches!(event, ThreadEvent::MessageDelta { text, .. } if text.contains("images:")),
    )
    .await;
    let ThreadEvent::MessageDelta { text, .. } = reply else {
        unreachable!();
    };
    assert_eq!(text, "images:1");
}
