//! Harness event pump: one task per running session that normalizes agent
//! events into persisted thread events.

use super::mapping::{
    self, approval_kind, approval_option, elicitation_dto, message_dto, subagent_dto, tool_data,
    tool_kind,
};
use super::{Inner, RunState};
use crate::error::ServerError;
use crate::events::EventMeta;
use crate::{approvals, elicitations, subagents, threads};
use std::sync::Arc;
use threadbase_agent::RawAgentEvent;
use threadbase_core::state::ThreadAction;
use threadbase_shared::{
    now_ms, Actor, ApprovalDecision, MessageId, MessageRole, ThreadEvent, ThreadId,
};
use tokio::sync::{broadcast, Mutex};

pub(crate) fn spawn(
    inner: Arc<Inner>,
    thread_id: String,
    agent: String,
    run: Arc<Mutex<RunState>>,
    mut events: broadcast::Receiver<RawAgentEvent>,
) {
    let pump = Pump {
        inner,
        thread_id,
        agent,
        run,
    };
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if let Err(error) = pump.handle(event).await {
                        tracing::warn!(%error, thread_id = %pump.thread_id, "agent pump event failed");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(%skipped, thread_id = %pump.thread_id, "agent pump lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

struct Pump {
    inner: Arc<Inner>,
    thread_id: String,
    agent: String,
    run: Arc<Mutex<RunState>>,
}

impl Pump {
    async fn handle(&self, event: RawAgentEvent) -> Result<(), ServerError> {
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
                let message_id = self
                    .run
                    .lock()
                    .await
                    .thinking_id
                    .get_or_insert_with(MessageId::new)
                    .clone();
                self.append(ThreadEvent::ThinkingDelta { message_id, text })
                    .await
            }
            RawAgentEvent::ToolCall { call } => {
                let call = tool_data(&self.thread_id, call);
                self.append(ThreadEvent::ToolCall { call }).await
            }
            RawAgentEvent::ToolCallUpdate { call } => {
                let call = tool_data(&self.thread_id, call);
                self.append(ThreadEvent::ToolCallUpdate { call }).await
            }
            RawAgentEvent::Plan { entries } => {
                self.append(ThreadEvent::PlanUpdated { entries }).await
            }
            RawAgentEvent::PermissionRequest { request } => self.permission(request).await,
            RawAgentEvent::ElicitationRequest { request } => {
                let elicitation = elicitation_dto(request, &self.thread_id, &self.agent);
                elicitations::create(
                    &self.inner.pool,
                    &self.inner.projection,
                    &self.inner.events,
                    elicitation,
                    Actor::agent("Threadbase agent"),
                    EventMeta::default(),
                )
                .await?;
                self.transition(ThreadAction::ElicitationPending).await
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
                self.transition(ThreadAction::ProcessExited).await
            }
            RawAgentEvent::Notice { message } => {
                self.append(ThreadEvent::SystemNotice {
                    level: "info".into(),
                    message,
                })
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
        let approval = approvals::create(
            &self.inner.pool,
            &self.inner.projection,
            &self.inner.events,
            approvals::NewApproval {
                id: request.id.clone(),
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
        .await?;
        self.transition(ThreadAction::ApprovalPending).await?;
        self.try_auto_rule(&approval).await
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
        let (turn_id, message_id, text) = {
            let mut run = self.run.lock().await;
            run.thinking_id = None;
            (
                run.turn_id.take(),
                run.agent_message_id.take(),
                std::mem::take(&mut run.agent_text),
            )
        };
        if let Some(message_id) = message_id {
            if !text.is_empty() {
                let message = message_dto(&self.thread_id, message_id, turn_id.clone(), text);
                self.append(ThreadEvent::MessageCompleted { message })
                    .await?;
            }
        }
        if let Some(turn_id) = turn_id {
            self.append(ThreadEvent::TurnEnded {
                turn_id,
                stop_reason,
            })
            .await?;
        }
        sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&self.thread_id)
            .execute(&self.inner.pool)
            .await?;
        self.transition(ThreadAction::TurnEnded).await?;
        let _ = self.inner.queue_tx.send(self.thread_id.clone());
        Ok(())
    }

    async fn abort_turn(&self, action: ThreadAction, message: String) -> Result<(), ServerError> {
        self.inner.clear_turn(&self.thread_id).await;
        let turn_id = self.run.lock().await.turn_id.take();
        self.append(ThreadEvent::TurnError { turn_id, message })
            .await?;
        sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&self.thread_id)
            .execute(&self.inner.pool)
            .await?;
        self.transition(action).await?;
        // A failed turn must not strand the messages queued behind it.
        let _ = self.inner.queue_tx.send(self.thread_id.clone());
        Ok(())
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

    async fn try_auto_rule(
        &self,
        approval: &threadbase_shared::ApprovalDto,
    ) -> Result<(), ServerError> {
        let agent = crate::bindings::agent(&self.inner.pool, &self.thread_id).await?;
        let Some(rule) = approvals::match_rule(
            &self.inner.pool,
            &self.thread_id,
            &agent,
            approval_kind(approval.kind),
            approval.command.as_deref(),
            approval.cwd.as_deref(),
            approval.subagent_id.is_some(),
        )
        .await?
        else {
            return Ok(());
        };
        let Some(session) = self.inner.session(&self.thread_id).await else {
            return Ok(());
        };
        approvals::auto_allow(
            &self.inner.pool,
            &self.inner.projection,
            &self.inner.events,
            approval.id.as_str(),
            &rule,
        )
        .await?;
        {
            let mut harness = session.harness.lock().await;
            harness
                .decide(approval.id.as_str(), ApprovalDecision::AllowOnce)
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
