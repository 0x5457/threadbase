//! End-to-end ACP flow against the scripted fake agent (no model credentials).

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::app::AppState;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::EventStore;
use threadbase_server::{agents, auth, db, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest, UpdateThreadRequest};
use threadbase_shared::{ApprovalDecision, ElicitationAction, ThreadEvent, ThreadStatus};
use tokio::time::{sleep, Duration};

fn policy() -> threadbase_core::permissions::PermissionPolicy {
    threadbase_core::permissions::PermissionPolicy::default()
}

struct Fixture {
    repo: TempDir,
    data: TempDir,
    state: AppState,
    user: CurrentUser,
    project: threadbase_shared::ProjectDto,
    thread: threadbase_shared::ThreadDto,
}

/// Locate the scripted adapter once per process.
///
/// `cargo nextest` runs every test in its own process, so calling `cargo build`
/// from each of them made 20+ builds fight over the package-cache lock while
/// clippy was running, which is why crash-recovery waits occasionally timed
/// out. The binary sits next to the test executable after any workspace build;
/// only build when it is genuinely missing.
fn fixture_binary() -> Option<PathBuf> {
    static BINARY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Ok(current) = std::env::current_exe() {
                if let Some(dir) = current.parent().and_then(Path::parent) {
                    let candidate = dir.join("threadbase_acp_fixture");
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
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
        })
        .clone()
}

/// Issues a real session cookie value for an identity created by `fixture`.
async fn session_for(fixture: &Fixture, login: &str) -> String {
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE identity_key = ?")
        .bind(format!("tailscale:{login}@example.com"))
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    threadbase_server::auth_session::create(
        &fixture.state.pool,
        &user_id,
        "tailscale",
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
        &auth::Identity::tailscale("ada@example.com", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let current = CurrentUser {
        user: user.clone(),
        method: "tailscale",
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
            title: Some("Agent flow".into()),
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
    // 30s, not 10s: these tests spawn real processes and are run alongside the
    // other quality gates. A real regression still fails, just with room for a
    // loaded machine to schedule the adapter.
    for _ in 0..600 {
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
    let seen: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT kind FROM events WHERE thread_id = ? ORDER BY seq DESC LIMIT 12",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    panic!(
        "timed out waiting for event after seq {after_seq}; status={status:?}; last events={seen:?}"
    );
}

/// Regression: pi reports an upstream model failure (a provider 402, an expired
/// key) as an empty assistant message and still answers the prompt with
/// `end_turn`, so the thread looked alive but never replied: nothing was
/// written to the transcript and the status went straight back to idle.
#[tokio::test]
async fn a_turn_without_output_is_reported_as_a_failure() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;

    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "silent", None)
        .await
        .unwrap();

    let event = wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(
            event,
            ThreadEvent::TurnError { .. } | ThreadEvent::TurnEnded { .. }
        )
    })
    .await;
    let ThreadEvent::TurnError { message, .. } = event else {
        panic!("a turn that produced nothing must not end as a success: {event:?}");
    };
    assert!(
        message.contains("without any output"),
        "unexpected message: {message}"
    );
    assert_eq!(
        threads::status(&fixture.state.pool, thread_id)
            .await
            .unwrap(),
        ThreadStatus::Error
    );

    // The thread stays usable: the next prompt runs again.
    let after = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "hello", None)
        .await
        .unwrap();
    wait_for_after(&fixture.state.pool, thread_id, after, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
}

/// Regression: the adapter named permission requests with a per-session counter
/// (`perm-1`, `perm-2`, …) while `approvals.id` is a primary key shared by every
/// thread. The second thread to hit the counter collided, the insert failed, the
/// request was never answered and the agent waited on it until the user
/// cancelled. One thread sitting in `running` forever was the only symptom.
#[tokio::test]
async fn approval_ids_stay_unique_across_threads() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let second = threads::create(
        &fixture.state.pool,
        &fixture.state.projection,
        fixture.data.path(),
        &fixture.user,
        fixture.project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Second thread".into()),
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

    let mut ids = Vec::new();
    for thread_id in [fixture.thread.id.as_str(), second.id.as_str()] {
        let before = max_seq(&fixture.state.pool, thread_id).await;
        fixture
            .state
            .agents
            .ensure_started(thread_id)
            .await
            .unwrap();
        fixture
            .state
            .agents
            .send(thread_id, &fixture.user, "permission", None)
            .await
            .unwrap();
        let event = wait_for_after(&fixture.state.pool, thread_id, before, |event| {
            matches!(event, ThreadEvent::PermissionRequested { .. })
        })
        .await;
        let ThreadEvent::PermissionRequested { approval } = event else {
            unreachable!("filtered for a permission request");
        };
        fixture
            .state
            .agents
            .decide(
                thread_id,
                approval.id.as_str(),
                ApprovalDecision::AllowOnce,
                None,
            )
            .await
            .unwrap();
        ids.push(approval.id.as_str().to_owned());
    }
    assert_ne!(
        ids[0], ids[1],
        "approval ids are global primary keys: {ids:?}"
    );
}

#[tokio::test]
async fn a_choice_uses_the_exact_option_id_not_the_decision_kind() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "permission-choice", None)
        .await
        .unwrap();
    let event = wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(event, ThreadEvent::PermissionRequested { .. })
    })
    .await;
    let ThreadEvent::PermissionRequested { approval } = event else {
        unreachable!();
    };
    let disabled = approval
        .options
        .iter()
        .find(|option| option.label.starts_with("Disabled"))
        .expect("the adapter offers a disabled choice");
    assert_eq!(disabled.kind, ApprovalDecision::AllowOnce);

    threadbase_server::approvals::decide(
        &fixture.state.pool,
        &fixture.state.projection,
        &fixture.state.events,
        approval.id.as_str(),
        &fixture.user,
        ApprovalDecision::AllowOnce,
        None,
    )
    .await
    .unwrap();
    fixture
        .state
        .agents
        .decide(
            thread_id,
            approval.id.as_str(),
            ApprovalDecision::AllowOnce,
            Some(disabled.id.as_str()),
        )
        .await
        .unwrap();

    wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(
            event,
            ThreadEvent::ToolCallUpdate { call }
                if call.id.as_str() == "call-perm"
                    && call
                        .output
                        .as_deref()
                        .is_some_and(|output| output.contains("opt-disabled"))
        )
    })
    .await;
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
            None,
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
            worktree: None,
        },
        &policy(),
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
        threadbase_server::presence::PresenceHub::new(),
        0,
        0,
        policy(),
    );
}

#[tokio::test]
async fn worktree_setup_runs_before_the_agent_can_start() {
    let Some(fixture) = fixture().await else {
        return;
    };
    sqlx::query("UPDATE projects SET setup_command = 'printf ready > setup.txt' WHERE id = ?")
        .bind(fixture.project.id.as_str())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let thread = threads::create(
        &fixture.state.pool,
        &fixture.state.projection,
        fixture.data.path(),
        &fixture.user,
        fixture.project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Setup".into()),
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
    assert!(
        threadbase_server::setup::maybe_start(&fixture.state, thread.id.as_str())
            .await
            .unwrap()
    );
    let mut row = threads::load_row(&fixture.state.pool, thread.id.as_str())
        .await
        .unwrap();
    for _ in 0..100 {
        if row.setup_status != "pending" && row.setup_status != "running" {
            break;
        }
        sleep(Duration::from_millis(50)).await;
        row = threads::load_row(&fixture.state.pool, thread.id.as_str())
            .await
            .unwrap();
    }
    assert_eq!(row.setup_status, "succeeded", "{:?}", row.setup_error);
    let worktree = row.worktree_path.as_deref().unwrap();
    assert_eq!(
        tokio::fs::read_to_string(Path::new(worktree).join("setup.txt"))
            .await
            .unwrap(),
        "ready"
    );
}

#[tokio::test]
async fn a_failed_worktree_setup_is_recorded_and_does_not_start_the_agent() {
    let Some(fixture) = fixture().await else {
        return;
    };
    sqlx::query("UPDATE projects SET setup_command = 'exit 9' WHERE id = ?")
        .bind(fixture.project.id.as_str())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let thread = threads::create(
        &fixture.state.pool,
        &fixture.state.projection,
        fixture.data.path(),
        &fixture.user,
        fixture.project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Setup failure".into()),
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
    assert!(
        threadbase_server::setup::maybe_start(&fixture.state, thread.id.as_str())
            .await
            .unwrap()
    );
    let mut row = threads::load_row(&fixture.state.pool, thread.id.as_str())
        .await
        .unwrap();
    for _ in 0..100 {
        if row.setup_status != "pending" && row.setup_status != "running" {
            break;
        }
        sleep(Duration::from_millis(50)).await;
        row = threads::load_row(&fixture.state.pool, thread.id.as_str())
            .await
            .unwrap();
    }
    assert_eq!(row.setup_status, "failed");
    assert!(row
        .setup_error
        .as_deref()
        .is_some_and(|error| error.contains("exited with")));
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
    // A fresh tailnet account lands on `pending` access, which authz clamps to
    // viewer: no thread-level grant is needed to read, and none can raise it.
    auth::ensure_identity_user(
        &fixture.state.pool,
        &auth::Identity::tailscale("viewer@example.com", Some("Viewer"), None),
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
    let viewer_session = session_for(&fixture, "viewer").await;
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
async fn editing_a_prompt_rewinds_the_tail_and_starts_a_new_session() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "stream", None)
        .await
        .unwrap();
    let first = wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    let ThreadEvent::TurnStarted { message_id, .. } = &first else {
        unreachable!();
    };
    let first_id = message_id.0.clone();
    let first_seq: i64 = sqlx::query_scalar(
        "SELECT seq FROM events WHERE thread_id = ? AND kind = 'turn_started' ORDER BY seq ASC LIMIT 1",
    )
    .bind(thread_id)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;

    // A second turn is the branch the edit is supposed to remove.
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "slow", None)
        .await
        .unwrap();

    let response = fixture
        .state
        .agents
        .rewind_and_edit(thread_id, &first_id, "stream", &fixture.user)
        .await
        .unwrap();
    // The rewind shut the adapter down, so the edited prompt is queued and the
    // worker starts the fresh session instead of the request waiting for it.
    assert_eq!(response.delivery, threadbase_shared::Delivery::Queued);
    let rewritten = wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    let ThreadEvent::TurnStarted {
        prompt, message_id, ..
    } = rewritten
    else {
        unreachable!();
    };
    assert_eq!(prompt, "stream");
    assert_ne!(message_id.0, first_id);

    let prompts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'turn_started'",
    )
    .bind(thread_id)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(prompts, 1, "the discarded branch must leave no prompt");

    let old_seq: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE thread_id = ? AND seq = ?")
            .bind(thread_id)
            .bind(first_seq)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(old_seq, 0);

    // A fresh adapter process must have asked for a new session; resuming the
    // old one would put the removed turns back into the model context.
    let journal = std::fs::read_to_string(fixture.data.path().join("journal.log")).unwrap();
    assert!(
        journal.matches("\"method\":\"session/new\"").count() >= 2,
        "expected a second session/new after the rewind"
    );

    wait_for_after(&fixture.state.pool, thread_id, first_seq, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
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
        threadbase_server::presence::PresenceHub::new(),
        0,
        0,
        policy(),
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
            worktree: None,
        },
        &policy(),
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
        journal.contains("This thread is running."),
        "the summary must not say idle while a turn is current; journal:\n{journal}"
    );
    assert!(
        journal.contains("1 approval waiting for a person"),
        "the summary should match the projection; journal:\n{journal}"
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
        &auth::Identity::tailscale("bob@example.com", Some("Bob"), None),
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
    let session = session_for(&fixture, "ada").await;
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
        &auth::Identity::tailscale("bob@example.com", Some("Bob"), None),
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
    let session = session_for(&fixture, "ada").await;
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
    let session = session_for(&fixture, "ada").await;
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

/// A cold start can no longer fail inside the request: the route answers before
/// any process exists. The message must survive that late failure somewhere the
/// sender can see it, so the input stays in the queue instead of disappearing
/// with the draft.
#[tokio::test]
async fn failed_start_keeps_the_message_queued() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    // Force agent startup to fail after the route has already answered.
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
    let session = session_for(&fixture, "ada").await;
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sent: threadbase_shared::SendMessageResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent.delivery, threadbase_shared::Delivery::Queued);

    // The failure lands on the thread instead of the HTTP status.
    let failure = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnError { turn_id: None, .. })
    })
    .await;
    let ThreadEvent::TurnError { message, .. } = failure else {
        unreachable!();
    };
    assert!(
        !message.is_empty(),
        "the thread must say why the start failed"
    );

    // The message itself is still there, still sendable.
    let items =
        threadbase_server::queue::list_for_thread(&fixture.state.pool, fixture.thread.id.as_str())
            .await
            .unwrap();
    assert_eq!(items.len(), 1, "the failed start must not drop the input");
    assert_eq!(items[0].text, "keep me");
    assert_eq!(items[0].status, threadbase_shared::QueueStatus::Queued);

    // The draft was consumed because the message is durable in the queue.
    let (text, _) = fixture
        .state
        .drafts
        .peek_draft(fixture.thread.id.as_str())
        .await
        .unwrap();
    assert_eq!(text.trim(), "", "the text now lives in the queue");
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
async fn cold_send_queues_without_waiting_for_the_adapter() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    // No `ensure_started`: the thread has never run a turn, so this send would
    // have to launch the adapter under the old synchronous path.
    let response = fixture
        .state
        .agents
        .send(
            fixture.thread.id.as_str(),
            &fixture.user,
            "cold start",
            None,
        )
        .await
        .unwrap();
    assert_eq!(response.delivery, threadbase_shared::Delivery::Queued);
    assert_eq!(response.queue_position, Some(1));
    assert!(
        response.message_id.is_none(),
        "a queued input has no message until the worker starts the turn"
    );

    // The worker starts the adapter and turns the item into a prompt; the
    // sender does not have to kick it again.
    let event = wait_for(&fixture.state.pool, fixture.thread.id.as_str(), |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    let ThreadEvent::TurnStarted { prompt, .. } = event else {
        unreachable!();
    };
    assert_eq!(prompt, "cold start");
    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(fixture.thread.id.as_str())
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(queued, 0, "the dispatched item must leave the queue bar");
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
async fn uploaded_files_download_and_reach_the_prompt() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, "ada").await;
    let body = b"# Notes\nhello from a file";

    let mut upload = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/attachments", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "text/markdown")
        .header("x-attachment-name", "notes.md")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::from(body.to_vec()))
        .unwrap();
    upload
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.clone().oneshot(upload).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let attachment: threadbase_shared::AttachmentDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(attachment.mime_type, "text/markdown");
    assert_eq!(attachment.file_name.as_deref(), Some("notes.md"));

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
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("notes.md"));
    let served = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(served.as_ref(), body);

    let attachments = threadbase_server::attachments::resolve(
        &fixture.state.pool,
        fixture.thread.id.as_str(),
        &[attachment.id],
    )
    .await
    .unwrap();
    let parts = threadbase_server::attachments::prompt_parts(&fixture.state.pool, &attachments)
        .await
        .unwrap();
    let threadbase_agent::PromptPart::Text(text) = &parts[0] else {
        panic!("expected a text prompt part");
    };
    assert!(text.contains("notes.md"), "{text}");
    assert!(text.contains("hello from a file"), "{text}");
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
    let session = session_for(&fixture, "ada").await;
    let png: &[u8] = b"\x89PNG\r\n\x1a\nimage-bytes";

    let mut upload = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{}/attachments", fixture.thread.id))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "image/png")
        .header("x-attachment-name", "pasted.png")
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
    assert_eq!(attachment.file_name.as_deref(), Some("pasted.png"));

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

#[tokio::test]
async fn restart_resumes_the_interrupted_turn() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    // The previous server process died in the middle of a turn.
    sqlx::query(
        "UPDATE threads SET status = 'running', current_turn_id = 'turn-crashed' WHERE id = ?",
    )
    .bind(thread_id)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;

    fixture.state.agents.recover_after_restart().await.unwrap();

    // The cut-off turn is reported ...
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnError { .. })
    })
    .await;
    // ... and the agent picks the work back up without a human message.
    let started = wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnStarted { authors, .. }
            if authors.iter().any(|author| author.username == "threadbase"))
    })
    .await;
    let ThreadEvent::TurnStarted { author, prompt, .. } = &started else {
        unreachable!();
    };
    // The continuation is Threadbase's, not a message the creator sent.
    assert_eq!(author.username, "threadbase");
    assert!(prompt.contains("restarted"), "{prompt}");
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
}

#[tokio::test]
async fn recovers_after_the_agent_process_crashes() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;

    // The fake adapter exits in the middle of the turn.
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "crash", None)
        .await
        .unwrap();
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::ProcessExited { .. })
    })
    .await;

    // The adapter is replaced and the cut-off turn continues by itself.
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnStarted { authors, .. }
            if authors.iter().any(|author| author.username == "threadbase"))
    })
    .await;
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;

    // The thread is healthy again for ordinary input.
    let after = max_seq(&fixture.state.pool, thread_id).await;
    let response = fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "hello", None)
        .await
        .unwrap();
    assert!(response.message_id.is_some());
    wait_for_after(&fixture.state.pool, thread_id, after, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
}

#[tokio::test]
async fn explicit_restart_is_not_a_crash() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;

    fixture.state.agents.restart(thread_id).await.unwrap();

    // Give a misclassified exit time to surface.
    sleep(Duration::from_millis(400)).await;
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM events WHERE thread_id = ? AND seq > ? ORDER BY seq")
            .bind(thread_id)
            .bind(before)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert!(
        !kinds.iter().any(|kind| kind == "process_exited"),
        "{kinds:?}"
    );
    assert_eq!(
        threads::status(&fixture.state.pool, thread_id)
            .await
            .unwrap(),
        ThreadStatus::Idle
    );
}

#[tokio::test]
async fn queued_input_runs_when_the_thread_is_idle() {
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    let before = max_seq(&fixture.state.pool, thread_id).await;

    // Nothing is running and no adapter has been started: the queue worker
    // must start both on its own.
    let response = fixture
        .state
        .agents
        .send(
            thread_id,
            &fixture.user,
            "queued hello",
            Some(threadbase_shared::api::DeliveryMode::Queue),
        )
        .await
        .unwrap();
    assert_eq!(response.delivery, threadbase_shared::Delivery::Queued);

    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnStarted { prompt, .. } if prompt == "queued hello")
    })
    .await;
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
}

#[tokio::test]
async fn send_now_interrupts_a_non_steering_agent() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    // Model an adapter that refuses mid-turn input.
    std::env::set_var("FAKE_AGENT_STEERING", "0");
    let Some(fixture) = fixture().await else {
        eprintln!("skipping: ACP fixture binary could not be built");
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "slow", None)
        .await
        .unwrap();
    wait_for_after(
        &fixture.state.pool,
        thread_id,
        before,
        |event| matches!(event, ThreadEvent::MessageDelta { text, .. } if text.contains("working")),
    )
    .await;

    let queued = fixture
        .state
        .agents
        .send(
            thread_id,
            &fixture.user,
            "queued hello",
            Some(threadbase_shared::api::DeliveryMode::Queue),
        )
        .await
        .unwrap();
    assert_eq!(queued.delivery, threadbase_shared::Delivery::Queued);
    let item_id: String = sqlx::query_scalar(
        "SELECT id FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(thread_id)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();

    let app = threadbase_server::routes::router(fixture.state.clone());
    let session = session_for(&fixture, "ada").await;
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/threads/{thread_id}/queue/{item_id}/steer"))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("cookie", format!("threadbase_session={session}"))
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The running turn is interrupted and the queued message takes over.
    let started = wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnStarted { prompt, .. } if prompt == "queued hello")
    })
    .await;
    let ThreadEvent::TurnStarted { message_id, .. } = started else {
        unreachable!();
    };
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::TurnEnded { .. })
    })
    .await;
    let status: String = sqlx::query_scalar("SELECT status FROM queued_inputs WHERE id = ?")
        .bind(&item_id)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "sent");
    assert!(!message_id.0.is_empty());
}

/// A permission mode decides who answers a request. Supervised waits for a
/// person, auto-accept edits answers file edits itself, and a command still
/// waits even then — the difference between "let it edit" and "let it loose".
#[tokio::test]
async fn permission_modes_decide_who_answers_a_request() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();

    // Supervised (the default): the request waits for a person.
    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let before = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "permission", None)
        .await
        .unwrap();
    let ThreadEvent::PermissionRequested { approval } =
        wait_for_after(&fixture.state.pool, thread_id, before, |event| {
            matches!(event, ThreadEvent::PermissionRequested { .. })
        })
        .await
    else {
        unreachable!("filtered for a permission request");
    };
    assert_eq!(
        pending_approvals(&fixture.state.pool, thread_id).await,
        1,
        "supervised mode must leave the request for a person"
    );
    answer_as_person(&fixture, thread_id, approval.id.as_str()).await;

    // Auto-accept edits: an edit is answered without a person.
    set_permission_mode(&fixture, thread_id, Some("auto_edit")).await;
    let before = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "permission", None)
        .await
        .unwrap();
    let resolved = wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::PermissionResolved { .. })
    })
    .await;
    let ThreadEvent::PermissionResolved {
        decision,
        decided_by,
        ..
    } = resolved
    else {
        unreachable!("filtered for a resolution");
    };
    assert_eq!(decision, ApprovalDecision::AllowOnce);
    assert!(decided_by.is_none(), "a policy decided this, not a person");
    assert_eq!(
        pending_approvals(&fixture.state.pool, thread_id).await,
        0,
        "the request must not stay pending"
    );

    // The same mode still asks about a command.
    let before = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "permission-execute", None)
        .await
        .unwrap();
    let ThreadEvent::PermissionRequested { approval } =
        wait_for_after(&fixture.state.pool, thread_id, before, |event| {
            matches!(event, ThreadEvent::PermissionRequested { .. })
        })
        .await
    else {
        unreachable!("filtered for a permission request");
    };
    assert_eq!(
        approval.kind,
        threadbase_shared::ApprovalKind::Command,
        "the fixture asked to run a command"
    );
    assert_eq!(
        pending_approvals(&fixture.state.pool, thread_id).await,
        1,
        "auto-accept edits must not run commands"
    );
    answer_as_person(&fixture, thread_id, approval.id.as_str()).await;

    // Full access: nothing asks.
    set_permission_mode(&fixture, thread_id, Some("full_access")).await;
    let before = max_seq(&fixture.state.pool, thread_id).await;
    fixture
        .state
        .agents
        .send(thread_id, &fixture.user, "permission-execute", None)
        .await
        .unwrap();
    wait_for_after(&fixture.state.pool, thread_id, before, |event| {
        matches!(event, ThreadEvent::PermissionResolved { .. })
    })
    .await;
    assert_eq!(pending_approvals(&fixture.state.pool, thread_id).await, 0);
}

async fn pending_approvals(pool: &SqlitePool, thread_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'")
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// What the HTTP route does: persist the decision, then hand it to the agent.
async fn answer_as_person(fixture: &Fixture, thread_id: &str, approval_id: &str) {
    threadbase_server::approvals::decide(
        &fixture.state.pool,
        &fixture.state.projection,
        &fixture.state.events,
        approval_id,
        &fixture.user,
        ApprovalDecision::AllowOnce,
        None,
    )
    .await
    .unwrap();
    let _ = fixture
        .state
        .agents
        .decide(thread_id, approval_id, ApprovalDecision::AllowOnce, None)
        .await;
}

async fn set_permission_mode(fixture: &Fixture, thread_id: &str, mode: Option<&str>) {
    let request = UpdateThreadRequest {
        title: None,
        pinned: None,
        archived: None,
        model: None,
        effort: None,
        auto_settle: None,
        permission_mode: mode.map(str::to_owned),
    };
    threads::update(
        &fixture.state.pool,
        &fixture.state.projection,
        thread_id,
        &fixture.user,
        &request,
        &policy(),
    )
    .await
    .unwrap();
}

/// Rewinding to a turn undoes that turn and every later one, in the worktree and
/// in the transcript, and leaves the earlier work alone.
#[tokio::test]
async fn rewind_to_a_turn_restores_files_and_truncates_the_log() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    let worktree = threads::load_row(&fixture.state.pool, thread_id)
        .await
        .unwrap()
        .worktree_path
        .expect("fixture threads run in a worktree");
    let worktree = std::path::PathBuf::from(worktree);

    // Turn 1, then create A; turn 2's checkpoint captures A. Create B, so turn
    // 3's checkpoint captures A and B.
    let mut prompts = Vec::new();
    for _ in 0..3 {
        let before = max_seq(&fixture.state.pool, thread_id).await;
        fixture
            .state
            .agents
            .ensure_started(thread_id)
            .await
            .unwrap();
        fixture
            .state
            .agents
            .send(thread_id, &fixture.user, "stream", None)
            .await
            .unwrap();
        wait_for_after(&fixture.state.pool, thread_id, before, |event| {
            matches!(event, ThreadEvent::TurnEnded { .. })
        })
        .await;
        prompts.push(before);
        if prompts.len() == 1 {
            tokio::fs::write(worktree.join("a.txt"), "from turn 1\n")
                .await
                .unwrap();
        }
        if prompts.len() == 2 {
            tokio::fs::write(worktree.join("b.txt"), "from turn 2\n")
                .await
                .unwrap();
        }
    }
    assert!(worktree.join("a.txt").is_file());
    assert!(worktree.join("b.txt").is_file());

    // The prompts in order, with their turn ids.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT json_extract(payload_json, '$.messageId'),
                json_extract(payload_json, '$.turnId')
           FROM events WHERE thread_id = ? AND kind = 'turn_started' ORDER BY seq ASC",
    )
    .bind(thread_id)
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3, "three turns were started");
    let (second_message, second_turn) = rows[1].clone();

    // Each turn's span is the work done between its checkpoint and the next one,
    // which is exactly what the Changes panel shows for "this turn".
    let turn_ids: Vec<String> = rows.iter().map(|(_, turn)| turn.clone()).collect();
    let row = threads::load_row(&fixture.state.pool, thread_id)
        .await
        .unwrap();
    let first = threadbase_server::diff::turn_changes(&fixture.state.pool, &row, &turn_ids[0])
        .await
        .unwrap();
    assert_eq!(
        first
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.txt"],
        "the file written after turn 1 belongs to turn 1"
    );
    let second = threadbase_server::diff::turn_changes(&fixture.state.pool, &row, &turn_ids[1])
        .await
        .unwrap();
    assert_eq!(
        second
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        vec!["b.txt"]
    );
    let third = threadbase_server::diff::turn_changes(&fixture.state.pool, &row, &turn_ids[2])
        .await
        .unwrap();
    assert!(third.files.is_empty(), "turn 3 wrote nothing");

    let checkpoints_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM checkpoints WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(checkpoints_before, 3, "one checkpoint per turn");

    fixture
        .state
        .agents
        .rewind_to_turn(thread_id, &second_message, &fixture.user)
        .await
        .unwrap();

    // Turn 2's work stays; turn 3's file is gone.
    assert!(
        worktree.join("a.txt").is_file(),
        "turn 2's checkpoint has a.txt"
    );
    assert!(
        !worktree.join("b.txt").exists(),
        "b.txt was created after the checkpoint"
    );
    // The log now ends before turn 2: only one turn_started remains.
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'turn_started'",
    )
    .bind(thread_id)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(remaining, 1, "the rewound turn and its successor are gone");
    // Its own checkpoint row went with it, and the earlier one stayed.
    let checkpoints: Vec<String> =
        sqlx::query_scalar("SELECT turn_id FROM checkpoints WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(checkpoints.len(), 1);
    assert_ne!(checkpoints[0], second_turn);
    // The next prompt must start a new conversation, not resume the discarded one.
    let session: Option<String> =
        sqlx::query_scalar("SELECT session_id FROM agent_bindings WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert!(session.is_none(), "the ACP session was cleared");
}

/// The thread's permission mode reaches the agent itself: at launch, and again
/// when it changes, so a long turn stops asking for what is already allowed.
#[tokio::test]
async fn a_permission_mode_is_applied_to_the_agent() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    let journal = fixture.state.config.data_dir.join("journal.log");
    set_permission_mode(&fixture, thread_id, Some("full_access")).await;

    fixture
        .state
        .agents
        .ensure_started(thread_id)
        .await
        .unwrap();
    let launched = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(
        launched.contains("session/set_mode") && launched.contains("agent-full-access"),
        "the codex profile maps full access to agent-full-access:\n{launched}"
    );

    let changed = fixture
        .state
        .agents
        .set_permission_mode(thread_id, threadbase_shared::PermissionMode::Supervised)
        .await
        .unwrap();
    assert!(changed, "a live session exists");
    let after = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(
        after.contains("\"agent\""),
        "supervised maps to the agent mode:\n{after}"
    );
}

/// An MCP tool call reaches the same code paths a click does, and leaves a line
/// in the transcript so nobody wonders where the turn came from.
#[tokio::test]
async fn an_mcp_token_can_start_a_turn_and_leaves_a_trace() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let thread_id = fixture.thread.id.as_str();
    let principal = threadbase_server::mcp::Principal {
        user: fixture.user.clone(),
        scopes: vec![
            threadbase_shared::McpScope::Read,
            threadbase_shared::McpScope::Write,
        ],
        label: "test client".to_owned(),
    };
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "thread_prompt",
            "arguments": { "thread_id": thread_id, "text": "stream" }
        }
    });
    let reply = threadbase_server::mcp::handle(&fixture.state, &principal, &call)
        .await
        .expect("a call with an id gets a reply");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    assert!(
        reply["result"]["isError"] == false,
        "the prompt was accepted: {reply}"
    );
    assert!(
        !parsed["delivery"].is_null(),
        "delivery is reported: {text}"
    );

    wait_for(&fixture.state.pool, thread_id, |event| {
        matches!(event, ThreadEvent::TurnStarted { .. })
    })
    .await;
    let notice: String = sqlx::query_scalar(
        "SELECT payload_json FROM events WHERE thread_id = ? AND kind = 'system_notice'
          ORDER BY seq DESC LIMIT 1",
    )
    .bind(thread_id)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert!(
        notice.contains("through MCP") && notice.contains("test client"),
        "the transcript names the client: {notice}"
    );
}
