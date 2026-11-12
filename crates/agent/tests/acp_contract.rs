//! Contract tests for the ACP harness against a scripted fake agent.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use threadbase_agent::{
    AcpHarness, AgentProfile, Capabilities, Harness, HarnessError, LaunchCtx, NewSession,
    RawAgentEvent, SteeringOutcome,
};
use threadbase_shared::{
    ApprovalDecision, ElicitationAction, PlanPriority, PlanStatus, SubagentStatus, ToolKind,
    ToolStatus,
};
use tokio::sync::broadcast;

static FAKE_PROFILE: AgentProfile = AgentProfile {
    id: "fake",
    label: "Fake agent",
    command: env!("CARGO_BIN_EXE_threadbase_acp_fixture"),
    args: &[],
    npm_package: None,
    npm_version: None,
    default_model: None,
    modes: &[],
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

struct TestHarness {
    harness: AcpHarness,
    events: broadcast::Receiver<RawAgentEvent>,
    journal: PathBuf,
    dir: tempfile::TempDir,
    capabilities: Capabilities,
}

async fn start(subagents: bool) -> TestHarness {
    let dir = tempfile::tempdir().expect("temp directory");
    let journal = dir.path().join("journal.jsonl");
    let ctx = LaunchCtx {
        thread_id: "thread-1".to_owned(),
        cwd: dir.path().to_path_buf(),
        agent: "fake".to_owned(),
        model: None,
        effort: None,
        subagents,
        env: vec![(
            "FAKE_AGENT_JOURNAL".to_owned(),
            journal.to_string_lossy().into_owned(),
        )],
    };
    let mut harness = AcpHarness::new(FAKE_PROFILE);
    let events = harness.events();
    let capabilities = harness.start(ctx).await.expect("start harness");
    TestHarness {
        harness,
        events,
        journal,
        dir,
        capabilities,
    }
}

async fn new_session(test: &mut TestHarness) -> String {
    test.harness
        .new_session(NewSession {
            cwd: test.dir.path().to_path_buf(),
            mcp_servers: Vec::new(),
        })
        .await
        .expect("new session")
        .0
}

async fn next_event(events: &mut broadcast::Receiver<RawAgentEvent>) -> RawAgentEvent {
    tokio::time::timeout(EVENT_TIMEOUT, events.recv())
        .await
        .expect("timed out waiting for an agent event")
        .expect("agent event channel closed")
}

async fn expect_event(
    events: &mut broadcast::Receiver<RawAgentEvent>,
    mut predicate: impl FnMut(&RawAgentEvent) -> bool,
) -> RawAgentEvent {
    loop {
        let event = next_event(events).await;
        if predicate(&event) {
            return event;
        }
    }
}

fn journal_entries(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn received(entry: &Value) -> &Value {
    entry.get("received").unwrap_or(entry)
}

async fn journal_entry(path: &Path, mut predicate: impl FnMut(&Value) -> bool) -> Value {
    let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
    loop {
        if let Some(entry) = journal_entries(path)
            .into_iter()
            .find(|entry| predicate(received(entry)))
        {
            return entry;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "journal entry not found in {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn expect_prompt_done(events: &mut broadcast::Receiver<RawAgentEvent>) -> String {
    match expect_event(events, |event| {
        matches!(event, RawAgentEvent::PromptDone { .. })
    })
    .await
    {
        RawAgentEvent::PromptDone { stop_reason } => stop_reason,
        _ => unreachable!("filtered for prompt completion"),
    }
}

async fn expect_selected_option(events: &mut broadcast::Receiver<RawAgentEvent>) -> String {
    loop {
        if let RawAgentEvent::ToolCallUpdate { call } = next_event(events).await {
            let Some(output) = call.output.as_deref() else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(output) else {
                continue;
            };
            if let Some(selected) = value.get("selected").and_then(Value::as_str) {
                return selected.to_owned();
            }
        }
    }
}

#[tokio::test]
async fn start_advertises_documented_client_capabilities() {
    let mut test = start(true).await;
    assert!(test.capabilities.steering);
    assert!(test.capabilities.subagents);
    assert!(test.capabilities.elicitation_form);
    assert!(!test.capabilities.elicitation_url);
    assert!(!test.capabilities.terminal);

    let info = test.harness.agent_info().expect("agent identity");
    assert_eq!(info.name, "fake-agent");
    assert_eq!(info.version, "0.0.1");

    let initialize = journal_entry(&test.journal, |message| {
        message.get("method").and_then(Value::as_str) == Some("initialize")
    })
    .await;
    let capabilities = received(&initialize)
        .pointer("/params/clientCapabilities")
        .expect("client capabilities");
    assert_eq!(
        capabilities
            .pointer("/fs/readTextFile")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        capabilities
            .pointer("/fs/writeTextFile")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        capabilities.get("terminal").and_then(Value::as_bool),
        Some(false)
    );
    assert!(capabilities
        .get("subagents")
        .and_then(Value::as_object)
        .is_some());
    assert!(capabilities.pointer("/elicitation/form").is_some());

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn start_without_subagent_opt_in_omits_the_capability() {
    let mut test = start(false).await;
    assert!(!test.capabilities.subagents);

    let initialize = journal_entry(&test.journal, |message| {
        message.get("method").and_then(Value::as_str) == Some("initialize")
    })
    .await;
    let capabilities = received(&initialize)
        .pointer("/params/clientCapabilities")
        .expect("client capabilities");
    assert!(capabilities.get("subagents").is_none());

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn prompt_streams_text_tools_plan_usage_and_completion() {
    let mut test = start(true).await;
    assert_eq!(new_session(&mut test).await, "sess-1");

    let turn = test
        .harness
        .prompt(vec!["stream".to_owned()])
        .await
        .expect("prompt");
    assert_eq!(turn.turn_id, "turn-1");

    let event = expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::AgentMessageChunk { text } if text == "Hello"),
    )
    .await;
    assert!(matches!(event, RawAgentEvent::AgentMessageChunk { .. }));

    expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::AgentThoughtChunk { text } if text == "Considering"),
    )
    .await;

    let call = expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::ToolCall { call } if call.id == "call-1"),
    )
    .await;
    let RawAgentEvent::ToolCall { call } = call else {
        unreachable!("filtered for a tool call")
    };
    assert_eq!(call.title, "Run tests");
    assert_eq!(call.kind, ToolKind::Execute);
    assert_eq!(call.status, ToolStatus::Pending);
    assert_eq!(call.locations.len(), 1);
    assert_eq!(call.locations[0].path, "/tmp/work/src/lib.rs");
    assert_eq!(call.locations[0].line, Some(12));
    assert_eq!(call.input.as_deref(), Some(r#"{"command":"cargo test"}"#));
    let diff = call.diff.as_ref().expect("diff summary");
    assert_eq!(diff.path, "/tmp/work/src/lib.rs");
    assert_eq!(diff.additions, 1);
    assert_eq!(diff.deletions, 0);
    assert!(diff
        .patch
        .as_deref()
        .unwrap_or_default()
        .contains("+    run();"));

    let update = expect_event(&mut test.events, |event| {
        matches!(
            event,
            RawAgentEvent::ToolCallUpdate { call }
                if call.id == "call-1" && call.status == ToolStatus::InProgress
        )
    })
    .await;
    let RawAgentEvent::ToolCallUpdate { call } = update else {
        unreachable!("filtered for a tool call update")
    };
    assert_eq!(call.title, "Run tests");
    assert_eq!(call.kind, ToolKind::Execute);

    let completed = expect_event(&mut test.events, |event| {
        matches!(
            event,
            RawAgentEvent::ToolCallUpdate { call }
                if call.id == "call-1" && call.status == ToolStatus::Completed
        )
    })
    .await;
    let RawAgentEvent::ToolCallUpdate { call } = completed else {
        unreachable!("filtered for a tool call update")
    };
    assert_eq!(call.output.as_deref(), Some(r#"{"stdout":"ok"}"#));

    let plan = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::Plan { .. })
    })
    .await;
    let RawAgentEvent::Plan { entries } = plan else {
        unreachable!("filtered for a plan")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].content, "Write code");
    assert_eq!(entries[0].priority, PlanPriority::High);
    assert_eq!(entries[0].status, PlanStatus::Completed);
    assert_eq!(entries[1].priority, PlanPriority::Medium);
    assert_eq!(entries[1].status, PlanStatus::InProgress);

    let usage = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::Usage { .. })
    })
    .await;
    let RawAgentEvent::Usage { usage } = usage else {
        unreachable!("filtered for usage")
    };
    assert_eq!(usage.context_used, Some(1234));
    assert_eq!(usage.context_size, Some(200_000));

    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");
    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn load_session_resumes_without_replaying_history() {
    let mut test = start(true).await;
    let session = new_session(&mut test).await;
    test.harness
        .load_session(&threadbase_agent::SessionId(session.clone()))
        .await
        .expect("load session");

    let load = journal_entry(&test.journal, |message| {
        message.get("method").and_then(Value::as_str) == Some("session/load")
    })
    .await;
    assert_eq!(
        received(&load)
            .pointer("/params/sessionId")
            .and_then(Value::as_str),
        Some(session.as_str())
    );

    // `session/load` replays the stored conversation as session updates. The
    // fixture writes the "replayed" chunk right after the load response, so a
    // short quiet window gives the driver time to consume it while the
    // harness is still replaying. A leak fails either this window or the
    // first-chunk check below (the replayed chunk precedes live output).
    let deadline = tokio::time::Instant::now() + Duration::from_millis(250);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, next_event(&mut test.events)).await {
            Err(_) => break,
            Ok(RawAgentEvent::AgentMessageChunk { text }) => {
                panic!("load replay leaked a message chunk: {text:?}")
            }
            Ok(RawAgentEvent::AgentThoughtChunk { .. })
            | Ok(RawAgentEvent::ToolCall { .. })
            | Ok(RawAgentEvent::ToolCallUpdate { .. }) => {
                panic!("load replay leaked a conversation event")
            }
            Ok(_) => {}
        }
    }

    // The first prompt re-opens the stream.
    test.harness
        .prompt(vec!["stream".to_owned()])
        .await
        .expect("prompt");
    let first = loop {
        match next_event(&mut test.events).await {
            event @ RawAgentEvent::AgentMessageChunk { .. } => break event,
            RawAgentEvent::PromptDone { .. } => {
                panic!("streaming prompt finished without a message chunk")
            }
            _ => {}
        }
    };
    let RawAgentEvent::AgentMessageChunk { text } = first else {
        unreachable!("filtered for a message chunk");
    };
    assert_eq!(text, "Hello", "replay must not precede live streaming");
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");

    test.harness.shutdown().await.expect("shutdown");
}

/// Regression: a resumed session used to report no configuration options at
/// all, because only the `session/new` response was read. Resumed threads then
/// showed "this agent does not advertise models" in the composer.
#[tokio::test]
async fn load_session_restores_config_options() {
    let mut test = start(true).await;
    let session = new_session(&mut test).await;

    let fresh = test.harness.config_options();
    assert!(
        fresh.iter().any(|option| option.category == "model"),
        "session/new must surface the model select: {fresh:?}"
    );

    test.harness
        .load_session(&threadbase_agent::SessionId(session))
        .await
        .expect("load session");

    let resumed = test.harness.config_options();
    let model = resumed
        .iter()
        .find(|option| option.category == "model")
        .expect("resumed session must still advertise the model select");
    assert_eq!(model.current_value, "fake-model-b");
    assert_eq!(model.options.len(), 2);
    assert!(
        resumed
            .iter()
            .any(|option| option.category == "thought_level"),
        "effort/thinking select must survive the resume too: {resumed:?}"
    );

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn permission_allow_selects_the_matching_option_kind() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["permission".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = event else {
        unreachable!("filtered for a permission request")
    };
    assert_eq!(request.kind, ToolKind::Edit);
    assert_eq!(request.command.as_deref(), Some("echo hi"));
    assert_eq!(request.cwd.as_deref(), Some("/tmp/work"));
    assert_eq!(request.options.len(), 3);
    assert!(request
        .options
        .iter()
        .any(|option| option.kind == "allow_once"));

    test.harness
        .decide(&request.id, ApprovalDecision::AllowOnce)
        .await
        .expect("allow once");
    assert_eq!(
        expect_selected_option(&mut test.events).await,
        "opt-allow-once"
    );
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");
    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn permission_deny_selects_a_reject_option() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["permission-deny".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = event else {
        unreachable!("filtered for a permission request")
    };
    assert!(!request
        .options
        .iter()
        .any(|option| option.kind == "reject_once"));

    test.harness
        .decide(&request.id, ApprovalDecision::Deny)
        .await
        .expect("deny");
    assert_eq!(
        expect_selected_option(&mut test.events).await,
        "opt-reject-always"
    );
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");
    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn permission_deny_prefers_reject_once_when_present() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["permission-reject".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = event else {
        unreachable!("filtered for a permission request")
    };
    assert!(request
        .options
        .iter()
        .any(|option| option.kind == "reject_once"));

    test.harness
        .decide(&request.id, ApprovalDecision::Deny)
        .await
        .expect("deny");
    assert_eq!(
        expect_selected_option(&mut test.events).await,
        "opt-reject-once"
    );
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");
    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn permission_without_matching_option_is_denied_by_default() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["permission-none".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::PermissionRequest { .. })
    })
    .await;
    let RawAgentEvent::PermissionRequest { request } = event else {
        unreachable!("filtered for a permission request")
    };
    assert_eq!(request.options.len(), 1);
    assert_eq!(request.options[0].kind, "allow_always");

    test.harness
        .decide(&request.id, ApprovalDecision::Deny)
        .await
        .expect("deny");
    assert_eq!(expect_selected_option(&mut test.events).await, "cancelled");
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");
    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn cancel_stops_the_running_turn() {
    let mut test = start(true).await;
    let session = new_session(&mut test).await;
    let turn = test
        .harness
        .prompt(vec!["slow".to_owned()])
        .await
        .expect("prompt");

    expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::AgentMessageChunk { text } if text == "working"),
    )
    .await;

    test.harness.cancel(&turn).await.expect("cancel");
    expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::AgentMessageChunk { text } if text == "cancelled"),
    )
    .await;
    assert_eq!(expect_prompt_done(&mut test.events).await, "cancelled");

    let cancel = journal_entry(&test.journal, |message| {
        message.get("method").and_then(Value::as_str) == Some("session/cancel")
    })
    .await;
    assert_eq!(
        received(&cancel)
            .pointer("/params/sessionId")
            .and_then(Value::as_str),
        Some(session.as_str())
    );

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn steering_returns_each_documented_outcome() {
    let mut test = start(true).await;
    let session = new_session(&mut test).await;

    let cases = [
        ("steer:injected", SteeringOutcome::Injected),
        ("steer:startedNewTurn", SteeringOutcome::StartedNewTurn),
        ("steer:promptRequired", SteeringOutcome::PromptRequired),
        (
            "steer:failed",
            SteeringOutcome::Failed {
                reason: Some("boom".to_owned()),
            },
        ),
    ];
    for (input, expected) in cases {
        let outcome = test
            .harness
            .steer(vec![input.to_owned()])
            .await
            .expect("steer");
        assert_eq!(outcome, expected);
    }

    let requests: Vec<Value> = journal_entries(&test.journal)
        .into_iter()
        .filter(|entry| {
            received(entry).get("method").and_then(Value::as_str) == Some("_session/steering")
        })
        .collect();
    assert_eq!(requests.len(), 4);
    for request in requests {
        let params = received(&request)
            .get("params")
            .expect("steering params")
            .clone();
        assert_eq!(
            params.get("sessionId").and_then(Value::as_str),
            Some(session.as_str())
        );
        assert_eq!(
            params
                .pointer("/_meta/steering/idleBehavior")
                .and_then(Value::as_str),
            Some("promptRequired")
        );
    }

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn elicitation_actions_map_to_protocol_responses() {
    let mut test = start(true).await;
    new_session(&mut test).await;

    let cases = [
        (
            ElicitationAction::Accept,
            Some(serde_json::json!({"environment": "staging"})),
            "accept",
        ),
        (ElicitationAction::Decline, None, "decline"),
        (ElicitationAction::Cancel, None, "cancel"),
    ];
    for (action, values, expected) in cases {
        test.harness
            .prompt(vec!["elicit".to_owned()])
            .await
            .expect("prompt");
        let event = expect_event(&mut test.events, |event| {
            matches!(event, RawAgentEvent::ElicitationRequest { .. })
        })
        .await;
        let RawAgentEvent::ElicitationRequest { request } = event else {
            unreachable!("filtered for an elicitation request")
        };
        assert_eq!(request.mode, "form");
        assert!(request.blocking);
        assert_eq!(
            request.requested_schema.get("type").and_then(Value::as_str),
            Some("object")
        );

        test.harness
            .respond_elicitation(&request.id, action, values)
            .await
            .expect("respond to elicitation");
        expect_event(&mut test.events, |event| {
            matches!(event, RawAgentEvent::AgentMessageChunk { text } if *text == format!("elicitation:{expected}"))
        })
        .await;
        assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");

        let response = journal_entry(&test.journal, |message| {
            message.get("method").is_none()
                && message.pointer("/result/action").and_then(Value::as_str) == Some(expected)
        })
        .await;
        if expected == "accept" {
            assert_eq!(
                received(&response)
                    .pointer("/result/content/environment")
                    .and_then(Value::as_str),
                Some("staging")
            );
        }
    }

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn nested_subagent_updates_upsert_and_only_declared_children_cancel() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["subagents".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::Subagent { event } if event.id == "sub-1"),
    )
    .await;
    let RawAgentEvent::Subagent { event } = event else {
        unreachable!("filtered for a subagent event")
    };
    assert_eq!(event.parent_id, None);
    assert_eq!(event.title, "Test investigator");
    assert_eq!(event.description.as_deref(), Some("Checks failures"));
    assert_eq!(event.status, SubagentStatus::Running);
    assert_eq!(event.cwd.as_deref(), Some("/tmp/fake-worktree/sub-1"));
    assert!(event.can_cancel);
    assert_eq!(event.usage, None);

    let event = expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::Subagent { event } if event.id == "sub-3"),
    )
    .await;
    let RawAgentEvent::Subagent { event } = event else {
        unreachable!("filtered for a subagent event")
    };
    assert_eq!(event.parent_id, None);
    assert!(!event.can_cancel);

    let event = expect_event(
        &mut test.events,
        |event| matches!(event, RawAgentEvent::Subagent { event } if event.id == "sub-2"),
    )
    .await;
    let RawAgentEvent::Subagent { event } = event else {
        unreachable!("filtered for a subagent event")
    };
    assert_eq!(event.parent_id.as_deref(), Some("sub-1"));
    assert_eq!(event.status, SubagentStatus::RequiresAction);
    assert!(event.can_cancel);

    let event = expect_event(&mut test.events, |event| {
        matches!(
            event,
            RawAgentEvent::Subagent { event }
                if event.id == "sub-1" && event.status == SubagentStatus::Cancelled
        )
    })
    .await;
    let RawAgentEvent::Subagent { event } = event else {
        unreachable!("filtered for a subagent event")
    };
    assert_eq!(event.usage.expect("subagent usage").total_tokens, 120);
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");

    test.harness
        .cancel_subagent("sub-1")
        .await
        .expect("cancel declared child");
    expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::AgentMessageChunk { text } if text == "subagent-cancelled:sub-1")
    })
    .await;

    let error = test
        .harness
        .cancel_subagent("sub-3")
        .await
        .expect_err("undeclared child must not be cancelled");
    assert!(matches!(error, HarnessError::Protocol(_)));

    let cancels: Vec<Value> = journal_entries(&test.journal)
        .into_iter()
        .filter(|entry| {
            received(entry).get("method").and_then(Value::as_str) == Some("session/cancel")
        })
        .collect();
    assert_eq!(cancels.len(), 1);
    assert_eq!(
        received(&cancels[0])
            .pointer("/params/sessionId")
            .and_then(Value::as_str),
        Some("sub-1")
    );

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn malformed_json_rpc_emits_an_error_and_keeps_streaming() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["malformed".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::Error { message } if message.contains("malformed JSON-RPC"))
    })
    .await;
    assert!(matches!(event, RawAgentEvent::Error { .. }));

    expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::AgentMessageChunk { text } if text == "after malformed")
    })
    .await;
    assert_eq!(expect_prompt_done(&mut test.events).await, "end_turn");

    test.harness.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn process_crash_emits_process_exited_and_closes_the_event_channel() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["crash".to_owned()])
        .await
        .expect("prompt");

    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::ProcessExited { .. })
    })
    .await;
    let RawAgentEvent::ProcessExited { code, signal } = event else {
        unreachable!("filtered for process exit")
    };
    assert_eq!(code, Some(7));
    assert_eq!(signal, None);

    loop {
        match tokio::time::timeout(EVENT_TIMEOUT, test.events.recv()).await {
            Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            Err(_) => panic!("agent event channel did not close after the process exited"),
        }
    }
    assert!(matches!(
        test.harness.events().try_recv(),
        Err(broadcast::error::TryRecvError::Closed)
    ));
}

#[tokio::test]
async fn inter_session_messages_map_to_agent_message_events() {
    let mut test = start(true).await;
    new_session(&mut test).await;
    test.harness
        .prompt(vec!["agent-message".to_owned()])
        .await
        .expect("prompt");
    let event = expect_event(&mut test.events, |event| {
        matches!(event, RawAgentEvent::AgentMessage { .. })
    })
    .await;
    let RawAgentEvent::AgentMessage {
        direction,
        from,
        to,
        text,
    } = event
    else {
        unreachable!("filtered for agent message")
    };
    assert_eq!(direction, "sent");
    assert_eq!(from, "sess-1");
    assert_eq!(to, "sub-1");
    assert_eq!(text, "peer message");
    expect_prompt_done(&mut test.events).await;
    test.harness.shutdown().await.expect("shutdown");
}
