//! Bridges persisted thread events to ACP harnesses and vice versa.
//!
//! Responsibilities are split by concern:
//! - this module owns sessions and the public [`AgentManager`] facade,
//! - [`pump`] turns harness events into persisted thread events,
//! - [`queue_worker`] drains the input queue,
//! - [`lifecycle`] and [`mapping`] hold the remaining side effects and
//!   conversions.

mod lifecycle;
mod mapping;
mod pump;
mod queue_worker;
mod session;

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::projection::Projection;
use crate::{approvals, bindings, elicitations, queue, subagents, threads};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use threadbase_agent::{
    all_profiles, find_profile, AcpHarness, Harness, LaunchCtx, NewSession, SessionId,
    SteeringOutcome, TurnHandle,
};
use threadbase_core::state::{ApprovalAction, SubagentMachine, ThreadAction};
use threadbase_core::{resolve_delivery, DeliveryContext, SendAction};
use threadbase_shared::api::{DeliveryMode, SendMessageResponse};
use threadbase_shared::{
    now_ms, Actor, ApprovalDecision as SharedDecision, Delivery, ElicitationAction, MessageId,
    ThreadEvent, ThreadId, ThreadStatus, TurnId, UserBrief,
};
use threadbase_workspace::git;
use tokio::sync::{mpsc, Mutex};

pub use mapping::{briefs_by_ids, user_brief};
pub(crate) use session::{RunState, Session, TurnInstall, TurnStartClaim};

#[derive(Clone)]
pub struct AgentManager {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) pool: SqlitePool,
    pub(crate) events: EventStore,
    pub(crate) projection: Projection,
    pub(crate) sessions: Mutex<HashMap<String, Arc<Session>>>,
    pub(crate) starts: crate::locks::KeyedLocks,
    pub(crate) queue_tx: mpsc::UnboundedSender<String>,
}

/// Everything needed to start one turn, whichever input path produced it.
pub(crate) struct TurnStart<'a> {
    pub thread_id: &'a str,
    pub author: &'a UserBrief,
    pub authors: &'a [UserBrief],
    pub prompt: &'a str,
    pub delivery: Delivery,
}

impl AgentManager {
    #[must_use]
    pub fn new(pool: SqlitePool, events: EventStore, projection: Projection) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel::<String>();
        let inner = Arc::new(Inner {
            pool,
            events,
            projection,
            sessions: Mutex::new(HashMap::new()),
            starts: crate::locks::KeyedLocks::new(),
            queue_tx: queue_tx.clone(),
        });
        queue_worker::spawn(inner.clone(), queue_rx);
        Self { inner }
    }

    pub async fn restart(&self, thread_id: &str) -> Result<(), ServerError> {
        let session = self.inner.sessions.lock().await.remove(thread_id);
        if let Some(session) = session {
            let mut harness = session.harness.lock().await;
            let _ = harness.shutdown().await;
        }
        threads::transition(
            &self.inner.pool,
            &self.inner.projection,
            thread_id,
            ThreadAction::Restart,
        )
        .await?;
        self.ensure_started(thread_id).await
    }

    pub fn kick_queue(&self, thread_id: &str) {
        let _ = self.inner.queue_tx.send(thread_id.to_owned());
    }

    /// Start the harness once. Concurrent callers for the same thread wait on
    /// a per-thread start lock instead of holding the session map.
    pub async fn ensure_started(&self, thread_id: &str) -> Result<(), ServerError> {
        if self.inner.session(thread_id).await.is_some() {
            return Ok(());
        }
        let _start = self.inner.starts.lock(thread_id).await;
        if self.inner.session(thread_id).await.is_some() {
            return Ok(());
        }
        self.start_session(thread_id).await
    }

    async fn start_session(&self, thread_id: &str) -> Result<(), ServerError> {
        let row = threads::load_row(&self.inner.pool, thread_id).await?;
        let binding = bindings::load(&self.inner.pool, thread_id)
            .await?
            .ok_or_else(|| ServerError::internal("agent binding is missing"))?;
        let profile = find_profile(&binding.agent).copied().unwrap_or_else(|| {
            all_profiles()
                .first()
                .copied()
                .expect("at least one agent profile")
        });
        let mut harness = Box::new(AcpHarness::new(profile));
        let events = harness.events();
        let ctx = LaunchCtx {
            thread_id: thread_id.to_owned(),
            cwd: PathBuf::from(&binding.cwd),
            agent: binding.agent.clone(),
            model: row.model.clone(),
            effort: row.effort.clone(),
            subagents: true,
            env: adapter_env(&binding.agent),
        };
        let capabilities = match harness.start(ctx).await {
            Ok(capabilities) => capabilities,
            Err(error) => {
                lifecycle::mark_failed(
                    &self.inner.pool,
                    &self.inner.projection,
                    &self.inner.events,
                    thread_id,
                    &error.to_string(),
                )
                .await?;
                return Err(ServerError::internal(format!(
                    "agent failed to start: {error}"
                )));
            }
        };
        let mut reset_context = false;
        let session_id = match binding.session_id.as_deref() {
            Some(saved) => match harness.load_session(&SessionId(saved.to_owned())).await {
                Ok(()) => Some(SessionId(saved.to_owned())),
                Err(error) => {
                    tracing::warn!(thread_id, session_id = saved, %error, "could not resume agent session");
                    reset_context = true;
                    new_session(&mut *harness, &binding.cwd).await?
                }
            },
            None => new_session(&mut *harness, &binding.cwd).await?,
        };
        if let Some(session_id) = session_id.as_ref() {
            bindings::set_session(&self.inner.pool, thread_id, session_id.0.as_str()).await?;
            bindings::set_status(&self.inner.pool, thread_id, "idle", None).await?;
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
                    EventMeta::default(),
                )
                .await;
        }
        let run = Arc::new(Mutex::new(RunState::default()));
        let session = Arc::new(Session::new(harness, capabilities, run.clone()));
        pump::spawn(
            self.inner.clone(),
            thread_id.to_owned(),
            binding.agent.clone(),
            run,
            events,
        );
        self.inner
            .sessions
            .lock()
            .await
            .insert(thread_id.to_owned(), session);
        if threads::status(&self.inner.pool, thread_id).await.ok() == Some(ThreadStatus::Preparing)
        {
            threads::transition(
                &self.inner.pool,
                &self.inner.projection,
                thread_id,
                ThreadAction::StartSucceeded,
            )
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
        let author = user_brief(user);
        self.send_with_authors(thread_id, user, text, delivery, &[author])
            .await
    }

    /// Send a message preserving the full list of draft-block authors.
    pub async fn send_with_authors(
        &self,
        thread_id: &str,
        user: &CurrentUser,
        text: &str,
        delivery: Option<DeliveryMode>,
        authors: &[UserBrief],
    ) -> Result<SendMessageResponse, ServerError> {
        self.ensure_started(thread_id).await?;
        let session = self.session(thread_id).await?;
        let status = threads::status(&self.inner.pool, thread_id).await?;
        let running = session
            .observe_running(status == ThreadStatus::Running)
            .await;
        let action = resolve_delivery(
            delivery.unwrap_or(DeliveryMode::Auto),
            DeliveryContext {
                turn_running: running,
                steering_supported: session.capabilities.steering,
            },
        );
        match action {
            SendAction::Queue => self.queue(thread_id, user, text, authors).await,
            SendAction::Inject => {
                let outcome = {
                    let mut harness = session.harness.lock().await;
                    harness.steer(vec![text.to_owned()]).await
                }
                .map_err(mapping::agent_error)?;
                match outcome {
                    SteeringOutcome::Injected | SteeringOutcome::StartedNewTurn => {
                        let author = user_brief(user);
                        let turn_id = self.inner.current_turn(thread_id).await.unwrap_or_default();
                        let message =
                            mapping::user_message(thread_id, &author, text, Some(turn_id), authors);
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
                        self.queue(thread_id, user, text, authors).await
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
                if let Some(turn) = session.take_turn().await {
                    let _ = session.harness.lock().await.cancel(&turn).await;
                }
                self.start_turn(
                    &session,
                    TurnStart {
                        thread_id,
                        author: &user_brief(user),
                        authors,
                        prompt: text,
                        delivery: Delivery::StoppedAndSent,
                    },
                    user,
                    text,
                    authors,
                )
                .await
            }
            SendAction::Prompt => {
                self.start_turn(
                    &session,
                    TurnStart {
                        thread_id,
                        author: &user_brief(user),
                        authors,
                        prompt: text,
                        delivery: Delivery::Injected,
                    },
                    user,
                    text,
                    authors,
                )
                .await
            }
        }
    }

    async fn queue(
        &self,
        thread_id: &str,
        user: &CurrentUser,
        text: &str,
        authors: &[UserBrief],
    ) -> Result<SendMessageResponse, ServerError> {
        let position = enqueue(&self.inner.pool, thread_id, user, text, authors).await?;
        let _ = queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
        Ok(SendMessageResponse {
            delivery: Delivery::Queued,
            queue_position: Some(position),
            message_id: None,
        })
    }

    async fn start_turn(
        &self,
        session: &Session,
        start: TurnStart<'_>,
        user: &CurrentUser,
        text: &str,
        authors: &[UserBrief],
    ) -> Result<SendMessageResponse, ServerError> {
        let delivery = start.delivery;
        let thread_id = start.thread_id;
        let Some(claim) = session.claim_start().await else {
            return self.queue(thread_id, user, text, authors).await;
        };
        let (_, message_id) = begin_turn(&self.inner, claim, start).await?;
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
        let harness = session.harness.lock().await;
        Ok(harness.config_options())
    }

    pub async fn set_config(
        &self,
        thread_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let mut harness = session.harness.lock().await;
        harness
            .set_config(model, effort)
            .await
            .map_err(mapping::agent_error)
    }

    pub async fn cancel(&self, thread_id: &str) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        let turn = session.take_turn_for_cancel().await;
        if let Some(turn) = &turn {
            session
                .harness
                .lock()
                .await
                .cancel(turn)
                .await
                .map_err(mapping::agent_error)?;
        }
        queue::supersede(
            &self.inner.pool,
            &self.inner.projection,
            thread_id,
            turn.as_ref().map(|turn| turn.turn_id.as_str()),
        )
        .await?;
        cancel_pending_approvals(&self.inner, thread_id).await;
        cancel_pending_elicitations(&self.inner, thread_id).await;
        let _ = queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
        let _ = threads::transition(
            &self.inner.pool,
            &self.inner.projection,
            thread_id,
            ThreadAction::Cancel,
        )
        .await;
        Ok(())
    }

    pub async fn decide(
        &self,
        thread_id: &str,
        approval_id: &str,
        decision: SharedDecision,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        // Approval decisions contend only on harness IPC, never on turn
        // preparation or the session map.
        {
            let mut harness = session.harness.lock().await;
            harness
                .decide(approval_id, decision)
                .await
                .map_err(mapping::agent_error)?;
        }
        let turn_active = self.inner.current_turn(thread_id).await.is_some();
        let remaining = approvals::count_pending_items(&self.inner.pool, thread_id).await?;
        threads::transition(
            &self.inner.pool,
            &self.inner.projection,
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
        let mapped = match action {
            threadbase_shared::ElicitationAction::Accept => ElicitationAction::Accept,
            threadbase_shared::ElicitationAction::Decline => ElicitationAction::Decline,
            threadbase_shared::ElicitationAction::Cancel => ElicitationAction::Cancel,
        };
        // Like approvals, elicitation responses take only the harness lock.
        {
            let mut harness = session.harness.lock().await;
            harness
                .respond_elicitation(elicitation_id, mapped, values)
                .await
                .map_err(mapping::agent_error)?;
        }
        let turn_active = self.inner.current_turn(thread_id).await.is_some();
        let remaining = approvals::count_pending_items(&self.inner.pool, thread_id).await?;
        threads::transition(
            &self.inner.pool,
            &self.inner.projection,
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
        let status = subagents::status_of(&self.inner.pool, subagent_id).await?;
        if SubagentMachine::is_terminal(status) || !SubagentMachine::can_cancel(status) {
            return Err(ServerError(threadbase_shared::ApiError::conflict(
                "subagent can no longer be cancelled",
            )));
        }
        let session = self.session(thread_id).await?;
        let mut harness = session.harness.lock().await;
        harness
            .cancel_subagent(subagent_id)
            .await
            .map_err(mapping::agent_error)
    }

    async fn session(&self, thread_id: &str) -> Result<Arc<Session>, ServerError> {
        self.inner
            .session(thread_id)
            .await
            .ok_or_else(|| ServerError::internal("agent is not running"))
    }
}

impl Inner {
    pub(crate) async fn session(&self, thread_id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().await.get(thread_id).cloned()
    }

    /// Clear the in-memory turn handle so a concurrent `send` observes an idle
    /// session before the completion events are published.
    pub(crate) async fn clear_turn(&self, thread_id: &str) {
        if let Some(session) = self.session(thread_id).await {
            session.clear_turn().await;
        }
    }

    pub(crate) async fn current_turn(&self, thread_id: &str) -> Option<TurnId> {
        sqlx::query_scalar::<_, Option<String>>("SELECT current_turn_id FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .flatten()
            .map(TurnId::from)
    }
}

/// Persist the start of a turn, then ask the harness to run it. Every input
/// path (direct send, stop-and-send, queued) goes through this function so the
/// event log and the in-memory run state cannot disagree. The caller holds the
/// turn-start claim so preparation can run without the turn or harness lock.
pub(crate) async fn begin_turn(
    inner: &Inner,
    claim: TurnStartClaim<'_>,
    start: TurnStart<'_>,
) -> Result<(TurnHandle, MessageId), ServerError> {
    let session = claim.session();
    match prepare_and_prompt(inner, session, start).await {
        Ok(started) => Ok(started),
        Err(error) => {
            claim.release().await;
            Err(error)
        }
    }
}

async fn prepare_and_prompt(
    inner: &Inner,
    session: &Session,
    start: TurnStart<'_>,
) -> Result<(TurnHandle, MessageId), ServerError> {
    // Git, title and event preparation happens before any harness lock.
    lifecycle::auto_title(&inner.pool, &inner.events, start.thread_id, start.prompt).await;
    snapshot_worktree(inner, start.thread_id).await;
    let turn_id = TurnId::new();
    let message_id = MessageId::new();
    inner
        .events
        .append(
            &ThreadId::from(start.thread_id.to_owned()),
            Actor::user(start.author),
            ThreadEvent::TurnStarted {
                turn_id: turn_id.clone(),
                message_id: message_id.clone(),
                author: start.author.clone(),
                authors: start.authors.to_vec(),
                prompt: start.prompt.to_owned(),
            },
            EventMeta {
                delivery: Some(start.delivery),
                ..Default::default()
            },
        )
        .await?;
    sqlx::query("UPDATE threads SET current_turn_id = ?, updated_at = ? WHERE id = ?")
        .bind(turn_id.as_str())
        .bind(now_ms())
        .bind(start.thread_id)
        .execute(&inner.pool)
        .await?;
    let turn_id_for_error = turn_id.clone();
    {
        let mut run = session.run.lock().await;
        run.reset();
        run.turn_id = Some(turn_id);
    }
    let mut parts = vec![start.prompt.to_owned()];
    if let Some(summary) = inner.projection.summary(start.thread_id).await {
        parts.push(summary);
    }
    // Mark the thread Running before the prompt is issued: any completion event
    // from the harness then necessarily arrives after the Prompt transition.
    threads::transition(
        &inner.pool,
        &inner.projection,
        start.thread_id,
        ThreadAction::Prompt,
    )
    .await?;
    // Harness IPC only. The turn lock stays free while the prompt is issued.
    let prompt_result = {
        let mut harness = session.harness.lock().await;
        harness.prompt(parts).await
    };
    let handle = match prompt_result {
        Ok(handle) => handle,
        Err(error) => {
            session.run.lock().await.reset();
            let _ = sqlx::query("UPDATE threads SET current_turn_id = NULL WHERE id = ?")
                .bind(start.thread_id)
                .execute(&inner.pool)
                .await;
            let _ = inner
                .events
                .append(
                    &ThreadId::from(start.thread_id.to_owned()),
                    Actor::agent("Threadbase"),
                    ThreadEvent::TurnError {
                        turn_id: Some(turn_id_for_error),
                        message: error.to_string(),
                    },
                    EventMeta::default(),
                )
                .await;
            let _ = threads::transition(
                &inner.pool,
                &inner.projection,
                start.thread_id,
                ThreadAction::TurnError,
            )
            .await;
            return Err(mapping::agent_error(error));
        }
    };
    let cancel_requested = match session.install_turn(handle.clone()).await {
        TurnInstall::Running { cancel_requested } => cancel_requested,
        TurnInstall::AlreadyFinished => {
            return Ok((handle, message_id));
        }
    };
    if cancel_requested {
        let _ = session.harness.lock().await.cancel(&handle).await;
    }
    Ok((handle, message_id))
}

async fn snapshot_worktree(inner: &Inner, thread_id: &str) {
    let Some(worktree) = threads::load_row(&inner.pool, thread_id)
        .await
        .ok()
        .and_then(|row| row.worktree_path)
    else {
        return;
    };
    let Ok(snapshot) =
        git::snapshot_worktree(std::path::Path::new(&worktree), "Threadbase turn snapshot").await
    else {
        return;
    };
    let _ = sqlx::query("UPDATE threads SET last_turn_sha = ?, updated_at = ? WHERE id = ?")
        .bind(snapshot)
        .bind(now_ms())
        .bind(thread_id)
        .execute(&inner.pool)
        .await;
}

async fn enqueue(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
    text: &str,
    authors: &[UserBrief],
) -> Result<u32, ServerError> {
    let authors_json = serde_json::to_string(authors).unwrap_or_else(|_| "[]".to_owned());
    queue::enqueue(
        pool,
        queue::NewQueuedInput {
            thread_id,
            author_id: user.user.id.as_str(),
            text,
            authors_json: Some(&authors_json),
            created_at: now_ms(),
        },
    )
    .await
}

async fn new_session(
    harness: &mut dyn Harness,
    cwd: &str,
) -> Result<Option<SessionId>, ServerError> {
    harness
        .new_session(NewSession {
            cwd: PathBuf::from(cwd),
            mcp_servers: Vec::new(),
        })
        .await
        .map(Some)
        .map_err(|error| ServerError::internal(format!("agent session failed: {error}")))
}

async fn cancel_pending_approvals(inner: &Inner, thread_id: &str) {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM approvals WHERE thread_id = ? AND status = 'pending'")
            .bind(thread_id)
            .fetch_all(&inner.pool)
            .await
            .unwrap_or_default();
    for id in ids {
        let _ = approvals::transition(
            &inner.pool,
            &inner.projection,
            thread_id,
            &id,
            ApprovalAction::Cancel,
        )
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
    let _ = elicitations::cancel_all(&inner.pool, &inner.projection, thread_id).await;
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
                EventMeta::default(),
            )
            .await;
    }
}

fn adapter_env(agent: &str) -> Vec<(String, String)> {
    if agent != "codex" {
        return Vec::new();
    }
    threadbase_agent::profile::find_in_path("codex")
        .map(|path| vec![("CODEX_PATH".to_owned(), path.to_string_lossy().to_string())])
        .unwrap_or_default()
}
