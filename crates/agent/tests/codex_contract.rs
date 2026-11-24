//! Contract tests for the native Codex app-server harness.

use std::time::Duration;

use threadbase_agent::{
    AgentProfile, CodexHarness, ForegroundState, Harness, LaunchCtx, NewSession, PromptPart,
    RawAgentEvent,
};
use threadbase_shared::{ApprovalDecision, PlanStatus, ToolKind};
use tokio::sync::broadcast;

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

fn profile() -> AgentProfile {
    AgentProfile {
        id: "codex".to_owned(),
        label: "Codex".to_owned(),
        command: env!("CARGO_BIN_EXE_threadbase_codex_fixture").to_owned(),
        args: Vec::new(),
        npm_package: None,
        npm_version: None,
        default_model: None,
        modes: &[],
        permission_modes: &[],
    }
}

async fn next(events: &mut broadcast::Receiver<RawAgentEvent>) -> RawAgentEvent {
    tokio::time::timeout(EVENT_TIMEOUT, events.recv())
        .await
        .expect("timed out waiting for a codex event")
        .expect("codex events closed")
}

async fn expect(
    events: &mut broadcast::Receiver<RawAgentEvent>,
    mut predicate: impl FnMut(&RawAgentEvent) -> bool,
) -> RawAgentEvent {
    for _ in 0..200 {
        let event = next(events).await;
        if predicate(&event) {
            return event;
        }
    }
    panic!("expected codex event not seen");
}

#[tokio::test]
async fn the_codex_client_mirrors_a_turn_and_answers_approvals() {
    let dir = tempfile::tempdir().expect("temp directory");
    let mut harness = CodexHarness::new(profile());
    let mut events = harness.events();
    let capabilities = harness
        .start(LaunchCtx {
            thread_id: "thread-1".to_owned(),
            cwd: dir.path().to_path_buf(),
            agent: "codex".to_owned(),
            model: None,
            effort: None,
            subagents: false,
            permission_mode: None,
            env: vec![(
                "THREADBASE_CODEX_BIN".to_owned(),
                env!("CARGO_BIN_EXE_threadbase_codex_fixture").to_owned(),
            )],
        })
        .await
        .expect("start codex harness");
    assert!(capabilities.steering);
    let info = harness.agent_info().expect("agent identity");
    assert_eq!(info.name, "codex");

    let session = harness
        .new_session(NewSession {
            cwd: dir.path().to_path_buf(),
            mcp_servers: Vec::new(),
        })
        .await
        .expect("thread/start");
    assert_eq!(session.0, "thread-1");
    let sessions = harness.list_sessions(None).await.expect("thread/list");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, "thread-0");
    assert_eq!(sessions[1].id, "thread-1");
    assert_eq!(sessions[0].title, "Old thread");
    assert_eq!(sessions[0].cwd, "/tmp/old");
    assert_eq!(sessions[0].updated_at, 123_000);

    let history = harness
        .read_history(&threadbase_agent::SessionId("thread-1".to_owned()))
        .await
        .expect("thread/turns/list");
    assert_eq!(history.turns.len(), 1);
    assert_eq!(history.turns[0].id.as_deref(), Some("turn-old"));
    assert_eq!(history.turns[0].user, "old question");
    assert_eq!(history.turns[0].completed_at, Some(1_700_000_000_000));
    assert!(matches!(
        &history.turns[0].items[0],
        threadbase_agent::SessionItem::Message { text, .. } if text == "old answer"
    ));
    assert!(matches!(
        &history.turns[0].items[1],
        threadbase_agent::SessionItem::Tool(call)
            if call.id == "c1" && call.output.as_deref() == Some("file\n")
    ));

    let options = harness.config_options();
    assert!(options.iter().any(|option| option.category == "model"));
    assert!(options
        .iter()
        .any(|option| option.category == "thought_level"));

    harness
        .prompt(vec![PromptPart::Text("hello".to_owned())])
        .await
        .expect("turn/start");

    expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::MessageChunk { text, .. } if text == "Hello"
        )
    })
    .await;

    let command = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::ToolUpsert { call } if call.id == "cmd-1" && call.output.is_some()
        )
    })
    .await;
    let RawAgentEvent::ToolUpsert { call } = command else {
        unreachable!()
    };
    assert_eq!(call.kind, ToolKind::Execute);
    assert_eq!(call.output.as_deref(), Some("hi\n"));

    let change = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::ToolUpsert { call } if call.id == "file-1" && call.diff.is_some()
        )
    })
    .await;
    let RawAgentEvent::ToolUpsert { call } = change else {
        unreachable!()
    };
    assert_eq!(call.kind, ToolKind::Edit);
    let diff = call.diff.expect("diff");
    assert_eq!((diff.additions, diff.deletions), (1, 1));

    let request = expect(&mut events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = request else {
        unreachable!()
    };
    assert_eq!(request.command.as_deref(), Some("rm -rf /tmp/x"));
    assert_eq!(request.description.as_deref(), Some("test approval"));
    harness
        .decide(&request.id, ApprovalDecision::AllowOnce, None)
        .await
        .expect("answer approval");

    expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::MessageUpsert { text: Some(text), .. } if text == "Hello world"
        )
    })
    .await;
    expect(&mut events, |event| {
        matches!(event, RawAgentEvent::Usage { .. })
    })
    .await;
    let plan = expect(&mut events, |event| {
        matches!(event, RawAgentEvent::Plan { .. })
    })
    .await;
    let RawAgentEvent::Plan { entries } = plan else {
        unreachable!()
    };
    assert_eq!(entries[0].status, PlanStatus::Completed);

    let idle = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::Foreground {
                state: ForegroundState::Idle { .. }
            }
        )
    })
    .await;
    let RawAgentEvent::Foreground {
        state: ForegroundState::Idle { stop_reason, .. },
    } = idle
    else {
        unreachable!()
    };
    assert_eq!(stop_reason.as_deref(), Some("end_turn"));

    harness.shutdown().await.expect("shutdown");
}

/// Manual check: joins the developer's running Codex daemon over WebSocket and
/// only reads the model list, so it never creates a thread or a turn.
#[tokio::test]
#[ignore = "requires a running codex app-server daemon"]
async fn live_daemon_accepts_a_websocket_client() {
    let dir = tempfile::tempdir().expect("temp directory");
    let mut harness = CodexHarness::new(profile());
    harness
        .start(LaunchCtx {
            thread_id: "thread-1".to_owned(),
            cwd: dir.path().to_path_buf(),
            agent: "codex".to_owned(),
            model: None,
            effort: None,
            subagents: false,
            permission_mode: None,
            env: Vec::new(),
        })
        .await
        .expect("join the codex daemon");
    assert_eq!(harness.agent_info().expect("identity").name, "codex");
    assert!(!harness.config_options().is_empty());
    harness.shutdown().await.expect("shutdown");
}

/// Manual smoke against the developer's Codex daemon: creates a thread in a
/// temp directory, asks for a one-word answer, and waits for the idle state.
/// It spends one small model call and leaves a thread in Codex's own history.
#[tokio::test]
#[ignore = "runs a real codex turn against the local daemon"]
async fn live_codex_answers_a_prompt() {
    let dir = tempfile::tempdir().expect("temp directory");
    let mut harness = CodexHarness::new(profile());
    let mut events = harness.events();
    let mut ctx = LaunchCtx {
        thread_id: "thread-1".to_owned(),
        cwd: dir.path().to_path_buf(),
        agent: "codex".to_owned(),
        model: None,
        effort: None,
        subagents: false,
        permission_mode: None,
        env: Vec::new(),
    };
    ctx.env
        .push(("THREADBASE_CODEX_STDIO".to_owned(), "0".to_owned()));
    harness.start(ctx).await.expect("join the codex daemon");
    harness
        .new_session(NewSession {
            cwd: dir.path().to_path_buf(),
            mcp_servers: Vec::new(),
        })
        .await
        .expect("thread/start");
    harness
        .prompt(vec![PromptPart::Text(
            "Reply with exactly OK and do not use any tools.".to_owned(),
        )])
        .await
        .expect("turn/start");
    let idle = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::Foreground {
                state: ForegroundState::Idle { .. }
            }
        )
    })
    .await;
    let RawAgentEvent::Foreground {
        state: ForegroundState::Idle { stop_reason, .. },
    } = idle
    else {
        unreachable!()
    };
    assert_eq!(stop_reason.as_deref(), Some("end_turn"));
    harness.shutdown().await.expect("shutdown");
}
