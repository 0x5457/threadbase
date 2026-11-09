//! Bridges persisted thread events to ACP harnesses and vice versa.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::threads;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use threadbase_agent::{
    all_profiles, find_profile, AcpHarness, Capabilities, Harness, LaunchCtx, NewSession,
    RawAgentEvent, SessionId, SteeringOutcome, TurnHandle,
};
use threadbase_core::{resolve_delivery, DeliveryContext, SendAction};
use threadbase_shared::api::{DeliveryMode, SendMessageResponse};
use threadbase_shared::{
    now_ms, user_color, Actor, ApprovalDecision as SharedDecision, ApprovalOption, Delivery,
    ElicitationAction, ElicitationDto, ElicitationMode, MessageDto, MessageId, MessageRole,
    SubagentDto, SubagentStatus, ThreadEvent, ThreadId, ThreadStatus, TurnId, UserBrief,
};
use tokio::sync::{mpsc, Mutex};
use ulid::Ulid;

#[derive(Clone)]
pub struct AgentManager {
    inner: Arc<Inner>,
}

struct Inner {
    pool: SqlitePool,
    events: EventStore,
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

impl AgentManager {
    #[must_use]
    pub fn new(pool: SqlitePool, events: EventStore) -> Self {
        let (queue_tx, mut queue_rx) = mpsc::unbounded_channel::<String>();
        let inner = Arc::new(Inner {
            pool,
            events,
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
        let ctx = LaunchCtx {
            thread_id: thread_id.to_owned(),
            cwd: PathBuf::from(&cwd),
            agent,
            model: row.model.clone(),
            effort: row.effort.clone(),
            subagents: true,
            env: Vec::new(),
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
        let session_id = match saved_session {
            Some(id) if harness.load_session(&SessionId(id.clone())).await.is_ok() => {
                Some(SessionId(id))
            }
            _ => match harness
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
            sqlx::query("UPDATE agent_bindings SET session_id = ?, status = 'idle', updated_at = ? WHERE thread_id = ?")
                .bind(session.0.as_str())
                .bind(now_ms())
                .bind(thread_id)
                .execute(&self.inner.pool)
                .await?;
        }
        let run = Arc::new(Mutex::new(RunState::default()));
        let pump = Pump {
            pool: self.inner.pool.clone(),
            events: self.inner.events.clone(),
            thread_id: thread_id.to_owned(),
            run: run.clone(),
            queue_tx: self.inner.queue_tx.clone(),
        };
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if pump.handle(event).await.is_err() {
                    break;
                }
            }
        });
        sessions.insert(
            thread_id.to_owned(),
            Arc::new(Mutex::new(Session {
                harness,
                capabilities,
                _session_id: session_id,
                turn: None,
                run,
            })),
        );
        sqlx::query("UPDATE threads SET status = 'idle', updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(thread_id)
            .execute(&self.inner.pool)
            .await?;
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
                let position = enqueue(&self.inner.pool, thread_id, user, text).await?;
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
                        let position = enqueue(&self.inner.pool, thread_id, user, text).await?;
                        Ok(SendMessageResponse {
                            delivery: Delivery::Queued,
                            queue_position: Some(position),
                            message_id: None,
                        })
                    }
                    SteeringOutcome::Failed { reason } => Err(ServerError::internal(
                        reason.unwrap_or_else(|| "steering failed".into()),
                    )),
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
        let handle = session
            .harness
            .prompt(vec![text.to_owned()])
            .await
            .map_err(agent_error)?;
        session.turn = Some(handle);
        self.inner
            .set_thread_status(thread_id, ThreadStatus::Running)
            .await?;
        Ok(SendMessageResponse {
            delivery,
            queue_position: None,
            message_id: Some(message_id.0),
        })
    }

    pub async fn cancel(&self, thread_id: &str) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut session = session.lock().await;
        if let Some(turn) = session.turn.take() {
            session.harness.cancel(&turn).await.map_err(agent_error)?;
        }
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
        self.inner
            .set_thread_status(thread_id, ThreadStatus::Running)
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
        Ok(())
    }

    pub async fn cancel_subagent(
        &self,
        thread_id: &str,
        subagent_id: &str,
    ) -> Result<(), ServerError> {
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
    async fn set_thread_status(
        &self,
        thread_id: &str,
        status: ThreadStatus,
    ) -> Result<(), ServerError> {
        let value = status_str(status);
        sqlx::query("UPDATE threads SET status = ?, updated_at = ? WHERE id = ?")
            .bind(value)
            .bind(now_ms())
            .bind(thread_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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
    pool: SqlitePool,
    events: EventStore,
    thread_id: String,
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
                crate::approvals::create(
                    &self.pool,
                    &self.events,
                    crate::approvals::NewApproval {
                        id: request.id.clone(),
                        thread_id: self.thread_id.clone(),
                        subagent_id: None,
                        turn_id: self.run.lock().await.turn_id.clone().map(|id| id.0),
                        tool_call_id: None,
                        agent: "agent".into(),
                        title: request.title,
                        kind: tool_kind_str(request.kind).to_owned(),
                        command: request.command,
                        cwd: request.cwd,
                        paths: request.paths,
                        options,
                    },
                    Actor::agent("Threadbase agent"),
                    EventMeta::default(),
                )
                .await?;
                self.set_status("requires_action").await?;
            }
            RawAgentEvent::ElicitationRequest { request } => {
                let elicitation = ElicitationDto {
                    id: request.id.into(),
                    thread_id: ThreadId::from(self.thread_id.clone()),
                    subagent_id: None,
                    tool_call_id: request.tool_call_id.map(Into::into),
                    agent: "agent".into(),
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
                    &self.pool,
                    &self.events,
                    elicitation,
                    Actor::agent("Threadbase agent"),
                    EventMeta::default(),
                )
                .await?;
                self.set_status("requires_action").await?;
            }
            RawAgentEvent::Subagent { event } => {
                let now = now_ms();
                sqlx::query(
                    "INSERT INTO subagents(id, thread_id, parent_id, title, description, status, cwd, can_cancel, usage_json, last_seq, created_at, updated_at)
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
                     ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description,
                        status = excluded.status, cwd = excluded.cwd, can_cancel = excluded.can_cancel,
                        usage_json = excluded.usage_json, updated_at = excluded.updated_at",
                )
                .bind(&event.id)
                .bind(&self.thread_id)
                .bind(event.parent_id.as_deref())
                .bind(&event.title)
                .bind(event.description.as_deref())
                .bind(subagent_status_str(event.status))
                .bind(event.cwd.as_deref())
                .bind(i64::from(event.can_cancel))
                .bind(event.usage.as_ref().map(|usage| serde_json::to_string(usage).unwrap_or_default()))
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;
                let dto = SubagentDto {
                    id: event.id.into(),
                    thread_id: ThreadId::from(self.thread_id.clone()),
                    parent_id: event.parent_id.map(Into::into),
                    title: event.title,
                    description: event.description,
                    status: event.status,
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
            RawAgentEvent::Usage { usage } => {
                self.append(ThreadEvent::UsageUpdated { usage }, EventMeta::default())
                    .await?;
            }
            RawAgentEvent::PromptDone { stop_reason } => {
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
                self.set_status("idle").await?;
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
                self.set_status("error").await?;
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
                self.set_status("interrupted").await?;
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

    async fn append(&self, payload: ThreadEvent, meta: EventMeta) -> Result<(), ServerError> {
        self.events
            .append(
                &ThreadId::from(self.thread_id.clone()),
                Actor::agent("Threadbase agent"),
                payload,
                meta,
            )
            .await?;
        Ok(())
    }

    async fn set_status(&self, status: &str) -> Result<(), ServerError> {
        sqlx::query("UPDATE threads SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(now_ms())
            .bind(&self.thread_id)
            .execute(&self.pool)
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
         ORDER BY position ASC LIMIT 1",
    )
    .bind(thread_id)
    .fetch_optional(&inner.pool)
    .await?;
    let Some((id, author_id, text)) = row else {
        return Ok(());
    };
    sqlx::query("UPDATE queued_inputs SET status = 'sending', updated_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(&id)
        .execute(&inner.pool)
        .await?;
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
    match session.harness.prompt(vec![text]).await {
        Ok(handle) => {
            session.turn = Some(handle);
            sqlx::query("UPDATE queued_inputs SET status = 'sent', turn_id = ?, updated_at = ? WHERE id = ?")
                .bind(session.turn.as_ref().map(|turn| turn.turn_id.clone()))
                .bind(now_ms())
                .bind(&id)
                .execute(&inner.pool)
                .await?;
            inner
                .set_thread_status(thread_id, ThreadStatus::Running)
                .await?;
        }
        Err(error) => {
            sqlx::query("UPDATE queued_inputs SET status = 'dropped', updated_at = ? WHERE id = ?")
                .bind(now_ms())
                .bind(&id)
                .execute(&inner.pool)
                .await?;
            return Err(agent_error(error));
        }
    }
    Ok(())
}

async fn enqueue(
    pool: &SqlitePool,
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
    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
         VALUES(?, ?, ?, ?, 'queued', ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(thread_id)
    .bind(user.user.id.as_str())
    .bind(text)
    .bind(position)
    .bind(now_ms())
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(u32::try_from(position).unwrap_or(0))
}

async fn mark_agent_error(inner: &Arc<Inner>, thread_id: &str, message: &str) {
    let _ = sqlx::query("UPDATE threads SET status = 'error', updated_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(thread_id)
        .execute(&inner.pool)
        .await;
    let _ = sqlx::query("UPDATE agent_bindings SET status = 'error', last_error = ?, updated_at = ? WHERE thread_id = ?")
        .bind(message)
        .bind(now_ms())
        .bind(thread_id)
        .execute(&inner.pool)
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
        diff: None,
        terminal_id: call.terminal_id,
        subagent_id: None,
        created_at: now_ms(),
        updated_at: now_ms(),
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

fn subagent_status_str(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Running => "running",
        SubagentStatus::RequiresAction => "requires_action",
        SubagentStatus::Idle => "idle",
        SubagentStatus::Cancelled => "cancelled",
        SubagentStatus::Error => "error",
    }
}

fn status_str(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::Preparing => "preparing",
        ThreadStatus::Idle => "idle",
        ThreadStatus::Running => "running",
        ThreadStatus::RequiresAction => "requires_action",
        ThreadStatus::Interrupted => "interrupted",
        ThreadStatus::Error => "error",
        ThreadStatus::Archived => "archived",
    }
}

fn agent_error(error: threadbase_agent::HarnessError) -> ServerError {
    ServerError::internal(error.to_string())
}
