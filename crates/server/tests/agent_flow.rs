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
            ApprovalDecision::AllowOnce,
        )
        .await
        .unwrap();
    threadbase_server::approvals::decide(
        &fixture.state.pool,
        &fixture.state.events,
        approval.id.as_str(),
        &fixture.user,
        ApprovalDecision::AllowOnce,
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
