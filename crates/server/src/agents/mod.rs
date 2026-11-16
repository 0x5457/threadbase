//! Bridges persisted thread events to ACP harnesses and vice versa.
//!
//! Responsibilities are split by concern:
//! - this module owns sessions and the public [`AgentManager`] facade,
//! - [`pump`] turns harness events into persisted thread events,
//! - [`queue_worker`] drains the input queue,
//! - [`lifecycle`] and [`mapping`] hold the remaining side effects and
//!   conversions.

mod derived;
mod lifecycle;
mod mapping;
mod pump;
mod queue_worker;
mod session;

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::presence::PresenceHub;
use crate::projection::Projection;
use crate::{approvals, bindings, elicitations, queue, subagents, threads, watchers};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use threadbase_agent::{
    all_profiles, find_profile, AcpHarness, Harness, LaunchCtx, NewSession, PromptPart, SessionId,
    SteeringOutcome, TurnHandle,
};
use threadbase_core::state::{ApprovalAction, SubagentMachine, ThreadAction};
use threadbase_core::{resolve_delivery, DeliveryContext, SendAction};
use threadbase_shared::api::{DeliveryMode, SendMessageResponse};
use threadbase_shared::{
    now_ms, Actor, ApiError, ApprovalDecision as SharedDecision, AttachmentDto, CommentAnchor,
    Delivery, ElicitationAction, MessageId, ThreadEvent, ThreadId, ThreadStatus, TurnId, UserBrief,
};
use threadbase_workspace::git;
use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};

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
    /// Timestamps of automatic adapter restarts per thread, so a crashing
    /// adapter cannot spin forever.
    pub(crate) restarts: Mutex<HashMap<String, Vec<i64>>>,
    /// Live focus, used to suppress completion pings for people already here.
    pub(crate) presence: PresenceHub,
    /// Global cap on concurrently running turns. The permit lives inside the
    /// session's turn slot, so it is released exactly when the turn ends.
    pub(crate) turn_slots: Arc<Semaphore>,
    pub(crate) idle_session_ms: i64,
}

/// Everything needed to start one turn, whichever input path produced it.
pub(crate) struct TurnStart<'a> {
    pub thread_id: &'a str,
    pub author: &'a UserBrief,
    pub authors: &'a [UserBrief],
    pub prompt: &'a str,
    pub delivery: Delivery,
    pub attachments: &'a [AttachmentDto],
    /// Acquired before the session is started; held for the whole turn.
    pub permit: Option<OwnedSemaphorePermit>,
}

impl AgentManager {
    #[must_use]
    pub fn new(
        pool: SqlitePool,
        events: EventStore,
        projection: Projection,
        presence: PresenceHub,
        idle_session_seconds: i64,
        max_active_turns: usize,
    ) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel::<String>();
        let capacity = if max_active_turns == 0 {
            Semaphore::MAX_PERMITS
        } else {
            max_active_turns
        };
        let inner = Arc::new(Inner {
            pool,
            events,
            projection,
            sessions: Mutex::new(HashMap::new()),
            starts: crate::locks::KeyedLocks::new(),
            queue_tx: queue_tx.clone(),
            restarts: Mutex::new(HashMap::new()),
            presence,
            turn_slots: Arc::new(Semaphore::new(capacity)),
            idle_session_ms: idle_session_seconds.saturating_mul(1000),
        });
        let manager = Self { inner };
        queue_worker::spawn(manager.clone(), queue_rx);
        manager.spawn_idle_reaper();
        manager
    }

    /// Stop an idle adapter on purpose before deleting or archiving its thread.
    pub async fn stop(&self, thread_id: &str) {
        self.shutdown_session(thread_id).await;
    }

    /// Stop every live adapter before the process exits. The systemd unit uses
    /// `KillMode=process` so tmux servers survive a restart; that also means
    /// agent children must be reaped explicitly here.
    pub async fn shutdown_all(&self) {
        let ids: Vec<String> = {
            let sessions = self.inner.sessions.lock().await;
            sessions.keys().cloned().collect()
        };
        for thread_id in ids {
            self.shutdown_session(&thread_id).await;
        }
    }

    /// Whether this session's workspace holds a goal it keeps working on
    /// without prompts. Errors read as "no goal": never keep a session alive
    /// because a database read failed.
    async fn runs_autonomous_goal(&self, thread_id: &str) -> bool {
        match bindings::load(&self.inner.pool, thread_id).await {
            Ok(Some(binding)) => crate::goals::runs_autonomously(
                std::path::Path::new(&binding.cwd),
                binding.session_id.as_deref(),
            ),
            _ => false,
        }
    }

    /// Shut down adapters that have been idle past the configured TTL. ACP
    /// session ids live in the binding, so the next prompt transparently
    /// resumes the conversation.
    fn spawn_idle_reaper(&self) {
        if self.inner.idle_session_ms <= 0 {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            let period = std::time::Duration::from_secs(60);
            loop {
                tokio::time::sleep(period).await;
                manager.evict_idle_sessions().await;
            }
        });
    }

    async fn evict_idle_sessions(&self) {
        let sessions: Vec<(String, Arc<Session>)> = self
            .inner
            .sessions
            .lock()
            .await
            .iter()
            .map(|(id, session)| (id.clone(), session.clone()))
            .collect();
        for (thread_id, session) in sessions {
            if session.idle_ms() < self.inner.idle_session_ms {
                continue;
            }
            // Claiming the start slot is the same lock a sender needs, so a
            // session cannot be reaped while a prompt is being prepared.
            let Some(claim) = session.claim_start().await else {
                continue;
            };
            let busy = threads::status(&self.inner.pool, &thread_id)
                .await
                .map(|status| status != ThreadStatus::Idle)
                .unwrap_or(true)
                || approvals::count_pending_items(&self.inner.pool, &thread_id)
                    .await
                    .map(|count| count > 0)
                    .unwrap_or(true)
                || has_queued_input(&self.inner.pool, &thread_id).await
                || self.runs_autonomous_goal(&thread_id).await;
            if busy {
                claim.release().await;
                continue;
            }
            session.mark_stopped();
            claim.release().await;
            self.shutdown_session(&thread_id).await;
            tracing::info!(thread_id, "idle agent session stopped");
        }
    }

    /// Clean up after a restart. Threads whose agent processes died with the
    /// old process are reset to idle, prompts that can no longer be answered
    /// are cancelled, input queued behind the interrupted turn is handed back
    /// to the queue worker, and turns that were cut off are continued so a
    /// restart does not silently stop the agent.
    pub async fn recover_after_restart(&self) -> Result<(), ServerError> {
        let reconciled = threads::reconcile_after_restart(
            &self.inner.pool,
            &self.inner.events,
            &self.inner.projection,
        )
        .await?;
        // A row the old process left mid-dispatch must not strand input: on an
        // interrupted thread the continuation prompt covers it, elsewhere the
        // worker should run it again.
        let interrupted: std::collections::HashSet<String> = reconciled
            .iter()
            .filter(|(_, turn_id)| turn_id.is_some())
            .map(|(thread_id, _)| thread_id.clone())
            .collect();
        queue::recover_sending(&self.inner.pool, &self.inner.projection, &interrupted).await?;
        let mut drain: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (thread_id, turn_id) in &reconciled {
            cancel_pending_approvals(&self.inner, thread_id).await;
            cancel_pending_elicitations(&self.inner, thread_id).await;
            // Newer input already waiting takes precedence over the turn that
            // was cut off; it was meant to run after that turn anyway.
            if turn_id.is_some() && !has_queued_input(&self.inner.pool, thread_id).await {
                self.queue_resume(thread_id).await?;
            }
            drain.insert(thread_id.clone());
        }
        // Queued input is durable work: drain it even for threads that were
        // idle at shutdown, otherwise a restart strands the queue forever.
        let queued: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT thread_id FROM queued_inputs WHERE status = 'queued'",
        )
        .fetch_all(&self.inner.pool)
        .await?;
        drain.extend(queued);
        for thread_id in drain {
            if has_queued_input(&self.inner.pool, &thread_id).await {
                self.kick_queue(&thread_id);
            }
        }
        Ok(())
    }

    /// A restart must not silently stop work. When a turn was cut off and no
    /// newer input is queued, append a continuation prompt attributed to
    /// Threadbase; the resumed ACP session already has the full context, so the
    /// agent can pick up from the working tree as it is.
    async fn queue_resume(&self, thread_id: &str) -> Result<(), ServerError> {
        const PROMPT: &str = "The server restarted during the previous turn. \
            Continue where you left off; re-check the working tree before changing \
            files so you do not redo work that is already done.";
        let author_id: Option<String> =
            sqlx::query_scalar("SELECT created_by FROM threads WHERE id = ?")
                .bind(thread_id)
                .fetch_optional(&self.inner.pool)
                .await?;
        let Some(author_id) = author_id else {
            return Ok(());
        };
        let system = UserBrief {
            id: threadbase_shared::UserId::from("threadbase"),
            username: "threadbase".to_owned(),
            display_name: "Threadbase".to_owned(),
            avatar_url: None,
            color: threadbase_shared::user_color("threadbase").to_owned(),
        };
        let authors_json = serde_json::to_string(&[system]).ok();
        queue::enqueue(
            &self.inner.pool,
            queue::NewQueuedInput {
                thread_id,
                author_id: &author_id,
                text: PROMPT,
                authors_json: authors_json.as_deref(),
                attachments_json: None,
                created_at: now_ms(),
            },
        )
        .await?;
        let _ = queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
        Ok(())
    }

    /// Rebuild subagent rows from historical delegation tool calls.
    pub async fn backfill_derived_subagents(&self) -> Result<(), ServerError> {
        derived::backfill(&self.inner.pool, &self.inner.projection).await
    }

    pub async fn restart(&self, thread_id: &str) -> Result<(), ServerError> {
        if crate::setup::busy(&self.inner.pool, thread_id).await {
            return Err(ServerError(ApiError::conflict(
                "worktree setup is still running; wait for it to finish",
            )));
        }
        self.shutdown_session(thread_id).await;
        self.start_fresh(thread_id).await
    }

    /// Replace one user prompt and discard everything that followed it.
    ///
    /// The adapter keeps its own conversation state, so a rewind cannot resume
    /// the old ACP session: the next prompt starts a fresh one. All view state
    /// after the edited message (events, queued input, attention items,
    /// anchored comments, subagent rows) is removed together with the tail so
    /// the transcript and the agent cannot disagree about what happened.
    pub async fn rewind_and_edit(
        &self,
        thread_id: &str,
        message_id: &str,
        text: &str,
        user: &CurrentUser,
    ) -> Result<SendMessageResponse, ServerError> {
        if crate::setup::busy(&self.inner.pool, thread_id).await {
            return Err(ServerError(ApiError::conflict(
                "worktree setup is still running; wait for it to finish",
            )));
        }
        let status = threads::status(&self.inner.pool, thread_id).await?;
        match status {
            ThreadStatus::Archived => {
                return Err(ServerError(ApiError::conflict(
                    "this thread is archived; unarchive it before editing a message",
                )));
            }
            ThreadStatus::Preparing => {
                return Err(ServerError(ApiError::conflict(
                    "the agent is still starting; try again in a moment",
                )));
            }
            _ => {}
        }
        let turn = self
            .inner
            .events
            .turn_started(&ThreadId::from(thread_id.to_owned()), message_id)
            .await?
            .ok_or_else(|| {
                ServerError(ApiError::not_found(
                    "message not found or it is not a user prompt",
                ))
            })?;
        let ThreadEvent::TurnStarted { attachments, .. } = &turn.payload else {
            return Err(ServerError::internal("turn_started payload was not a turn"));
        };
        let attachments = attachments.clone();
        let cutoff = turn.seq;

        // Serialize against `ensure_started` (queue worker, concurrent sends).
        // Without this a worker that already read the old queue head could
        // start an adapter between our shutdown and the truncation, then send
        // the message the rewind is supposed to discard.
        let start_guard = self.inner.starts.lock(thread_id).await;

        // Stop the current adapter before touching its log. `cancel` also
        // supersedes queued input and clears pending approvals/questions; the
        // unconditional pass below covers an adapter that was already gone.
        if matches!(status, ThreadStatus::Running | ThreadStatus::RequiresAction) {
            if self.inner.session(thread_id).await.is_some() {
                self.cancel(thread_id).await?;
            } else {
                let _ = threads::transition(
                    &self.inner.pool,
                    &self.inner.projection,
                    thread_id,
                    ThreadAction::Cancel,
                )
                .await;
            }
        }
        let pump_done = self
            .inner
            .session(thread_id)
            .await
            .map(|session| session.pump_done.clone());
        self.shutdown_session(thread_id).await;
        if let Some(done) = pump_done {
            // A late completion event from the old adapter must not land after
            // the cut. The timeout keeps a wedged adapter from blocking edit.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), done.notified()).await;
        }
        queue::supersede(&self.inner.pool, &self.inner.projection, thread_id, None).await?;
        cancel_pending_approvals(&self.inner, thread_id).await;
        cancel_pending_elicitations(&self.inner, thread_id).await;

        let removed = self
            .inner
            .events
            .truncate_from(&ThreadId::from(thread_id.to_owned()), cutoff)
            .await?;
        prune_rewound_comments(&self.inner.pool, thread_id, &removed).await;
        sqlx::query(
            "DELETE FROM subagents WHERE thread_id = ? AND (created_at >= ? OR updated_at >= ?)",
        )
        .bind(thread_id)
        .bind(turn.ts)
        .bind(turn.ts)
        .execute(&self.inner.pool)
        .await?;
        sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(thread_id)
            .execute(&self.inner.pool)
            .await?;
        // Notifications about turns and comments that no longer exist would
        // otherwise send people back to a message that is gone.
        let inbox_users: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT user_id FROM inbox WHERE thread_id = ? AND created_at >= ?",
        )
        .bind(thread_id)
        .bind(turn.ts)
        .fetch_all(&self.inner.pool)
        .await
        .unwrap_or_default();
        sqlx::query("DELETE FROM inbox WHERE thread_id = ? AND created_at >= ?")
            .bind(thread_id)
            .bind(turn.ts)
            .execute(&self.inner.pool)
            .await?;
        for user_id in inbox_users {
            crate::inbox::publish_count(&self.inner.pool, &self.inner.projection, &user_id).await;
        }
        // The old ACP session contains the discarded tail. Clearing the id is
        // what makes `ensure_started` create a new conversation instead of
        // resuming it.
        bindings::clear_session(&self.inner.pool, thread_id).await?;
        crate::search::reindex_thread(&self.inner.pool, thread_id).await?;
        self.inner.projection.publish_rewound(thread_id);
        self.inner.projection.publish(thread_id).await;
        drop(start_guard);

        let author = user_brief(user);
        self.send_with_authors(thread_id, user, text, None, &[author], &attachments)
            .await
    }

    /// Stop the current adapter on purpose. The closing flag keeps the exit
    /// monitor from treating this as a crash and reviving the session.
    async fn shutdown_session(&self, thread_id: &str) {
        let session = self.inner.sessions.lock().await.remove(thread_id);
        if let Some(session) = session {
            session.mark_stopped();
            let mut harness = session.harness.lock().await;
            let _ = harness.shutdown().await;
        }
    }

    /// Transition through `preparing` into a fresh adapter and drain input
    /// that was waiting for one.
    async fn start_fresh(&self, thread_id: &str) -> Result<(), ServerError> {
        threads::transition(
            &self.inner.pool,
            &self.inner.projection,
            thread_id,
            ThreadAction::Restart,
        )
        .await?;
        // The interrupted turn is gone with the old adapter; a stale id would
        // make steering and the projection think a turn is still active.
        sqlx::query("UPDATE threads SET current_turn_id = NULL WHERE id = ?")
            .bind(thread_id)
            .execute(&self.inner.pool)
            .await?;
        self.ensure_started(thread_id).await?;
        if has_queued_input(&self.inner.pool, thread_id).await {
            self.kick_queue(thread_id);
        }
        Ok(())
    }

    /// Bring a thread back after its adapter exited without being asked to.
    /// The dead session has already been dropped, so `ensure_started` builds a
    /// new one; a turn that was cut off is continued by Threadbase itself.
    pub(crate) async fn recover_from_exit(&self, thread_id: &str, had_turn: bool) {
        if !self.allow_adapter_restart(thread_id).await {
            let _ = self
                .inner
                .events
                .append(
                    &ThreadId::from(thread_id.to_owned()),
                    Actor::system(),
                    ThreadEvent::SystemNotice {
                        level: "error".to_owned(),
                        message: "The agent process keeps exiting; Threadbase stopped restarting \
                                  it. Restart the thread to try again."
                            .to_owned(),
                    },
                    EventMeta::default(),
                )
                .await;
            let _ = threads::transition(
                &self.inner.pool,
                &self.inner.projection,
                thread_id,
                ThreadAction::AgentFailed,
            )
            .await;
            return;
        }
        if let Err(error) = self.start_fresh(thread_id).await {
            tracing::warn!(thread_id, %error, "could not restart agent after an unexpected exit");
            return;
        }
        // Newer queued input takes precedence; it was meant to run after the
        // interrupted turn. Otherwise the turn itself is continued.
        if had_turn && !has_queued_input(&self.inner.pool, thread_id).await {
            if let Err(error) = self.queue_resume(thread_id).await {
                tracing::warn!(thread_id, %error, "could not resume the interrupted turn");
            }
            self.kick_queue(thread_id);
        }
    }

    /// Bound automatic restarts so a crashing adapter cannot burn turns in a
    /// loop; a successful turn clears the window again.
    async fn allow_adapter_restart(&self, thread_id: &str) -> bool {
        const LIMIT: usize = 3;
        const WINDOW_MS: i64 = 5 * 60 * 1000;
        let now = now_ms();
        let mut restarts = self.inner.restarts.lock().await;
        let attempts = restarts.entry(thread_id.to_owned()).or_default();
        attempts.retain(|at| now - *at < WINDOW_MS);
        if attempts.len() >= LIMIT {
            return false;
        }
        attempts.push(now);
        true
    }

    pub fn kick_queue(&self, thread_id: &str) {
        let _ = self.inner.queue_tx.send(thread_id.to_owned());
    }

    /// Start the harness once. Concurrent callers for the same thread wait on
    /// a per-thread start lock instead of holding the session map.
    pub async fn ensure_started(&self, thread_id: &str) -> Result<(), ServerError> {
        if crate::setup::busy(&self.inner.pool, thread_id).await {
            return Err(ServerError(ApiError::conflict(
                "worktree setup is still running; wait for it to finish",
            )));
        }
        if let Some(session) = self.inner.session(thread_id).await {
            if !session.is_stopped() {
                return Ok(());
            }
            // The adapter is gone; a session inserted after the exit must not
            // keep the thread pinned to a dead process.
            self.inner.remove_session(thread_id, &session).await;
        }
        let _start = self.inner.starts.lock(thread_id).await;
        if let Some(session) = self.inner.session(thread_id).await {
            if !session.is_stopped() {
                return Ok(());
            }
            self.inner.remove_session(thread_id, &session).await;
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
            self.clone(),
            thread_id.to_owned(),
            binding.agent.clone(),
            run,
            events,
            Arc::downgrade(&session),
            session.pump_done.clone(),
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
        self.send_with_authors(thread_id, user, text, delivery, &[author], &[])
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
        attachments: &[AttachmentDto],
    ) -> Result<SendMessageResponse, ServerError> {
        watchers::watch(&self.inner.pool, thread_id, user.user.id.as_str()).await?;
        self.ensure_started(thread_id).await?;
        let session = self.session(thread_id).await?;
        if !attachments.is_empty() && !session.capabilities.images {
            return Err(ServerError(ApiError::validation(
                "this agent does not accept images",
            )));
        }
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
            SendAction::Queue => {
                self.queue(thread_id, user, text, authors, attachments)
                    .await
            }
            SendAction::Inject => {
                let mut parts = vec![PromptPart::text(text)];
                parts
                    .extend(crate::attachments::prompt_parts(&self.inner.pool, attachments).await?);
                let outcome = {
                    let mut harness = session.harness.lock().await;
                    harness.steer(parts).await
                }
                .map_err(mapping::agent_error)?;
                match outcome {
                    SteeringOutcome::Injected | SteeringOutcome::StartedNewTurn => {
                        let author = user_brief(user);
                        let turn_id = self.inner.current_turn(thread_id).await.unwrap_or_default();
                        let message = mapping::user_message(
                            thread_id,
                            &author,
                            text,
                            Some(turn_id),
                            authors,
                            attachments,
                        );
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
                        self.queue(thread_id, user, text, authors, attachments)
                            .await
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
                        attachments,
                        permit: None,
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
                        attachments,
                        permit: None,
                    },
                    user,
                    text,
                    authors,
                )
                .await
            }
        }
    }

    /// Deliver an already-queued input into the running turn. Unlike
    /// [`Self::send_with_authors`] this never enqueues: the caller owns the
    /// queue row, so a rejection surfaces as an error instead of a duplicate.
    pub async fn steer_queued(
        &self,
        thread_id: &str,
        author: &UserBrief,
        text: &str,
        authors: &[UserBrief],
        attachments: &[AttachmentDto],
    ) -> Result<(), ServerError> {
        let Some(session) = self.inner.session(thread_id).await else {
            return Err(ServerError(ApiError::conflict(
                "no turn is running; the message stays queued",
            )));
        };
        let status = threads::status(&self.inner.pool, thread_id).await?;
        let running = session
            .observe_running(status == ThreadStatus::Running)
            .await;
        if !running {
            return Err(ServerError(ApiError::conflict(
                "no turn is running; the message stays queued",
            )));
        }
        if !session.capabilities.steering {
            return Err(ServerError(ApiError::conflict(
                "this agent can't accept input mid-turn; the message will be sent when the current turn ends",
            )));
        }
        if !attachments.is_empty() && !session.capabilities.images {
            return Err(ServerError(ApiError::conflict(
                "this agent does not accept images; the message stays queued",
            )));
        }
        let mut parts = vec![PromptPart::text(text)];
        parts.extend(crate::attachments::prompt_parts(&self.inner.pool, attachments).await?);
        let outcome = {
            let mut harness = session.harness.lock().await;
            harness.steer(parts).await
        }
        .map_err(mapping::agent_error)?;
        match outcome {
            SteeringOutcome::Injected | SteeringOutcome::StartedNewTurn => {
                let turn_id = self.inner.current_turn(thread_id).await.unwrap_or_default();
                let message = mapping::user_message(
                    thread_id,
                    author,
                    text,
                    Some(turn_id),
                    authors,
                    attachments,
                );
                self.inner
                    .events
                    .append(
                        &ThreadId::from(thread_id.to_owned()),
                        Actor::user(author),
                        ThreadEvent::MessageCompleted { message },
                        EventMeta {
                            delivery: Some(Delivery::Injected),
                            ..Default::default()
                        },
                    )
                    .await?;
                Ok(())
            }
            SteeringOutcome::PromptRequired => Err(ServerError(ApiError::conflict(
                "the agent asked for a new turn; the message stays queued",
            ))),
            SteeringOutcome::Failed { reason } => {
                tracing::warn!(?reason, "steering a queued message failed");
                Err(ServerError(ApiError::conflict(
                    "the agent rejected the steering input",
                )))
            }
        }
    }

    async fn queue(
        &self,
        thread_id: &str,
        user: &CurrentUser,
        text: &str,
        authors: &[UserBrief],
        attachments: &[AttachmentDto],
    ) -> Result<SendMessageResponse, ServerError> {
        let attachments_json = if attachments.is_empty() {
            None
        } else {
            Some(serde_json::to_string(attachments).unwrap_or_default())
        };
        let position = enqueue(
            &self.inner.pool,
            thread_id,
            user,
            text,
            authors,
            attachments_json.as_deref(),
        )
        .await?;
        let _ = queue::publish(&self.inner.pool, &self.inner.events, thread_id).await;
        // A thread that is idle must not wait for a turn end that will never
        // come; the worker no-ops while a turn is running.
        self.kick_queue(thread_id);
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
            return self
                .queue(thread_id, user, text, authors, start.attachments)
                .await;
        };
        // A direct prompt waits behind the global turn cap by queuing, not by
        // holding the HTTP request open.
        let Some(permit) = self.inner.try_turn_permit() else {
            claim.release().await;
            return self
                .queue(thread_id, user, text, authors, start.attachments)
                .await;
        };
        let start = TurnStart {
            permit: Some(permit),
            ..start
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

    /// Apply model/effort to the live adapter and report whether one was
    /// there to receive it. An idle thread has no adapter: the stored value is
    /// the source of truth and `start_session` applies it, so a missing or
    /// stopped session is a no-op rather than an error.
    pub async fn set_config(
        &self,
        thread_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<bool, ServerError> {
        let Some(session) = self.inner.session(thread_id).await else {
            return Ok(false);
        };
        if session.is_stopped() {
            return Ok(false);
        }
        let mut harness = session.harness.lock().await;
        harness
            .set_config(model, effort)
            .await
            .map_err(mapping::agent_error)?;
        Ok(true)
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

    /// Stop the running turn without superseding queued input, so the next
    /// queued message still runs once the turn ends. Returns whether a turn
    /// had to be cancelled.
    pub async fn interrupt(&self, thread_id: &str) -> Result<bool, ServerError> {
        let Some(session) = self.inner.session(thread_id).await else {
            return Ok(false);
        };
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
        cancel_pending_approvals(&self.inner, thread_id).await;
        cancel_pending_elicitations(&self.inner, thread_id).await;
        let _ = threads::transition(
            &self.inner.pool,
            &self.inner.projection,
            thread_id,
            ThreadAction::Cancel,
        )
        .await;
        Ok(turn.is_some())
    }

    pub async fn decide(
        &self,
        thread_id: &str,
        approval_id: &str,
        decision: SharedDecision,
        option_id: Option<&str>,
    ) -> Result<(), ServerError> {
        let session = self.session(thread_id).await?;
        // Approval decisions contend only on harness IPC, never on turn
        // preparation or the session map.
        {
            let mut harness = session.harness.lock().await;
            harness
                .decide(approval_id, decision, option_id)
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
    /// Non-blocking permit for a direct prompt; queue instead when full.
    pub(crate) fn try_turn_permit(&self) -> Option<OwnedSemaphorePermit> {
        self.turn_slots.clone().try_acquire_owned().ok()
    }

    /// Blocking permit for the queue worker; it resumes when a turn finishes.
    pub(crate) async fn acquire_turn_permit(&self) -> OwnedSemaphorePermit {
        self.turn_slots
            .clone()
            .acquire_owned()
            .await
            .expect("turn semaphore is never closed")
    }

    pub(crate) async fn session(&self, thread_id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().await.get(thread_id).cloned()
    }

    /// Drop a session only when it is still the current one; a restart may
    /// have installed a replacement already.
    pub(crate) async fn remove_session(&self, thread_id: &str, session: &Arc<Session>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(thread_id)
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            sessions.remove(thread_id);
        }
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
    // A rewind, archive or delete may have shut this session down after the
    // caller claimed a start. Never write a prompt for an adapter that is gone:
    // it would resurrect part of a turn after the log was truncated.
    if session.is_stopped() {
        return Err(ServerError(ApiError::conflict(
            "the agent session was stopped; try again",
        )));
    }
    // Git, title and event preparation happens before any harness lock.
    lifecycle::auto_title(&inner.pool, &inner.events, start.thread_id, start.prompt).await;
    snapshot_worktree(inner, start.thread_id).await;
    // Read the attached images up front: a missing file must fail before the
    // turn is recorded, not leave a turn with no prompt behind.
    let image_parts = crate::attachments::prompt_parts(&inner.pool, start.attachments).await?;
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
                attachments: start.attachments.to_vec(),
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
    let mut parts = vec![PromptPart::text(start.prompt)];
    parts.extend(image_parts);
    // Mark the thread Running before the prompt is issued: any completion event
    // from the harness then necessarily arrives after the Prompt transition.
    // Read the summary after this transition too: built before it, the text
    // said `status=idle` next to the turn id it had just written, which gave
    // the agent a contradictory picture of its own thread.
    threads::transition(
        &inner.pool,
        &inner.projection,
        start.thread_id,
        ThreadAction::Prompt,
    )
    .await?;
    if let Some(summary) = inner.projection.summary(start.thread_id).await {
        parts.push(PromptPart::text(summary));
    }
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
    let permit = start.permit;
    let cancel_requested = match session.install_turn(handle.clone(), permit).await {
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
        git::snapshot_worktree(std::path::Path::new(&worktree), git::TURN_SNAPSHOT_MESSAGE).await
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
    attachments_json: Option<&str>,
) -> Result<u32, ServerError> {
    let authors_json = serde_json::to_string(authors).unwrap_or_else(|_| "[]".to_owned());
    queue::enqueue(
        pool,
        queue::NewQueuedInput {
            thread_id,
            author_id: user.user.id.as_str(),
            text,
            authors_json: Some(&authors_json),
            attachments_json,
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

async fn has_queued_input(pool: &SqlitePool, thread_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await
    .unwrap_or_default();
    count > 0
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

/// Drop comments whose anchor no longer exists after a rewind. File and diff
/// anchors survive: they describe the worktree, not a discarded turn.
async fn prune_rewound_comments(
    pool: &SqlitePool,
    thread_id: &str,
    removed: &[threadbase_shared::EventEnvelope],
) {
    let mut messages = HashSet::new();
    let mut tools = HashSet::new();
    let mut elicitations = HashSet::new();
    for event in removed {
        match &event.payload {
            ThreadEvent::TurnStarted { message_id, .. }
            | ThreadEvent::MessageDelta { message_id, .. }
            | ThreadEvent::ThinkingDelta { message_id, .. } => {
                messages.insert(message_id.as_str().to_owned());
            }
            ThreadEvent::MessageCompleted { message } => {
                messages.insert(message.id.as_str().to_owned());
            }
            ThreadEvent::ToolCall { call } | ThreadEvent::ToolCallUpdate { call } => {
                tools.insert(call.id.as_str().to_owned());
            }
            ThreadEvent::ElicitationRequested { elicitation } => {
                elicitations.insert(elicitation.id.as_str().to_owned());
            }
            _ => {}
        }
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, anchor_json FROM comments
          WHERE thread_id = ? AND anchor_kind IN ('message', 'tool_call', 'elicitation')",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (id, json) in rows {
        let Ok(anchor) = serde_json::from_str::<CommentAnchor>(&json) else {
            continue;
        };
        let stale = match anchor {
            CommentAnchor::Message { message_id } => messages.contains(&message_id),
            CommentAnchor::ToolCall { tool_call_id } => tools.contains(&tool_call_id),
            CommentAnchor::Elicitation { elicitation_id } => elicitations.contains(&elicitation_id),
            _ => false,
        };
        if stale {
            let _ = sqlx::query("DELETE FROM comments WHERE id = ?")
                .bind(&id)
                .execute(pool)
                .await;
        }
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
