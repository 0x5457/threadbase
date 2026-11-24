//! Attach a threadbase thread to an existing native agent session.
//!
//! Discovery and history come from the harness: Codex reads its own app-server
//! thread store. The import is one-way and runs once; later turns resume the
//! same native session through the binding.

use std::path::Path;

use threadbase_agent::profile::Transport;
use threadbase_agent::{
    CodexHarness, ExternalSession, Harness, LaunchCtx, MessageRole as AgentMessageRole,
    SessionHistory, SessionId, SessionItem, SessionTurn, SubagentEvent,
};
use threadbase_shared::{now_ms, Actor, ApiError, MessageId, MessageRole, ThreadEvent, ThreadId};

use super::mapping::{message_dto, subagent_dto, tool_data};
use super::{catalog, AgentManager};
use crate::bindings;
use crate::error::ServerError;
use crate::events::EventMeta;

/// Sessions the agent still knows for this directory. Only transports with a
/// session store answer; the rest return an empty list.
pub(crate) async fn list(
    manager: &AgentManager,
    agent: &str,
    cwd: &Path,
) -> Result<Vec<ExternalSession>, ServerError> {
    let profile = resolve(manager, agent).await?;
    if profile.transport() != Transport::Codex {
        // Claude keeps its conversations as JSONL, so discovery needs no
        // process. Other ACP adapters expose no session store.
        return Ok(if agent == "claude" {
            threadbase_agent::claude_sessions::list(cwd)
        } else {
            Vec::new()
        });
    }
    let mut harness = open(manager, profile, agent, cwd).await?;
    let sessions = harness
        .list_sessions(Some(cwd))
        .await
        .map_err(|error| ServerError::internal(format!("could not list agent sessions: {error}")));
    let _ = harness.shutdown().await;
    sessions
}

/// Copy the native conversation into the thread log and bind the thread to the
/// native session, so the first prompt resumes it instead of starting fresh.
pub(crate) async fn import(
    manager: &AgentManager,
    thread_id: &str,
    session: &SessionId,
) -> Result<(), ServerError> {
    let binding = bindings::load(&manager.inner.pool, thread_id)
        .await?
        .ok_or_else(|| ServerError::internal("agent binding is missing"))?;
    let cwd = Path::new(&binding.cwd);
    let profile = resolve(manager, &binding.agent).await?;
    let history = match (&binding.agent[..], profile.transport()) {
        ("claude", _) => {
            threadbase_agent::claude_sessions::read(cwd, &session.0).map_err(|error| {
                ServerError::internal(format!("could not read the session: {error}"))
            })?
        }
        (_, Transport::Codex) => {
            let mut harness = open(manager, profile, &binding.agent, cwd).await?;
            let history = harness.read_history(session).await.map_err(|error| {
                ServerError::internal(format!("could not read the agent session: {error}"))
            });
            let _ = harness.shutdown().await;
            history?
        }
        _ => {
            return Err(ServerError(ApiError::validation(format!(
                "{} cannot resume native sessions",
                binding.agent
            ))))
        }
    };

    let synced = history
        .turns
        .iter()
        .filter_map(|turn| turn.completed_at)
        .max()
        .unwrap_or_else(threadbase_shared::now_ms);
    let last_turn = history.turns.iter().rev().find_map(|turn| turn.id.clone());
    for event in history_events(thread_id, history) {
        manager
            .inner
            .events
            .append(
                &ThreadId::from(thread_id.to_owned()),
                Actor::system(),
                event,
                EventMeta::default(),
            )
            .await?;
    }
    bindings::set_session(&manager.inner.pool, thread_id, &session.0).await?;
    bindings::mark_native(&manager.inner.pool, thread_id, synced, last_turn.as_deref()).await
}

/// Import native turns completed after the thread's watermark. Runs on resume,
/// so work done by another client while threadbase was stopped lands in the
/// transcript instead of being lost.
pub(crate) async fn backfill(
    manager: &AgentManager,
    thread_id: &str,
    harness: &mut dyn Harness,
    session: &SessionId,
) -> Result<(), ServerError> {
    if !is_native_thread(&manager.inner.pool, thread_id).await {
        return Ok(());
    }
    let synced = bindings::native_synced_at(&manager.inner.pool, thread_id).await;
    let cursor = bindings::native_turn_id(&manager.inner.pool, thread_id).await;
    let history = harness.read_history(session).await.map_err(|error| {
        ServerError::internal(format!("could not read the agent session: {error}"))
    })?;
    let (turns, latest, next) = turns_after(history, synced, cursor.as_deref());
    if turns.is_empty() {
        return Ok(());
    }
    for event in turns
        .into_iter()
        .flat_map(|turn| history_turn_events(thread_id, turn))
    {
        manager
            .inner
            .events
            .append(
                &ThreadId::from(thread_id.to_owned()),
                Actor::system(),
                event,
                EventMeta::default(),
            )
            .await?;
    }
    bindings::set_native_progress(&manager.inner.pool, thread_id, latest, next.as_deref()).await
}

/// A thread whose conversation lives in the agent's own session store. Its
/// history cannot be truncated or copied locally without diverging from the
/// agent.
pub(crate) async fn is_native_thread(pool: &sqlx::SqlitePool, thread_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT native FROM agent_bindings WHERE thread_id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|native| native != 0)
}

async fn resolve(
    manager: &AgentManager,
    agent: &str,
) -> Result<threadbase_agent::AgentProfile, ServerError> {
    catalog::resolve_profile(&manager.inner.pool, agent)
        .await?
        .ok_or_else(|| ServerError(ApiError::validation(format!("unknown agent: {agent}"))))
}

async fn open(
    manager: &AgentManager,
    profile: threadbase_agent::AgentProfile,
    agent: &str,
    cwd: &Path,
) -> Result<CodexHarness, ServerError> {
    let mut harness = CodexHarness::new(profile);
    let ctx = LaunchCtx {
        thread_id: format!("native-{}", ulid::Ulid::new()),
        cwd: cwd.to_path_buf(),
        agent: agent.to_owned(),
        model: None,
        effort: None,
        subagents: false,
        permission_mode: None,
        env: catalog::launch_env(&manager.inner.pool, agent).await?,
    };
    harness.start(ctx).await.map_err(|error| {
        ServerError::internal(format!("could not open the agent store: {error}"))
    })?;
    Ok(harness)
}

/// A Claude Task/Agent tool use in history shape. Codex has no such tool, so
/// the title check keeps ordinary tools out.
fn subagent_from_tool(call: &threadbase_agent::ToolCallEvent) -> Option<SubagentEvent> {
    if !(call.title.starts_with("Task ") || call.title.starts_with("Agent ")) {
        return None;
    }
    let input: serde_json::Value = serde_json::from_str(call.input.as_deref()?).ok()?;
    let title = input.get("description")?.as_str()?;
    let status = match call.status {
        threadbase_shared::ToolStatus::Completed => threadbase_shared::SubagentStatus::Idle,
        threadbase_shared::ToolStatus::Failed => threadbase_shared::SubagentStatus::Error,
        _ => threadbase_shared::SubagentStatus::Running,
    };
    Some(SubagentEvent {
        id: call.id.clone(),
        parent_id: None,
        title: title.to_owned(),
        description: input
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .map(|prompt| prompt.chars().take(240).collect()),
        status,
        cwd: None,
        can_cancel: false,
        usage: None,
    })
}

/// Turns completed after `synced` (Unix ms) and the newest completion time
/// among them. Turns without a completion time are still running or come from
/// a source that does not report one, so they are skipped.
fn turns_after(
    history: SessionHistory,
    synced: i64,
    cursor: Option<&str>,
) -> (Vec<SessionTurn>, i64, Option<String>) {
    let start = cursor.and_then(|cursor| {
        history
            .turns
            .iter()
            .position(|turn| turn.id.as_deref() == Some(cursor))
            .map(|index| index + 1)
    });
    // A turn id is authoritative; without one fall back to completion time.
    let by_cursor = start.is_some();
    let mut latest = synced;
    let mut next = cursor.map(str::to_owned);
    let mut turns = Vec::new();
    for turn in history.turns.into_iter().skip(start.unwrap_or(0)) {
        let Some(at) = turn.completed_at else {
            continue;
        };
        if !by_cursor && at <= synced {
            continue;
        }
        latest = latest.max(at);
        if let Some(id) = &turn.id {
            next = Some(id.clone());
        }
        turns.push(turn);
    }
    (turns, latest, next)
}

/// One `ThreadEvent` per imported row. User and agent messages become
/// completed messages; tools keep their structured diff. Turns are not
/// replayed: their boundaries belong to the native timeline, not the log.
pub(crate) fn history_events(thread_id: &str, history: SessionHistory) -> Vec<ThreadEvent> {
    history
        .turns
        .into_iter()
        .flat_map(|turn| history_turn_events(thread_id, turn))
        .collect()
}

fn history_turn_events(thread_id: &str, turn: SessionTurn) -> Vec<ThreadEvent> {
    let at = turn.completed_at.unwrap_or_else(now_ms);
    let mut events = Vec::new();
    if !turn.user.trim().is_empty() {
        let mut message = message_dto(thread_id, MessageId::new(), None, turn.user);
        message.role = MessageRole::User;
        message.created_at = at;
        events.push(ThreadEvent::MessageCompleted { message });
    }
    for item in turn.items {
        match item {
            SessionItem::Message {
                role: AgentMessageRole::Agent,
                text,
            } => {
                let mut message = message_dto(thread_id, MessageId::new(), None, text);
                message.created_at = at;
                events.push(ThreadEvent::MessageCompleted { message });
            }
            SessionItem::Message { .. } => {}
            SessionItem::Tool(call) => {
                if let Some(subagent) = subagent_from_tool(&call) {
                    let status = subagent.status;
                    events.push(ThreadEvent::SubagentUpdated {
                        subagent: subagent_dto(&subagent, thread_id, status),
                    });
                }
                let mut call = tool_data(thread_id, *call);
                call.created_at = at;
                call.updated_at = at;
                events.push(ThreadEvent::ToolCall { call });
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_agent::{SessionTurn, ToolCallEvent};
    use threadbase_shared::{ToolKind, ToolStatus};

    #[test]
    fn a_task_tool_in_history_becomes_a_subagent() {
        let call = ToolCallEvent {
            id: "task-1".to_owned(),
            title: "Task Investigate".to_owned(),
            kind: ToolKind::Other,
            status: ToolStatus::Completed,
            input: Some(
                serde_json::json!({
                    "description": "Investigate",
                    "prompt": "Look around"
                })
                .to_string(),
            ),
            output: Some("done".to_owned()),
            locations: Vec::new(),
            terminal_id: None,
            diff: None,
            raw: serde_json::Value::Null,
        };
        let history = SessionHistory {
            turns: vec![SessionTurn {
                items: vec![SessionItem::Tool(Box::new(call))],
                ..Default::default()
            }],
        };
        let events = history_events("thread-1", history);
        let subagent = events
            .iter()
            .find_map(|event| match event {
                ThreadEvent::SubagentUpdated { subagent } => Some(subagent),
                _ => None,
            })
            .expect("subagent event");
        assert_eq!(subagent.title, "Investigate");
        assert_eq!(subagent.description.as_deref(), Some("Look around"));
        assert_eq!(subagent.status, threadbase_shared::SubagentStatus::Idle);
    }

    #[test]
    fn only_turns_after_the_watermark_are_imported() {
        let history = SessionHistory {
            turns: vec![
                SessionTurn {
                    completed_at: Some(100),
                    ..Default::default()
                },
                SessionTurn {
                    completed_at: Some(300),
                    ..Default::default()
                },
                SessionTurn::default(),
            ],
        };
        let (turns, latest, next) = turns_after(history, 200, None);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].completed_at, Some(300));
        assert_eq!(latest, 300);
        assert_eq!(next, None);
    }

    #[test]
    fn a_turn_id_cursor_wins_over_second_precision_time() {
        let history = SessionHistory {
            turns: vec![
                SessionTurn {
                    id: Some("a".to_owned()),
                    completed_at: Some(100),
                    ..Default::default()
                },
                SessionTurn {
                    id: Some("b".to_owned()),
                    completed_at: Some(100),
                    ..Default::default()
                },
            ],
        };
        let (turns, latest, next) = turns_after(history, 100, Some("a"));
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id.as_deref(), Some("b"));
        assert_eq!(latest, 100);
        assert_eq!(next.as_deref(), Some("b"));
    }

    #[test]
    fn history_becomes_messages_and_tools() {
        let call = ToolCallEvent {
            id: "c1".to_owned(),
            title: "ls".to_owned(),
            kind: ToolKind::Execute,
            status: ToolStatus::Completed,
            input: None,
            output: Some("file\n".to_owned()),
            locations: Vec::new(),
            terminal_id: None,
            diff: None,
            raw: serde_json::Value::Null,
        };
        let history = SessionHistory {
            turns: vec![SessionTurn {
                user: "old question".to_owned(),
                items: vec![
                    SessionItem::Message {
                        role: AgentMessageRole::Agent,
                        text: "old answer".to_owned(),
                    },
                    SessionItem::Tool(Box::new(call)),
                ],
                ..Default::default()
            }],
        };
        let events = history_events("thread-1", history);
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0],
            ThreadEvent::MessageCompleted { message }
                if message.role == MessageRole::User && message.text == "old question"
        ));
        assert!(matches!(
            &events[1],
            ThreadEvent::MessageCompleted { message }
                if message.role == MessageRole::Agent && message.text == "old answer"
        ));
        assert!(matches!(
            &events[2],
            ThreadEvent::ToolCall { call } if call.id.as_str() == "c1"
        ));
    }
}
