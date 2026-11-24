//! Contract tests for the native Claude stream-json harness.

use std::time::Duration;

use threadbase_agent::{
    AgentProfile, ClaudeHarness, ForegroundState, Harness, LaunchCtx, MessageRole, NewSession,
    PromptPart, RawAgentEvent,
};
use threadbase_shared::{ApprovalDecision, SubagentStatus, ToolKind, ToolStatus};
use tokio::sync::broadcast;

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

fn profile() -> AgentProfile {
    AgentProfile {
        id: "claude".to_owned(),
        label: "Claude Code".to_owned(),
        command: env!("CARGO_BIN_EXE_threadbase_claude_fixture").to_owned(),
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
        .expect("timed out waiting for a claude event")
        .expect("claude events closed")
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
    panic!("expected claude event not seen");
}

#[tokio::test]
async fn the_claude_client_streams_a_turn_and_answers_permissions() {
    let dir = tempfile::tempdir().expect("temp directory");
    let mut harness = ClaudeHarness::new(profile());
    let mut events = harness.events();
    let capabilities = harness
        .start(LaunchCtx {
            thread_id: "thread-1".to_owned(),
            cwd: dir.path().to_path_buf(),
            agent: "claude".to_owned(),
            model: None,
            effort: None,
            subagents: false,
            permission_mode: None,
            env: vec![(
                "THREADBASE_CLAUDE_BIN".to_owned(),
                env!("CARGO_BIN_EXE_threadbase_claude_fixture").to_owned(),
            )],
        })
        .await
        .expect("start claude harness");
    assert!(capabilities.steering);
    assert!(capabilities.images);
    assert_eq!(harness.agent_info().expect("identity").name, "claude");

    let session = harness
        .new_session(NewSession {
            cwd: dir.path().to_path_buf(),
            mcp_servers: Vec::new(),
        })
        .await
        .expect("new session");
    assert!(!session.0.is_empty());
    harness
        .prompt(vec![PromptPart::Text("hello".to_owned())])
        .await
        .expect("prompt");
    // The process starts on the first prompt; its model catalog arrives with
    // initialize/list_models.
    let model = harness
        .config_options()
        .into_iter()
        .find(|option| option.category == "model")
        .expect("model option");
    assert!(model.options.len() >= 2, "list_models populates the picker");

    expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::MessageChunk {
                role: MessageRole::Agent,
                text,
                ..
            } if text == "Hello"
        )
    })
    .await;

    let tool = expect(
        &mut events,
        |event| matches!(event, RawAgentEvent::ToolUpsert { call } if call.id == "t1"),
    )
    .await;
    let RawAgentEvent::ToolUpsert { call } = tool else {
        unreachable!()
    };
    assert_eq!(call.kind, ToolKind::Execute);
    assert_eq!(call.status, ToolStatus::InProgress);

    let subagent = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::Subagent { event } if event.id == "task-1"
        )
    })
    .await;
    let RawAgentEvent::Subagent { event } = subagent else {
        unreachable!()
    };
    assert_eq!(event.title, "Investigate");
    assert_eq!(event.description.as_deref(), Some("Look around"));
    assert_eq!(event.status, SubagentStatus::Running);

    let request = expect(&mut events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = request else {
        unreachable!()
    };
    assert_eq!(request.command.as_deref(), Some("echo hi"));
    assert_eq!(request.description.as_deref(), Some("Run the command"));
    harness
        .decide(&request.id, ApprovalDecision::AllowOnce, None)
        .await
        .expect("answer permission");

    let completed = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::ToolUpsert { call }
                if call.id == "t1" && call.status == ToolStatus::Completed
        )
    })
    .await;
    let RawAgentEvent::ToolUpsert { call } = completed else {
        unreachable!()
    };
    assert_eq!(call.output.as_deref(), Some("hi\n"));

    let done = expect(&mut events, |event| {
        matches!(
            event,
            RawAgentEvent::Subagent { event }
                if event.id == "task-1" && event.status == SubagentStatus::Idle
        )
    })
    .await;
    let RawAgentEvent::Subagent { event } = done else {
        unreachable!()
    };
    assert_eq!(event.usage.expect("usage").total_tokens, 10);

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
