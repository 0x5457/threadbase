//! Bridges persisted thread events to ACP harnesses and vice versa.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::state::StateStore;
use crate::threads;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use threadbase_agent::{
    all_profiles, find_profile, AcpHarness, Capabilities, Harness, LaunchCtx, NewSession,
    RawAgentEvent, SessionId, SteeringOutcome, TurnHandle,
};
use threadbase_core::state::{ApprovalAction, SubagentAction, SubagentMachine, ThreadAction};
use threadbase_core::{resolve_delivery, DeliveryContext, SendAction};
use threadbase_shared::api::{DeliveryMode, SendMessageResponse};
use threadbase_shared::{
    now_ms, user_color, Actor, ApprovalDecision as SharedDecision, ApprovalOption, Delivery,
    ElicitationAction, ElicitationDto, ElicitationMode, MessageDto, MessageId, MessageRole,
    SubagentDto, SubagentStatus, ThreadEvent, ThreadId, ThreadStatus, TurnId, UserBrief,
};
use threadbase_workspace::git;
use tokio::sync::{mpsc, Mutex};
use ulid::Ulid;

#[derive(Clone)]
pub struct AgentManager {
    inner: Arc<Inner>,
}

struct Inner {
    pool: SqlitePool,
    events: EventStore,
    state: StateStore,
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
    queue_tx: mpsc::UnboundedSender<String>,
}

struct Session {
    harness: Box<dyn Harness>,
    capabilities: Capabilities,
    _session_id: Option<SessionId>,
    turn: Option<TurnHandle>,
    run: Arc<Mutex<RunState>>,
}

#[derive(Default)]
struct RunState {
    turn_id: Option<TurnId>,
    user_message_id: Option<MessageId>,
    agent_message_id: Option<MessageId>,
    agent_text: String,
    author: Option<UserBrief>,
}

async fn cancel_pending_approvals(inner: &Inner, thread_id: &str) {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM approvals WHERE thread_id = ? AND status = 'pending'")
            .bind(thread_id)
            .fetch_all(&inner.pool)
            .await
            .unwrap_or_default();
    for id in ids {
        let _ = inner
            .state
            .set_approval_status(thread_id, &id, ApprovalAction::Cancel)
            .await;
        let _ = inner
            .events
            .append(
                &ThreadId::from(thread_id.to_owned()),
                Actor::system(),
                ThreadEvent::PermissionCancelled {
                    approval_id: id.into(),
                },
                EventMeta::default(),
            )
            .await;
    }
}

async fn cancel_pending_elicitations(inner: &Inner, thread_id: &str) {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM elicitations WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(thread_id)
    .fetch_all(&inner.pool)
    .await
    .unwrap_or_default();
    if ids.is_empty() {
        return;
    }
    let _ = inner.state.cancel_elicitations(thread_id).await;
    for id in ids {
        let _ = inner
            .events
            .append(
                &ThreadId::from(thread_id.to_owned()),
                Actor::system(),
                ThreadEvent::ElicitationResolved {
                    elicitation_id: id.into(),
                    action: ElicitationAction::Cancel,
                    response: None,
                    responded_by: None,
                },
                Default::default(),
            )
            .await;
    }
}

impl AgentManager {
    #[must_use]
    pub fn new(pool: SqlitePool, events: EventStore, state: StateStore) -> Self {
        let (queue_tx, mut queue_rx) = mpsc::unbounded_channel::<String>();
        let inner = Arc::new(Inner {
            pool,
            events,
            state,
            sessions: Mutex::new(HashMap::new()),
            queue_tx: queue_tx.clone(),
        });
        let worker = inner.clone();
        tokio::spawn(async move {
            while let Some(thread_id) = queue_rx.recv().await {
                if let Err(error) = process_queue(&worker, &thread_id).await {
                    tracing::warn!(%error, thread_id, "queue processing failed");
                }
            }
        });
        Self { inner }
    }

    pub async fn restart(&self, thread_id: &str) -> Result<(), ServerError> {
        let session = self.inner.sessions.lock().await.remove(thread_id);
        if let Some(session) = session {
            let mut session = session.lock().await;
            let _ = session.harness.shutdown().await;
        }
        self.inner
            .state
            .transition_thread(thread_id, ThreadAction::Restart)
            .await?;
        self.ensure_started(thread_id).await
    }

    pub fn kick_queue(&self, thread_id: &str) {
        let _ = self.inner.queue_tx.send(thread_id.to_owned());
    }

    pub async fn ensure_started(&self, thread_id: &str) -> Result<(), ServerError> {
        if self.inner.sessions.lock().await.contains_key(thread_id) {
            return Ok(());
        }
        let mut sessions = self.inner.sessions.lock().await;
        if sessions.contains_key(thread_id) {
            return Ok(());
        }
        let row = threads::load_row(&self.inner.pool, thread_id).await?;
        let binding: Option<(String, String, Option<String>)> =
            sqlx::query_as("SELECT agent, cwd, session_id FROM agent_bindings WHERE thread_id = ?")
                .bind(thread_id)
                .fetch_optional(&self.inner.pool)
                .await?;
        let Some((agent, cwd, saved_session)) = binding else {
            return Err(ServerError::internal("agent binding is missing"));
        };
        let profile = find_profile(&agent).copied().unwrap_or_else(|| {
            all_profiles()
                .first()
                .copied()
                .expect("at least one agent profile")
        });
        let mut harness = Box::new(AcpHarness::new(profile));
        let mut events = harness.events();
        let mut env = Vec::new();
        if agent == "codex" {
            if let Some(path) = threadbase_agent::profile::find_in_path("codex") {
                env.push(("CODEX_PATH".to_owned(), path.to_string_lossy().to_string()));
            }
        }
        let ctx = LaunchCtx {
            thread_id: thread_id.to_owned(),
            cwd: PathBuf::from(&cwd),
            agent: agent.clone(),
            model: row.model.clone(),
            effort: row.effort.clone(),
            subagents: true,
            env,
        };
        let capabilities = match harness.start(ctx).await {
            Ok(capabilities) => capabilities,
            Err(error) => {
                mark_agent_error(&self.inner, thread_id, &error.to_string()).await;
                return Err(ServerError::internal(format!(
                    "agent failed to start: {error}"
                )));
            }
        };
        let mut reset_context = false;
        let session_id = match saved_session {
            Some(id) => match harness.load_session(&SessionId(id.clone())).await {
                Ok(()) => Some(SessionId(id)),
                Err(_) => {
                    reset_context = true;
                    match harness
                        .new_session(NewSession {
                            cwd: PathBuf::from(&cwd),
                            mcp_servers: Vec::new(),
                        })
                        .await
                    {
                        Ok(id) => Some(id),
                        Err(error) => {
                            mark_agent_error(&self.inner, thread_id, &error.to_string()).await;
                            return Err(ServerError::internal(format!(
                                "agent session failed: {error}"
                            )));
                        }
                    }
                }
            },
            None => match harness
                .new_session(NewSession {
                    cwd: PathBuf::from(&cwd),
                    mcp_servers: Vec::new(),
                })
                .await
            {
                Ok(id) => Some(id),
                Err(error) => {
                    mark_agent_error(&self.inner, thread_id, &error.to_string()).await;
                    return Err(ServerError::internal(format!(
                        "agent session failed: {error}"
                    )));
                }
            },
        };
        if let Some(session) = session_id.as_ref() {
            sqlx::query(
                "UPDATE agent_bindings SET session_id = ?, updated_at = ? WHERE thread_id = ?",
            )
            .bind(session.0.as_str())
            .bind(now_ms())
            .bind(thread_id)
            .execute(&self.inner.pool)
            .await?;
            self.inner
                .state
                .set_binding_status(thread_id, "idle", None)
                .await?;
        }
        if reset_context {
            let _ = self
                .inner
                .events
                .append(
                    &ThreadId::from(thread_id.to_owned()),
                    Actor::system(),
                    ThreadEvent::SystemNotice {
                        level: "warning".to_owned(),
                        message:
                            "The previous agent session could not be restored; context was reset."
                                .to_owned(),
                    },
                    Default::default(),
                )
                .await;
        }
        let run = Arc::new(Mutex::new(RunState::default()));
        let pump_agent = agent.clone();
        let session = Arc::new(Mutex::new(Session {
            harness,
            capabilities,
            _session_id: session_id,
            turn: None,
            run: run.clone(),
        }));
        let pump = Pump {
            inner: self.inner.clone(),
            thread_id: thread_id.to_owned(),
            agent: pump_agent,
            run: run.clone(),
            queue_tx: self.inner.queue_tx.clone(),
        };
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let Err(error) = pump.handle(event).await {
                    tracing::warn!(%error, thread_id = %pump.thread_id, "agent pump event failed");
                }
            }
        });
        sessions.insert(thread_id.to_owned(), session);
        if self.inner.state.thread_status(thread_id).await.ok() == Some(ThreadStatus::Preparing) {
            self.inner
                .state
                .transition_thread(thread_id, ThreadAction::StartSucceeded)
                .await?;
        }
        Ok(())
    }

    pub async fn send(
        &self,
        thread_id: &str,
        user: &CurrentUser,
        text: &str,
        delivery: Option<DeliveryMode>,
    ) -> Result<SendMessageResponse, ServerError> {
        self.ensure_started(thread_id).await?;
        let session = self.session(thread_id).await?;
        let row = threads::load_row(&self.inner.pool, thread_id).await?;
        let mut session = session.lock().await;
        let running = session.turn.is_some() && row.status == "running";
        if !running {
            session.turn = None;
        }
        let action = resolve_delivery(
            delivery.unwrap_or(DeliveryMode::Auto),
            DeliveryContext {
                turn_running: running,
                steering_supported: session.capabilities.steering,
            },
        );
        match action {
            SendAction::Queue => {
                let position =
                    enqueue(&self.inner.pool, &self.inner.state, thread_id, user, text).await?;
                let _ =
                    crate::queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
                Ok(SendMessageResponse {
                    delivery: Delivery::Queued,
                    queue_position: Some(position),
                    message_id: None,
                })
            }
            SendAction::Inject => {
                let outcome = session
                    .harness
                    .steer(vec![text.to_owned()])
                    .await
                    .map_err(agent_error)?;
                match outcome {
                    SteeringOutcome::Injected | SteeringOutcome::StartedNewTurn => {
                        let author = user_brief(user);
                        let turn_id = self.inner.current_turn(thread_id).await.unwrap_or_default();
                        let message = user_message(thread_id, &author, text, Some(turn_id.clone()));
                        self.inner
                            .events
                            .append(
                                &ThreadId::from(thread_id.to_owned()),
                                Actor::user(&author),
                                ThreadEvent::MessageCompleted {
                                    message: message.clone(),
                                },
                                EventMeta {
                                    delivery: Some(Delivery::Injected),
                                    ..Default::default()
                                },
                            )
                            .await?;
                        Ok(SendMessageResponse {
                            delivery: Delivery::Injected,
                            queue_position: None,
                            message_id: Some(message.id.0),
                        })
                    }
                    SteeringOutcome::PromptRequired => {
                        let position =
                            enqueue(&self.inner.pool, &self.inner.state, thread_id, user, text)
                                .await?;
                        Ok(SendMessageResponse {
                            delivery: Delivery::Queued,
                            queue_position: Some(position),
                            message_id: None,
                        })
                    }
                    SteeringOutcome::Failed { reason } => {
                        tracing::warn!(?reason, "steering failed; dropping input");
                        Ok(SendMessageResponse {
                            delivery: Delivery::Dropped,
                            queue_position: None,
                            message_id: None,
                        })
                    }
                }
            }
            SendAction::StopAndSend => {
                if let Some(turn) = session.turn.take() {
                    let _ = session.harness.cancel(&turn).await;
                }
                self.prompt(
                    &mut session,
                    thread_id,
                    user,
                    text,
                    Delivery::StoppedAndSent,
                )
                .await
            }
            SendAction::Prompt => {
                self.prompt(&mut session, thread_id, user, text, Delivery::Injected)
                    .await
            }
        }
    }

    async fn prompt(
        &self,
        session: &mut Session,
        thread_id: &str,
        user: &CurrentUser,
        text: &str,
        delivery: Delivery,
    ) -> Result<SendMessageResponse, ServerError> {
        auto_title(&self.inner, thread_id, text).await;
        if let Some(worktree) = threads::load_row(&self.inner.pool, thread_id)
            .await
            .ok()
            .and_then(|row| row.worktree_path)
        {
            if let Ok(snapshot) =
                git::snapshot_worktree(std::path::Path::new(&worktree), "Threadbase turn snapshot")
                    .await
            {
                let _ = sqlx::query(
                    "UPDATE threads SET last_turn_sha = ?, updated_at = ? WHERE id = ?",
                )
                .bind(snapshot)
                .bind(now_ms())
                .bind(thread_id)
                .execute(&self.inner.pool)
                .await;
            }
        }
        let turn_id = TurnId::new();
        let message_id = MessageId::new();
        let author = user_brief(user);
        self.inner
            .events
            .append(
                &ThreadId::from(thread_id.to_owned()),
                Actor::user(&author),
                ThreadEvent::TurnStarted {
                    turn_id: turn_id.clone(),
                    message_id: message_id.clone(),
                    author: author.clone(),
                    prompt: text.to_owned(),
                },
                EventMeta {
                    delivery: Some(delivery),
                    ..Default::default()
                },
            )
            .await?;
        {
            let mut run = session.run.lock().await;
            run.turn_id = Some(turn_id);
            run.user_message_id = Some(message_id.clone());
            run.agent_message_id = None;
            run.agent_text.clear();
            run.author = Some(author);
        }
        let mut parts = vec![text.to_owned()];
        if let Some(summary) = self.inner.state.summary(thread_id).await {
            parts.push(summary);
        }
        let handle = session.harness.prompt(parts).await.map_err(agent_error)?;
        session.turn = Some(handle);
        self.inner
            .state
            .transition_thread(thread_id, ThreadAction::Prompt)
            .await?;
        Ok(SendMessageResponse {
            delivery,
            queue_position: None,
            message_id: Some(message_id.0),
        })
    }

    pub async fn config_options(
        &self,
        thread_id: &str,
    ) -> Result<Vec<threadbase_shared::AgentConfigOptionDto>, ServerError> {
        let session = self.session(thread_id).await?;
        let session = session.lock().await;
        Ok(session.harness.config_options())
    }

    pub async fn set_config(
        &self,
        thread_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        session
            .harness
            .set_config(model, effort)
            .await
            .map_err(agent_error)
    }

    pub async fn cancel(&self, thread_id: &str) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        if let Some(turn) = session.turn.take() {
            session.harness.cancel(&turn).await.map_err(agent_error)?;
        }
        crate::queue::mark_superseded(
            &self.inner.state,
            thread_id,
            session.turn.as_ref().map(|turn| turn.turn_id.as_str()),
        )
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?;
        cancel_pending_approvals(&self.inner, thread_id).await;
        cancel_pending_elicitations(&self.inner, thread_id).await;
        let _ = self
            .inner
            .state
            .transition_thread(thread_id, ThreadAction::Cancel)
            .await;
        let _ = crate::queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
        Ok(())
    }

    pub async fn decide(
        &self,
        thread_id: &str,
        approval_id: &str,
        decision: SharedDecision,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        session
            .harness
            .decide(approval_id, decision)
            .await
            .map_err(agent_error)?;
        let turn_active = session.turn.is_some();
        let remaining = crate::approvals::count_pending_items(&self.inner.pool, thread_id).await?;
        self.inner
            .state
            .transition_thread(
                thread_id,
                ThreadAction::ApprovalResolved {
                    turn_active,
                    remaining_pending: remaining,
                },
            )
            .await?;
        Ok(())
    }

    pub async fn respond_elicitation(
        &self,
        thread_id: &str,
        elicitation_id: &str,
        action: threadbase_shared::ElicitationAction,
        values: Option<serde_json::Value>,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        let mapped = match action {
            threadbase_shared::ElicitationAction::Accept => ElicitationAction::Accept,
            threadbase_shared::ElicitationAction::Decline => ElicitationAction::Decline,
            threadbase_shared::ElicitationAction::Cancel => ElicitationAction::Cancel,
        };
        session
            .harness
            .respond_elicitation(elicitation_id, mapped, values)
            .await
            .map_err(agent_error)?;
        let turn_active = session.turn.is_some();
        let remaining = crate::approvals::count_pending_items(&self.inner.pool, thread_id).await?;
        self.inner
            .state
            .transition_thread(
                thread_id,
                ThreadAction::ElicitationResolved {
                    turn_active,
                    remaining_pending: remaining,
                },
            )
            .await?;
        Ok(())
    }

    pub async fn cancel_subagent(
        &self,
        thread_id: &str,
        subagent_id: &str,
    ) -> Result<(), ServerError> {
        let status = self
            .inner
            .state
            .subagent_status(thread_id, subagent_id)
            .await?;
        if SubagentMachine::is_terminal(status) || !SubagentMachine::can_cancel(status) {
            return Err(ServerError(threadbase_shared::ApiError::conflict(
                "subagent can no longer be cancelled",
            )));
        }
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        session
            .harness
            .cancel_subagent(subagent_id)
            .await
            .map_err(agent_error)
    }

    async fn session(&self, thread_id: &str) -> Result<Arc<Mutex<Session>>, ServerError> {
        self.inner
            .sessions
            .lock()
            .await
            .get(thread_id)
            .cloned()
            .ok_or_else(|| ServerError::internal("agent is not running"))
    }
}

impl Inner {
    /// Clear the in-memory turn handle for a thread.
    ///
    /// Called before a turn's completion events are published so a concurrent
    /// `send` observes an idle session instead of injecting into a finished turn.
    async fn clear_turn(&self, thread_id: &str) {
        if let Some(session) = self.sessions.lock().await.get(thread_id).cloned() {
            session.lock().await.turn = None;
        }
    }

    async fn current_turn(&self, thread_id: &str) -> Option<TurnId> {
        sqlx::query_scalar(
            "SELECT payload_json FROM events WHERE thread_id = ? AND kind = 'turn_started'
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(thread_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .and_then(|json: String| serde_json::from_str::<ThreadEvent>(&json).ok())
        .and_then(|event| match event {
            ThreadEvent::TurnStarted { turn_id, .. } => Some(turn_id),
            _ => None,
        })
    }
}

struct Pump {
    inner: Arc<Inner>,
    thread_id: String,
    agent: String,
    run: Arc<Mutex<RunState>>,
    queue_tx: mpsc::UnboundedSender<String>,
}

impl Pump {
    async fn handle(&self, event: RawAgentEvent) -> Result<(), ServerError> {
        match event {
            RawAgentEvent::AgentMessageChunk { text } => {
                let (message_id, author) = {
                    let mut run = self.run.lock().await;
                    if run.agent_message_id.is_none() {
                        run.agent_message_id = Some(MessageId::new());
                    }
                    (run.agent_message_id.clone().unwrap(), run.author.clone())
                };
                {
                    let mut run = self.run.lock().await;
                    run.agent_text.push_str(&text);
                }
                let _ = author;
                self.append(
                    ThreadEvent::MessageDelta {
                        message_id,
                        role: MessageRole::Agent,
                        text,
                        author: None,
                    },
                    EventMeta::default(),
                )
                .await?;
            }
            RawAgentEvent::AgentThoughtChunk { text } => {
                let message_id = self
                    .run
                    .lock()
                    .await
                    .agent_message_id
                    .clone()
                    .unwrap_or_default();
                self.append(
                    ThreadEvent::ThinkingDelta { message_id, text },
                    EventMeta::default(),
                )
                .await?;
            }
            RawAgentEvent::ToolCall { call } => {
                let data = tool_data(&self.thread_id, call);
                self.append(ThreadEvent::ToolCall { call: data }, EventMeta::default())
                    .await?;
            }
            RawAgentEvent::ToolCallUpdate { call } => {
                let data = tool_data(&self.thread_id, call);
                self.append(
                    ThreadEvent::ToolCallUpdate { call: data },
                    EventMeta::default(),
                )
                .await?;
            }
            RawAgentEvent::Plan { entries } => {
                self.append(ThreadEvent::PlanUpdated { entries }, EventMeta::default())
                    .await?;
            }
            RawAgentEvent::PermissionRequest { request } => {
                let options: Vec<ApprovalOption> = request
                    .options
                    .iter()
                    .map(|option| ApprovalOption {
                        id: option.id.clone(),
                        kind: match option.kind.as_str() {
                            "allow_once" => SharedDecision::AllowOnce,
                            "allow_always" => SharedDecision::AllowSession,
                            "reject_once" | "reject_always" => SharedDecision::Deny,
                            _ => SharedDecision::Deny,
                        },
                        label: option.label.clone(),
                    })
                    .collect();
                let approval = crate::approvals::create(
                    &self.inner.pool,
                    &self.inner.state,
                    &self.inner.events,
                    crate::approvals::NewApproval {
                        id: request.id.clone(),
                        thread_id: self.thread_id.clone(),
                        subagent_id: None,
                        turn_id: self.run.lock().await.turn_id.clone().map(|id| id.0),
                        tool_call_id: None,
                        agent: self.agent.clone(),
                        title: request.title,
                        kind: tool_kind_str(request.kind).to_owned(),
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
                self.set_thread_status(ThreadAction::ApprovalPending)
                    .await?;
                self.try_auto_rule(&approval).await?;
            }
            RawAgentEvent::ElicitationRequest { request } => {
                let elicitation = ElicitationDto {
                    id: request.id.into(),
                    thread_id: ThreadId::from(self.thread_id.clone()),
                    subagent_id: None,
                    tool_call_id: request.tool_call_id.map(Into::into),
                    agent: self.agent.clone(),
                    message: request.message,
                    mode: if request.mode == "url" {
                        ElicitationMode::Url
                    } else {
                        ElicitationMode::Form
                    },
                    requested_schema: request.requested_schema,
                    blocking: request.blocking,
                    status: threadbase_shared::ElicitationStatus::Pending,
                    response: None,
                    responded_by: None,
                    responded_at: None,
                    created_at: now_ms(),
                };
                crate::approvals::create_elicitation(
                    &self.inner.pool,
                    &self.inner.state,
                    &self.inner.events,
                    elicitation,
                    Actor::agent("Threadbase agent"),
                    EventMeta::default(),
                )
                .await?;
                self.set_thread_status(ThreadAction::ElicitationPending)
                    .await?;
            }
            RawAgentEvent::Subagent { event } => {
                let usage_json = event
                    .usage
                    .as_ref()
                    .map(|usage| serde_json::to_string(usage).unwrap_or_default());
                let next_status = match self
                    .inner
                    .state
                    .create_subagent(
                        &self.thread_id,
                        &event.id,
                        event.parent_id.as_deref(),
                        &event.title,
                        event.description.as_deref(),
                        event.cwd.as_deref(),
                        event.can_cancel,
                        usage_json.as_deref(),
                        subagent_action(event.status),
                    )
                    .await
                {
                    Ok(next) => next,
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            subagent_id = %event.id,
                            "illegal subagent transition"
                        );
                        self.inner
                            .state
                            .subagent_status(&self.thread_id, &event.id)
                            .await
                            .unwrap_or(event.status)
                    }
                };
                let now = now_ms();
                let dto = SubagentDto {
                    id: event.id.into(),
                    thread_id: ThreadId::from(self.thread_id.clone()),
                    parent_id: event.parent_id.map(Into::into),
                    title: event.title,
                    description: event.description,
                    status: next_status,
                    cwd: event.cwd,
                    can_cancel: event.can_cancel,
                    usage: event.usage,
                    last_seq: 0,
                    created_at: now,
                    updated_at: now,
                };
                self.append(
                    ThreadEvent::SubagentUpdated { subagent: dto },
                    EventMeta::default(),
                )
                .await?;
            }
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
                self.append(
                    ThreadEvent::AgentMessage {
                        direction,
                        from,
                        to,
                        text,
                    },
                    EventMeta::default(),
                )
                .await?;
            }
            RawAgentEvent::Usage { usage } => {
                self.append(ThreadEvent::UsageUpdated { usage }, EventMeta::default())
                    .await?;
            }
            RawAgentEvent::PromptDone { stop_reason } => {
                // Publish the turn as finished in memory before the completion
                // events so a concurrent send starts a fresh prompt.
                self.inner.clear_turn(&self.thread_id).await;
                let (turn_id, message_id, text, author) = {
                    let mut run = self.run.lock().await;
                    (
                        run.turn_id.take(),
                        run.agent_message_id.take(),
                        std::mem::take(&mut run.agent_text),
                        run.author.take(),
                    )
                };
                if let (Some(message_id), false) = (message_id.clone(), text.is_empty()) {
                    let message = MessageDto {
                        id: message_id,
                        thread_id: ThreadId::from(self.thread_id.clone()),
                        turn_id: turn_id.clone(),
                        role: MessageRole::Agent,
                        author: None,
                        authors: Vec::new(),
                        text,
                        model: None,
                        streaming: false,
                        created_at: now_ms(),
                    };
                    self.append(
                        ThreadEvent::MessageCompleted { message },
                        EventMeta::default(),
                    )
                    .await?;
                }
                if let Some(turn_id) = turn_id {
                    self.append(
                        ThreadEvent::TurnEnded {
                            turn_id,
                            stop_reason,
                        },
                        EventMeta::default(),
                    )
                    .await?;
                }
                self.set_thread_status(ThreadAction::TurnEnded).await?;
                let _ = self.queue_tx.send(self.thread_id.clone());
                let _ = author;
            }
            RawAgentEvent::Error { message } => {
                self.append(
                    ThreadEvent::TurnError {
                        turn_id: self.run.lock().await.turn_id.clone(),
                        message,
                    },
                    EventMeta::default(),
                )
                .await?;
                self.set_thread_status(ThreadAction::TurnError).await?;
            }
            RawAgentEvent::ProcessExited { code, signal } => {
                self.append(
                    ThreadEvent::ProcessExited {
                        code,
                        signal,
                        message: "agent process exited".into(),
                    },
                    EventMeta::default(),
                )
                .await?;
                self.set_thread_status(ThreadAction::ProcessExited).await?;
            }
            RawAgentEvent::Notice { message } => {
                self.append(
                    ThreadEvent::SystemNotice {
                        level: "info".into(),
                        message,
                    },
                    EventMeta::default(),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn try_auto_rule(
        &self,
        approval: &threadbase_shared::ApprovalDto,
    ) -> Result<(), ServerError> {
        let agent: String =
            sqlx::query_scalar("SELECT agent FROM agent_bindings WHERE thread_id = ?")
                .bind(&self.thread_id)
                .fetch_one(&self.inner.pool)
                .await?;
        let Some(rule) = crate::approvals::match_rule(
            &self.inner.pool,
            &self.thread_id,
            &agent,
            approval_kind_db(approval.kind),
            approval.command.as_deref(),
            approval.cwd.as_deref(),
            approval.subagent_id.is_some(),
        )
        .await?
        else {
            return Ok(());
        };
        let session = self
            .inner
            .sessions
            .lock()
            .await
            .get(&self.thread_id)
            .cloned();
        let Some(session) = session else {
            return Ok(());
        };
        crate::approvals::auto_allow(
            &self.inner.pool,
            &self.inner.state,
            &self.inner.events,
            approval.id.as_str(),
            &rule,
        )
        .await?;
        let turn_active = {
            let mut session = session.lock().await;
            session
                .harness
                .decide(approval.id.as_str(), SharedDecision::AllowOnce)
                .await
                .map_err(agent_error)?;
            session.turn.is_some()
        };
        let remaining =
            crate::approvals::count_pending_items(&self.inner.pool, &self.thread_id).await?;
        self.inner
            .state
            .transition_thread(
                &self.thread_id,
                ThreadAction::ApprovalResolved {
                    turn_active,
                    remaining_pending: remaining,
                },
            )
            .await?;
        Ok(())
    }

    async fn append(&self, payload: ThreadEvent, meta: EventMeta) -> Result<(), ServerError> {
        self.inner
            .events
            .append(
                &ThreadId::from(self.thread_id.clone()),
                Actor::agent("Threadbase agent"),
                payload,
                meta,
            )
            .await?;
        Ok(())
    }

    async fn set_thread_status(&self, action: ThreadAction) -> Result<(), ServerError> {
        self.inner
            .state
            .transition_thread(&self.thread_id, action)
            .await?;
        Ok(())
    }
}

async fn process_queue(inner: &Arc<Inner>, thread_id: &str) -> Result<(), ServerError> {
    let session = inner
        .sessions
        .lock()
        .await
        .get(thread_id)
        .cloned()
        .ok_or_else(|| ServerError::internal("agent is not running"))?;
    let mut session = session.lock().await;
    let thread_status: String = sqlx::query_scalar("SELECT status FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_one(&inner.pool)
        .await?;
    if session.turn.is_some() && thread_status == "running" {
        return Ok(());
    }
    if thread_status != "running" {
        session.turn = None;
    }
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, author_id, text FROM queued_inputs WHERE thread_id = ? AND status = 'queued'
         AND superseded = 0 ORDER BY position ASC LIMIT 1",
    )
    .bind(thread_id)
    .fetch_optional(&inner.pool)
    .await?;
    let Some((id, author_id, text)) = row else {
        return Ok(());
    };
    inner.state.dispatch_queue_item(thread_id, &id).await?;
    let author = crate::auth::load_user_by_id(&inner.pool, &author_id)
        .await?
        .map(|user| user_dto_brief(&user))
        .unwrap_or_else(|| UserBrief {
            id: author_id.clone().into(),
            username: "member".into(),
            display_name: "Member".into(),
            avatar_url: None,
            color: user_color(&author_id).to_owned(),
        });
    auto_title(inner, thread_id, &text).await;
    let turn_id = TurnId::new();
    let message_id = MessageId::new();
    inner
        .events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::user(&author),
            ThreadEvent::TurnStarted {
                turn_id: turn_id.clone(),
                message_id,
                author: author.clone(),
                prompt: text.clone(),
            },
            EventMeta {
                delivery: Some(Delivery::Queued),
                ..Default::default()
            },
        )
        .await?;
    {
        let mut run = session.run.lock().await;
        run.turn_id = Some(turn_id);
        run.agent_message_id = None;
        run.agent_text.clear();
        run.author = Some(author);
    }
    let mut parts = vec![text];
    if let Some(summary) = inner.state.summary(thread_id).await {
        parts.push(summary);
    }
    match session.harness.prompt(parts).await {
        Ok(handle) => {
            session.turn = Some(handle);
            inner
                .state
                .complete_queue_item(
                    thread_id,
                    &id,
                    session.turn.as_ref().map(|turn| turn.turn_id.as_str()),
                )
                .await?;
            inner
                .state
                .transition_thread(thread_id, ThreadAction::Prompt)
                .await?;
        }
        Err(error) => {
            inner.state.fail_queue_item(thread_id, &id).await?;
            let _ = crate::queue::publish(&inner.pool, &inner.events, thread_id).await;
            return Err(agent_error(error));
        }
    }
    Ok(())
}

async fn enqueue(
    pool: &SqlitePool,
    state: &crate::state::StateStore,
    thread_id: &str,
    user: &CurrentUser,
    text: &str,
) -> Result<u32, ServerError> {
    let position: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM queued_inputs WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    let id = Ulid::new().to_string();
    state
        .enqueue(
            &id,
            thread_id,
            user.user.id.as_str(),
            text,
            position,
            now_ms(),
        )
        .await?;
    Ok(u32::try_from(position).unwrap_or(0))
}

async fn mark_agent_error(inner: &Arc<Inner>, thread_id: &str, message: &str) {
    let _ = inner
        .state
        .transition_thread(thread_id, ThreadAction::AgentFailed)
        .await;
    let _ = inner
        .state
        .set_binding_status(thread_id, "error", Some(message))
        .await;
    let _ = inner
        .events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::agent("Threadbase"),
            ThreadEvent::TurnError {
                turn_id: None,
                message: message.to_owned(),
            },
            EventMeta::default(),
        )
        .await;
}

async fn auto_title(inner: &Arc<Inner>, thread_id: &str, text: &str) {
    let title: Option<String> = sqlx::query_scalar("SELECT title FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_optional(&inner.pool)
        .await
        .ok()
        .flatten();
    if title.as_deref() != Some("New thread") {
        return;
    }
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return;
    }
    let mut next: String = first_line.chars().take(60).collect();
    if next.len() < first_line.len() {
        next.push('…');
    }
    let _ = sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
        .bind(&next)
        .bind(now_ms())
        .bind(thread_id)
        .execute(&inner.pool)
        .await;
    let _ = inner
        .events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            ThreadEvent::ThreadUpdated {
                title: Some(next),
                status: None,
                pinned: None,
                share_level: None,
            },
            EventMeta::default(),
        )
        .await;
}

fn user_brief(user: &CurrentUser) -> UserBrief {
    user_dto_brief(&user.user)
}

fn user_dto_brief(user: &threadbase_shared::UserDto) -> UserBrief {
    UserBrief {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        color: user_color(user.id.as_str()).to_owned(),
    }
}

fn user_message(
    thread_id: &str,
    author: &UserBrief,
    text: &str,
    turn_id: Option<TurnId>,
) -> MessageDto {
    MessageDto {
        id: MessageId::new(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        turn_id,
        role: MessageRole::User,
        author: Some(author.clone()),
        authors: vec![author.clone()],
        text: text.to_owned(),
        model: None,
        streaming: false,
        created_at: now_ms(),
    }
}

fn tool_data(
    thread_id: &str,
    call: threadbase_agent::ToolCallEvent,
) -> threadbase_shared::ToolCallData {
    threadbase_shared::ToolCallData {
        id: call.id.into(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        title: call.title,
        kind: call.kind,
        status: call.status,
        input: call.input,
        output: call.output,
        locations: call.locations,
        diff: call.diff,
        terminal_id: call.terminal_id,
        subagent_id: None,
        created_at: now_ms(),
        updated_at: now_ms(),
    }
}

fn approval_kind_db(kind: threadbase_shared::ApprovalKind) -> &'static str {
    match kind {
        threadbase_shared::ApprovalKind::Command => "command",
        threadbase_shared::ApprovalKind::FileRead => "file_read",
        threadbase_shared::ApprovalKind::FileWrite => "file_write",
        threadbase_shared::ApprovalKind::Network => "network",
        threadbase_shared::ApprovalKind::Other => "other",
    }
}

fn tool_kind_str(kind: threadbase_shared::ToolKind) -> &'static str {
    match kind {
        threadbase_shared::ToolKind::Read => "file_read",
        threadbase_shared::ToolKind::Edit => "file_write",
        threadbase_shared::ToolKind::Delete => "file_write",
        threadbase_shared::ToolKind::Move => "file_write",
        threadbase_shared::ToolKind::Execute => "command",
        _ => "other",
    }
}

fn subagent_action(status: SubagentStatus) -> SubagentAction {
    match status {
        SubagentStatus::Running => SubagentAction::Run,
        SubagentStatus::Idle => SubagentAction::Idle,
        SubagentStatus::RequiresAction => SubagentAction::RequireAction,
        SubagentStatus::Error => SubagentAction::Fail,
        SubagentStatus::Cancelled => SubagentAction::Cancel,
    }
}

fn agent_error(error: threadbase_agent::HarnessError) -> ServerError {
    ServerError::internal(error.to_string())
}
