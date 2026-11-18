//! Harness event pump: one task per running session that normalizes agent
//! events into persisted thread events.

use super::mapping::{
    self, approval_kind, approval_option, elicitation_dto, message_dto, subagent_dto, tool_data,
    tool_kind,
};
use super::{AgentManager, Inner, RunState, Session};
use crate::error::ServerError;
use crate::events::EventMeta;
use crate::{approvals, elicitations, subagents, threads};
use std::sync::{Arc, Weak};
use std::time::Duration;
use threadbase_agent::RawAgentEvent;
use threadbase_core::state::ThreadAction;
use threadbase_shared::{
    now_ms, Actor, ApprovalDecision, MessageId, MessageRole, ThreadEvent, ThreadId, ThreadStatus,
};
use tokio::sync::{broadcast, Mutex};

pub(crate) fn spawn(
    manager: AgentManager,
    thread_id: String,
    agent: String,
    run: Arc<Mutex<RunState>>,
    mut events: broadcast::Receiver<RawAgentEvent>,
    session: Weak<Session>,
    pump_done: Arc<tokio::sync::Notify>,
) {
    let pump = Pump {
        inner: manager.inner.clone(),
        manager,
        thread_id,
        agent,
        run,
        session,
    };
    tokio::spawn(async move {
        // Thinking chunks arrive in bursts (10-20+ per second) and are only
        // ever rendered as a collapsed "Thought" row and its tooltip, so a row
        // per chunk buys nothing and turns one turn into thousands of SQLite
        // transactions. Hold them for a moment and append the folded text as
        // one event. Every other event flushes first, so transcript order is
        // preserved.
        const THOUGHT_FLUSH: Duration = Duration::from_millis(500);
        let mut thought: Option<(MessageId, String)> = None;
        // A fixed deadline, not a fresh timeout per chunk: a continuously
        // streaming thought would otherwise postpone the flush forever and the
        // "Thought" row would not appear until the agent moved on.
        let mut deadline: Option<tokio::time::Instant> = None;
        loop {
            let received = match deadline {
                Some(at) => match tokio::time::timeout_at(at, events.recv()).await {
                    Ok(received) => received,
                    Err(_) => {
                        flush_thought(&pump, &mut thought).await;
                        deadline = None;
                        continue;
                    }
                },
                None => events.recv().await,
            };
            // Any adapter output means the session is alive, including turns an
            // extension starts on its own (a goal continuation has no client
            // prompt). The idle reaper must not read that silence as idle.
            if let Some(session) = pump.session.upgrade() {
                session.touch();
            }
            match received {
                Ok(RawAgentEvent::AgentThoughtChunk { text }) => {
                    let message_id = pump.thinking_id().await;
                    match &mut thought {
                        Some((id, buffer)) if *id == message_id => push_delta(buffer, &text),
                        Some(_) => {
                            flush_thought(&pump, &mut thought).await;
                            thought = Some((message_id, text));
                            deadline = Some(tokio::time::Instant::now() + THOUGHT_FLUSH);
                        }
                        None => {
                            thought = Some((message_id, text));
                            deadline = Some(tokio::time::Instant::now() + THOUGHT_FLUSH);
                        }
                    }
                }
                Ok(event) => {
                    flush_thought(&pump, &mut thought).await;
                    deadline = None;
                    if let Err(error) = pump.handle(event).await {
                        tracing::warn!(%error, thread_id = %pump.thread_id, "agent pump event failed");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    flush_thought(&pump, &mut thought).await;
                    deadline = None;
                    tracing::warn!(%skipped, thread_id = %pump.thread_id, "agent pump lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    flush_thought(&pump, &mut thought).await;
                    break;
                }
            }
        }
        // `notify_one` stores a permit when the wait has not started yet, so a
        // rewind that arrives after shutdown still observes a completed pump.
        pump_done.notify_one();
    });
}

fn truncate_summary(message: &str) -> String {
    const MAX: usize = 240;
    let mut text: String = message.chars().take(MAX).collect();
    if message.chars().count() > MAX {
        text.push('…');
    }
    format!("The agent stopped with an error: {text}")
}

/// Events that put a row in the transcript. `AgentThoughtChunk` is folded and
/// flushed separately, so it is counted there instead.
fn is_visible(event: &RawAgentEvent) -> bool {
    matches!(
        event,
        RawAgentEvent::AgentMessageChunk { .. }
            | RawAgentEvent::ToolCall { .. }
            | RawAgentEvent::ToolCallUpdate { .. }
            | RawAgentEvent::Plan { .. }
            | RawAgentEvent::PermissionRequest { .. }
            | RawAgentEvent::ElicitationRequest { .. }
            | RawAgentEvent::Subagent { .. }
            | RawAgentEvent::AgentMessage { .. }
    )
}

/// Stop reasons that mean the turn did not end normally, even with output.
fn is_stop_error(stop_reason: &str) -> bool {
    matches!(stop_reason, "error" | "refusal")
}

fn stop_failure_message(stop_reason: &str, silent: bool) -> String {
    if silent {
        format!(
            "The agent ended the turn without any output. The model request most likely failed upstream — check the thread's model, the provider quota and the agent's login. (stop reason: {stop_reason})"
        )
    } else {
        format!("The agent stopped before finishing the turn. (stop reason: {stop_reason})")
    }
}

/// A turn still counts as finished for these, but the transcript has to say what
/// was cut short.
fn stop_reason_notice(stop_reason: &str) -> Option<&'static str> {
    match stop_reason {
        "max_tokens" => Some("The reply was cut off: the model hit its output token limit."),
        "max_turn_requests" => {
            Some("The turn stopped at the agent's request limit; send another message to continue.")
        }
        _ => None,
    }
}

/// Append a text delta to a buffer that may receive either incremental chunks
/// or cumulative snapshots, mirroring the rule used by the web reducer and the
/// cold-page fold.
fn push_delta(buffer: &mut String, chunk: &str) {
    if chunk.starts_with(buffer.as_str()) {
        buffer.clear();
        buffer.push_str(chunk);
    } else {
        buffer.push_str(chunk);
    }
}

async fn flush_thought(pump: &Pump, thought: &mut Option<(MessageId, String)>) {
    let Some((message_id, text)) = thought.take() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    pump.run.lock().await.saw_output = true;
    if let Err(error) = pump
        .append(ThreadEvent::ThinkingDelta { message_id, text })
        .await
    {
        tracing::warn!(%error, thread_id = %pump.thread_id, "agent pump thought flush failed");
    }
}

struct Pump {
    inner: Arc<Inner>,
    manager: AgentManager,
    thread_id: String,
    agent: String,
    run: Arc<Mutex<RunState>>,
    session: Weak<Session>,
}

impl Pump {
    /// One thinking message id per turn: the pump folds every thought chunk of
    /// a turn into the same row, so buffered chunks stay ordered even when a
    /// tool call interrupts the stream.
    async fn thinking_id(&self) -> MessageId {
        self.run
            .lock()
            .await
            .thinking_id
            .get_or_insert_with(MessageId::new)
            .clone()
    }

    async fn handle(&self, event: RawAgentEvent) -> Result<(), ServerError> {
        if is_visible(&event) {
            self.run.lock().await.saw_output = true;
        }
        match event {
            RawAgentEvent::AgentMessageChunk { text } => {
                let message_id = {
                    let mut run = self.run.lock().await;
                    let id = run
                        .agent_message_id
                        .get_or_insert_with(MessageId::new)
                        .clone();
                    run.agent_text.push_str(&text);
                    id
                };
                self.append(ThreadEvent::MessageDelta {
                    message_id,
                    role: MessageRole::Agent,
                    text,
                    author: None,
                })
                .await
            }
            RawAgentEvent::AgentThoughtChunk { text } => {
                let message_id = self.thinking_id().await;
                self.append(ThreadEvent::ThinkingDelta { message_id, text })
                    .await
            }
            RawAgentEvent::ToolCall { call } => {
                let call = tool_data(&self.thread_id, call);
                self.tool_subagent(&call).await?;
                self.append(ThreadEvent::ToolCall { call }).await
            }
            RawAgentEvent::ToolCallUpdate { call } => {
                let call = tool_data(&self.thread_id, call);
                self.tool_subagent(&call).await?;
                self.append(ThreadEvent::ToolCallUpdate { call }).await
            }
            RawAgentEvent::Plan { entries } => {
                self.append(ThreadEvent::PlanUpdated { entries }).await
            }
            RawAgentEvent::PermissionRequest { request } => self.permission(request).await,
            RawAgentEvent::ElicitationRequest { request } => {
                let elicitation = elicitation_dto(request, &self.thread_id, &self.agent);
                let elicitation_id = elicitation.id.clone();
                match elicitations::create(
                    &self.inner.pool,
                    &self.inner.projection,
                    &self.inner.events,
                    elicitation,
                    Actor::agent("Threadbase agent"),
                    EventMeta::default(),
                )
                .await
                {
                    Ok(_) => self.transition(ThreadAction::ElicitationPending).await,
                    Err(error) => {
                        // Same rule as a permission request: never leave the
                        // agent waiting on a question the transcript cannot show.
                        tracing::warn!(%error, thread_id = %self.thread_id, "could not record an elicitation");
                        let _ = self
                            .manager
                            .respond_elicitation(
                                &self.thread_id,
                                elicitation_id.as_str(),
                                threadbase_shared::ElicitationAction::Cancel,
                                None,
                            )
                            .await;
                        self.append(ThreadEvent::SystemNotice {
                            level: "warning".to_owned(),
                            message: format!(
                                "A question from the agent could not be recorded ({error}); it was cancelled so the turn could continue."
                            ),
                        })
                        .await
                    }
                }
            }
            RawAgentEvent::Subagent { event } => self.subagent(event).await,
            RawAgentEvent::AgentMessage {
                direction,
                from,
                to,
                text,
            } => {
                let direction = match direction.as_str() {
                    "sent" => threadbase_shared::AgentMessageDirection::Sent,
                    _ => threadbase_shared::AgentMessageDirection::Received,
                };
                self.append(ThreadEvent::AgentMessage {
                    direction,
                    from,
                    to,
                    text,
                })
                .await
            }
            RawAgentEvent::Usage { usage } => {
                self.append(ThreadEvent::UsageUpdated { usage }).await
            }
            RawAgentEvent::PromptDone { stop_reason } => self.finish_turn(stop_reason).await,
            RawAgentEvent::Error { message } => {
                self.abort_turn(ThreadAction::TurnError, message).await
            }
            RawAgentEvent::ProcessExited { code, signal } => {
                // An adapter that was told to stop is not a crash. When the
                // session is already gone, a restart is in progress too.
                let Some(session) = self.session.upgrade() else {
                    return Ok(());
                };
                if session.is_stopped() {
                    return Ok(());
                }
                session.mark_stopped();
                // A dying adapter reports a prompt error first, and that path
                // clears `current_turn_id`. Ask the run state too, otherwise
                // whether an interrupted turn is continued depends on which
                // event the ACP reader happens to deliver first.
                let had_turn = self.inner.current_turn(&self.thread_id).await.is_some()
                    || self.run.lock().await.turn_id.is_some();
                self.inner.clear_turn(&self.thread_id).await;
                self.run.lock().await.turn_id = None;
                sqlx::query(
                    "UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?",
                )
                .bind(now_ms())
                .bind(&self.thread_id)
                .execute(&self.inner.pool)
                .await?;
                self.append(ThreadEvent::ProcessExited {
                    code,
                    signal,
                    message: "agent process exited".into(),
                })
                .await?;
                self.notify("error", "The agent process exited unexpectedly.", false)
                    .await;
                let archived = threads::status(&self.inner.pool, &self.thread_id)
                    .await
                    .ok()
                    == Some(ThreadStatus::Archived);
                // The adapter is dead: drop it so a replacement can start,
                // even for an archived thread that is not revived.
                self.manager
                    .inner
                    .remove_session(&self.thread_id, &session)
                    .await;
                if archived {
                    return Ok(());
                }
                // `preparing` rejects this transition; a crash during startup
                // must still leave the thread revivable.
                if let Err(error) = self.transition(ThreadAction::ProcessExited).await {
                    tracing::warn!(
                        thread_id = %self.thread_id,
                        %error,
                        "could not mark the exited agent"
                    );
                }
                self.manager
                    .recover_from_exit(&self.thread_id, had_turn)
                    .await;
                Ok(())
            }
            RawAgentEvent::Notice { level, message } => {
                self.append(ThreadEvent::SystemNotice { level, message })
                    .await
            }
            RawAgentEvent::AgentLog { level, text } => {
                self.append(ThreadEvent::AgentLog { level, text }).await
            }
        }
    }

    async fn permission(
        &self,
        request: threadbase_agent::PermissionRequest,
    ) -> Result<(), ServerError> {
        let options = request
            .options
            .iter()
            .map(|option| approval_option(&option.id, &option.kind, &option.label))
            .collect();
        let request_id = request.id.clone();
        let approval = match approvals::create(
            &self.inner.pool,
            &self.inner.projection,
            &self.inner.events,
            approvals::NewApproval {
                id: request_id.clone(),
                thread_id: self.thread_id.clone(),
                subagent_id: None,
                turn_id: self.run.lock().await.turn_id.clone().map(|id| id.0),
                tool_call_id: None,
                agent: self.agent.clone(),
                title: request.title,
                kind: tool_kind(request.kind).to_owned(),
                command: request.command.clone(),
                cwd: request.cwd.clone(),
                paths: request.paths,
                options,
                input: Some(serde_json::to_string(&request.raw).unwrap_or_default()),
            },
            Actor::agent("Threadbase agent"),
            EventMeta::default(),
        )
        .await
        {
            Ok(approval) => approval,
            Err(error) => {
                // Nothing in the transcript can ask the user, and an agent that
                // is never answered waits forever: deny the tool so the turn
                // continues, and say why. A stalled thread used to be the only
                // symptom of an approval that failed to record.
                tracing::warn!(%error, thread_id = %self.thread_id, "could not record a permission request");
                let _ = self
                    .manager
                    .decide(
                        &self.thread_id,
                        request_id.as_str(),
                        ApprovalDecision::Deny,
                        None,
                    )
                    .await;
                self.append(ThreadEvent::SystemNotice {
                    level: "warning".to_owned(),
                    message: format!(
                        "A tool permission request could not be recorded ({error}); the tool was denied so the turn could continue."
                    ),
                })
                .await?;
                return Ok(());
            }
        };
        self.transition(ThreadAction::ApprovalPending).await?;
        self.try_auto_rule(&approval).await
    }

    /// pi reports its delegations as `subagent` / `subagent_launch` tool calls
    /// instead of ACP subagent events; mirror them into the subagent table.
    async fn tool_subagent(
        &self,
        call: &threadbase_shared::ToolCallData,
    ) -> Result<(), ServerError> {
        let Some(node) = super::derived::derive(call) else {
            return Ok(());
        };
        let Some(dto) = super::derived::apply(
            &self.inner.pool,
            &self.inner.projection,
            &self.thread_id,
            &node,
        )
        .await?
        else {
            return Ok(());
        };
        self.append(ThreadEvent::SubagentUpdated { subagent: dto })
            .await
    }

    async fn subagent(&self, event: threadbase_agent::SubagentEvent) -> Result<(), ServerError> {
        let usage_json = event
            .usage
            .as_ref()
            .map(|usage| serde_json::to_string(usage).unwrap_or_default());
        let status = match subagents::upsert(
            &self.inner.pool,
            &self.inner.projection,
            subagents::Upsert {
                thread_id: &self.thread_id,
                id: &event.id,
                parent_id: event.parent_id.as_deref(),
                title: &event.title,
                description: event.description.as_deref(),
                cwd: event.cwd.as_deref(),
                can_cancel: event.can_cancel,
                usage_json: usage_json.as_deref(),
                action: mapping::subagent_action(event.status),
            },
        )
        .await
        {
            Ok(status) => status,
            Err(error) => {
                tracing::warn!(%error, subagent_id = %event.id, "illegal subagent transition");
                subagents::status_of(&self.inner.pool, &event.id)
                    .await
                    .unwrap_or(event.status)
            }
        };
        let dto = subagent_dto(&event, &self.thread_id, status);
        self.append(ThreadEvent::SubagentUpdated { subagent: dto })
            .await
    }

    async fn finish_turn(&self, stop_reason: String) -> Result<(), ServerError> {
        self.inner.clear_turn(&self.thread_id).await;
        let (turn_id, message_id, text, silent) = {
            let mut run = self.run.lock().await;
            run.thinking_id = None;
            (
                run.turn_id.take(),
                run.agent_message_id.take(),
                std::mem::take(&mut run.agent_text),
                !std::mem::take(&mut run.saw_output),
            )
        };
        if let Some(message_id) = message_id {
            if !text.is_empty() {
                let message = message_dto(&self.thread_id, message_id, turn_id.clone(), text);
                self.append(ThreadEvent::MessageCompleted { message })
                    .await?;
            }
        }
        // Adapters report `end_turn` even for a turn that died upstream: pi
        // turns a provider error (a 402, an expired key) into an empty
        // assistant message and never fails the prompt. Ending "successfully"
        // with nothing to show left the thread looking hung, so a turn that
        // produced nothing at all is reported as a failure instead.
        let failure = (stop_reason != "cancelled" && (silent || is_stop_error(&stop_reason)))
            .then(|| stop_failure_message(&stop_reason, silent));
        match (&failure, turn_id) {
            (Some(message), Some(turn_id)) => {
                self.append(ThreadEvent::TurnError {
                    turn_id: Some(turn_id),
                    message: message.clone(),
                })
                .await?;
            }
            (Some(message), None) => {
                self.append(ThreadEvent::SystemNotice {
                    level: "error".to_owned(),
                    message: message.clone(),
                })
                .await?;
            }
            (None, turn_id) => {
                if let Some(notice) = stop_reason_notice(&stop_reason) {
                    self.append(ThreadEvent::SystemNotice {
                        level: "warning".to_owned(),
                        message: notice.to_owned(),
                    })
                    .await?;
                }
                if let Some(turn_id) = turn_id {
                    self.append(ThreadEvent::TurnEnded {
                        turn_id,
                        stop_reason,
                    })
                    .await?;
                }
            }
        }
        sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&self.thread_id)
            .execute(&self.inner.pool)
            .await?;
        match failure {
            Some(message) => {
                self.transition(ThreadAction::TurnError).await?;
                self.notify("error", &truncate_summary(&message), false)
                    .await;
            }
            None => {
                self.transition(ThreadAction::TurnEnded).await?;
                self.notify("completed", "The agent finished its turn.", true)
                    .await;
            }
        }
        // A finished turn proves the adapter is healthy; forget earlier crashes.
        self.inner.restarts.lock().await.remove(&self.thread_id);
        let _ = self.inner.queue_tx.send(self.thread_id.clone());
        Ok(())
    }

    async fn abort_turn(&self, action: ThreadAction, message: String) -> Result<(), ServerError> {
        self.inner.clear_turn(&self.thread_id).await;
        let summary = truncate_summary(&message);
        // Clone rather than take: if this error was the adapter dying, the
        // following `ProcessExited` still has to know a turn was cut off.
        // A new turn resets the run state, a normal finish clears it.
        let turn_id = self.run.lock().await.turn_id.clone();
        self.append(ThreadEvent::TurnError { turn_id, message })
            .await?;
        sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&self.thread_id)
            .execute(&self.inner.pool)
            .await?;
        self.transition(action).await?;
        self.notify("error", &summary, false).await;
        // A failed turn must not strand the messages queued behind it.
        let _ = self.inner.queue_tx.send(self.thread_id.clone());
        Ok(())
    }

    /// Best-effort watcher notification; it must never fail the turn itself.
    async fn notify(&self, kind: &str, summary: &str, skip_focused: bool) {
        let presence = skip_focused.then_some(&self.inner.presence);
        if let Err(error) = crate::notify::notify_watchers(
            &self.inner.pool,
            &self.inner.projection,
            &self.thread_id,
            kind,
            None,
            None,
            summary,
            presence,
        )
        .await
        {
            tracing::warn!(%error, thread_id = %self.thread_id, "could not notify watchers");
        }
    }

    async fn transition(&self, action: ThreadAction) -> Result<(), ServerError> {
        threads::transition(
            &self.inner.pool,
            &self.inner.projection,
            &self.thread_id,
            action,
        )
        .await?;
        Ok(())
    }

    async fn append(&self, payload: ThreadEvent) -> Result<(), ServerError> {
        self.inner
            .events
            .append(
                &ThreadId::from(self.thread_id.clone()),
                Actor::agent("Threadbase agent"),
                payload,
                EventMeta::default(),
            )
            .await?;
        Ok(())
    }

    /// Answer a request without a person when something already covers it.
    ///
    /// A standing rule wins over the mode: the rule is a deliberate "always
    /// this" from a user, while the mode is only the thread's default posture.
    async fn try_auto_rule(
        &self,
        approval: &threadbase_shared::ApprovalDto,
    ) -> Result<(), ServerError> {
        let agent = crate::bindings::agent(&self.inner.pool, &self.thread_id).await?;
        let reason = match approvals::match_rule(
            &self.inner.pool,
            &self.thread_id,
            &agent,
            approval_kind(approval.kind),
            approval.command.as_deref(),
            approval.cwd.as_deref(),
            approval.subagent_id.is_some(),
        )
        .await?
        {
            Some(rule) => approvals::AutoApproval::Rule(rule),
            None => {
                let mode =
                    threads::permission_mode(&self.inner.pool, &self.thread_id, &self.inner.policy)
                        .await?;
                if !self.inner.policy.allows(mode, approval.kind) {
                    return Ok(());
                }
                approvals::AutoApproval::Mode(mode)
            }
        };
        let Some(session) = self.inner.session(&self.thread_id).await else {
            return Ok(());
        };
        approvals::auto_allow(
            &self.inner.pool,
            &self.inner.projection,
            &self.inner.events,
            approval.id.as_str(),
            &reason,
        )
        .await?;
        {
            let mut harness = session.harness.lock().await;
            harness
                .decide(approval.id.as_str(), ApprovalDecision::AllowOnce, None)
                .await
                .map_err(mapping::agent_error)?;
        }
        let turn_active = self.inner.current_turn(&self.thread_id).await.is_some();
        let remaining = approvals::count_pending_items(&self.inner.pool, &self.thread_id).await?;
        self.transition(ThreadAction::ApprovalResolved {
            turn_active,
            remaining_pending: remaining,
        })
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::push_delta;

    #[test]
    fn folded_text_handles_incremental_and_cumulative_chunks() {
        let mut buffer = String::new();
        for chunk in [".", " Using", " the histogram"] {
            push_delta(&mut buffer, chunk);
        }
        assert_eq!(buffer, ". Using the histogram");

        let mut cumulative = String::new();
        for chunk in ["Hello", "Hello world", "Hello world again"] {
            push_delta(&mut cumulative, chunk);
        }
        assert_eq!(cumulative, "Hello world again");
    }
}
