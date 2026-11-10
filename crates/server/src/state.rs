//! Shared server state and the single writer for lifecycle state.

use crate::agents::AgentManager;
use crate::config::Config;
use crate::drafts::DraftHub;
use crate::error::ServerError;
use crate::events::EventStore;
use crate::presence::PresenceHub;
use crate::rate_limit::RateLimiter;
use sqlx::SqlitePool;
use std::sync::Arc;
use threadbase_core::state::{
    approval_decision_str, approval_status_str, comment_status_str, elicitation_status_str,
    parse_approval_status, parse_comment_status, parse_elicitation_status, parse_queue_status,
    parse_subagent_status, parse_thread_status, queue_status_str, subagent_status_str,
    thread_status_str, ApprovalAction, ApprovalMachine, CommentAction, CommentMachine,
    ElicitationAction, ElicitationMachine, QueueAction, QueueMachine, SubagentAction,
    SubagentMachine, ThreadAction, ThreadMachine, INITIAL_BINDING_STATUS,
};
use threadbase_shared::api::{ServerWsMessage, ThreadState};
use threadbase_shared::{
    now_ms, ApiError, ApprovalDecision, ApprovalStatus, CommentStatus, ElicitationStatus,
    QueueStatus, SubagentStatus, ThreadId, ThreadStatus,
};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub events: EventStore,
    pub state_tx: broadcast::Sender<ServerWsMessage>,
    pub state_store: StateStore,
    pub started_at: i64,
    pub login_limiter: RateLimiter,
    pub presence: PresenceHub,
    pub drafts: DraftHub,
    pub agents: AgentManager,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let events = EventStore::new(pool.clone());
        let (state_tx, _) = broadcast::channel::<ServerWsMessage>(256);
        let state_store = StateStore::new(pool.clone(), state_tx.clone());
        Self {
            config: Arc::new(config),
            drafts: DraftHub::new(pool.clone()),
            agents: AgentManager::new(pool.clone(), events.clone(), state_store.clone()),
            events,
            state_tx,
            state_store,
            pool,
            started_at: now_ms(),
            login_limiter: RateLimiter::new(),
            presence: PresenceHub::new(),
        }
    }
}

/// Single writer for every lifecycle status column.
///
/// Each transition is validated by the pure machines in `threadbase_core::state`
/// and persisted here, then the shared [`ThreadState`] projection is broadcast
/// to WebSocket subscribers. See `docs/08` §3.
#[derive(Clone)]
pub struct StateStore {
    pool: SqlitePool,
    state_tx: broadcast::Sender<ServerWsMessage>,
}

impl StateStore {
    pub fn new(pool: SqlitePool, state_tx: broadcast::Sender<ServerWsMessage>) -> Self {
        Self { pool, state_tx }
    }

    // ----- thread -----

    pub async fn thread_status(&self, thread_id: &str) -> Result<ThreadStatus, ServerError> {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(&self.pool)
            .await?;
        let value = value.ok_or_else(|| ServerError::internal("thread not found"))?;
        parse_thread_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown thread status: {value}")))
    }

    pub async fn transition_thread(
        &self,
        thread_id: &str,
        action: ThreadAction,
    ) -> Result<ThreadStatus, ServerError> {
        let current = self.thread_status(thread_id).await?;
        let next = ThreadMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        sqlx::query("UPDATE threads SET status = ?, updated_at = ? WHERE id = ?")
            .bind(thread_status_str(next))
            .bind(now_ms())
            .bind(thread_id)
            .execute(&self.pool)
            .await?;
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    // ----- shared projection -----

    /// The single read model consumed by the web UI, the REST state endpoint and
    /// the agent summary.
    pub async fn projection(&self, thread_id: &str) -> Result<ThreadState, ServerError> {
        let status = self.thread_status(thread_id).await?;
        let current_turn: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM events WHERE thread_id = ? AND kind = 'turn_started'
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(thread_id)
        .fetch_optional(&self.pool)
        .await?
        .and_then(|json: String| serde_json::from_str::<threadbase_shared::ThreadEvent>(&json).ok())
        .and_then(|event| match event {
            threadbase_shared::ThreadEvent::TurnStarted { turn_id, .. } => Some(turn_id.0),
            _ => None,
        });
        let pending_approvals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;
        let pending_elicitations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM elicitations WHERE thread_id = ? AND status = 'pending'",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;
        let queued_inputs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;
        let active_subagents: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM subagents WHERE thread_id = ? AND status IN ('running','requires_action')",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;
        let row = crate::threads::load_row(&self.pool, thread_id).await?;
        let diverged = crate::threads::is_diverged(&self.pool, &row).await;
        Ok(ThreadState {
            thread_id: ThreadId::from(thread_id.to_owned()),
            status,
            current_turn,
            pending_approvals: u32::try_from(pending_approvals).unwrap_or(0),
            pending_elicitations: u32::try_from(pending_elicitations).unwrap_or(0),
            queued_inputs: u32::try_from(queued_inputs).unwrap_or(0),
            active_subagents: u32::try_from(active_subagents).unwrap_or(0),
            diverged,
        })
    }

    pub async fn publish_thread_state(&self, thread_id: &str) {
        let Ok(state) = self.projection(thread_id).await else {
            return;
        };
        let message = ServerWsMessage::ThreadState { state };
        let _ = self.state_tx.send(message);
    }

    /// Compact semantic state summary for the agent at prompt start, derived
    /// from the same projection the UI consumes.
    ///
    /// Generated server-side; it does not depend on adapter capabilities, so
    /// adapters that do not report subagents/elicitations still get an accurate
    /// summary.
    pub async fn summary(&self, thread_id: &str) -> Option<String> {
        let state = self.projection(thread_id).await.ok()?;
        if state.pending_approvals == 0
            && state.pending_elicitations == 0
            && state.queued_inputs == 0
            && state.active_subagents == 0
        {
            return None;
        }
        Some(format!(
            "[Thread state] status={}; current_turn={}; pending_approvals={}; pending_elicitations={}; queued_inputs={}; active_subagents={}",
            thread_status_str(state.status),
            state.current_turn.as_deref().unwrap_or("-"),
            state.pending_approvals,
            state.pending_elicitations,
            state.queued_inputs,
            state.active_subagents
        ))
    }

    // ----- approval -----

    pub async fn approval_status(&self, approval_id: &str) -> Result<ApprovalStatus, ServerError> {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM approvals WHERE id = ?")
            .bind(approval_id)
            .fetch_optional(&self.pool)
            .await?;
        let value = value.ok_or_else(|| ServerError::internal("approval not found"))?;
        parse_approval_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown approval status: {value}")))
    }

    pub async fn set_approval_status(
        &self,
        thread_id: &str,
        approval_id: &str,
        action: ApprovalAction,
    ) -> Result<ApprovalStatus, ServerError> {
        let current = self.approval_status(approval_id).await?;
        let next = ApprovalMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        let updated = sqlx::query("UPDATE approvals SET status = ? WHERE id = ? AND status = ?")
            .bind(approval_status_str(next))
            .bind(approval_id)
            .bind(approval_status_str(current))
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(ServerError::internal("approval changed concurrently"));
        }
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn decide_approval(
        &self,
        thread_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        scope: Option<&str>,
        decided_by: &str,
        decided_at: i64,
        note: Option<&str>,
    ) -> Result<ApprovalStatus, ServerError> {
        let action = match decision {
            ApprovalDecision::AllowOnce
            | ApprovalDecision::AllowSession
            | ApprovalDecision::AllowThread => ApprovalAction::Allow,
            ApprovalDecision::Deny => ApprovalAction::Deny,
        };
        let current = self.approval_status(approval_id).await?;
        let next = ApprovalMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        let updated = sqlx::query(
            "UPDATE approvals SET status = ?, decision = ?, scope = ?, decided_by = ?, decided_at = ?,
                note = ? WHERE id = ? AND status = ?",
        )
        .bind(approval_status_str(next))
        .bind(approval_decision_str(decision))
        .bind(scope)
        .bind(decided_by)
        .bind(decided_at)
        .bind(note)
        .bind(approval_id)
        .bind(approval_status_str(current))
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(ServerError::internal("approval changed concurrently"));
        }
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    pub async fn auto_allow_approval(
        &self,
        thread_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        scope: &str,
        decided_by: &str,
        decided_at: i64,
    ) -> Result<ApprovalStatus, ServerError> {
        let current = self.approval_status(approval_id).await?;
        if current != ApprovalStatus::Pending {
            // Lost a race with a concurrent user decision; keep it idempotent.
            return Ok(current);
        }
        self.decide_approval(
            thread_id,
            approval_id,
            decision,
            Some(scope),
            decided_by,
            decided_at,
            Some("matched standing rule"),
        )
        .await
    }

    // ----- elicitation -----

    pub async fn elicitation_status(
        &self,
        elicitation_id: &str,
    ) -> Result<ElicitationStatus, ServerError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT status FROM elicitations WHERE id = ?")
                .bind(elicitation_id)
                .fetch_optional(&self.pool)
                .await?;
        let value = value.ok_or_else(|| ServerError::internal("elicitation not found"))?;
        parse_elicitation_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown elicitation status: {value}")))
    }

    pub async fn set_elicitation_status(
        &self,
        thread_id: &str,
        elicitation_id: &str,
        action: ElicitationAction,
    ) -> Result<ElicitationStatus, ServerError> {
        let current = self.elicitation_status(elicitation_id).await?;
        let next = ElicitationMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        let updated = sqlx::query("UPDATE elicitations SET status = ? WHERE id = ? AND status = ?")
            .bind(elicitation_status_str(next))
            .bind(elicitation_id)
            .bind(elicitation_status_str(current))
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(ServerError::internal("elicitation changed concurrently"));
        }
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    pub async fn respond_elicitation(
        &self,
        thread_id: &str,
        elicitation_id: &str,
        action: ElicitationAction,
        response: Option<&serde_json::Value>,
        responded_by: &str,
        responded_at: i64,
    ) -> Result<ElicitationStatus, ServerError> {
        let current = self.elicitation_status(elicitation_id).await?;
        let next = ElicitationMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        let response_json = response.map(|value| value.to_string());
        let updated = sqlx::query(
            "UPDATE elicitations SET status = ?, response_json = ?, responded_by = ?, responded_at = ?
             WHERE id = ? AND status = ?",
        )
        .bind(elicitation_status_str(next))
        .bind(response_json)
        .bind(responded_by)
        .bind(responded_at)
        .bind(elicitation_id)
        .bind(elicitation_status_str(current))
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(ServerError::internal("elicitation changed concurrently"));
        }
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    pub async fn cancel_elicitations(&self, thread_id: &str) -> Result<u32, ServerError> {
        let next =
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Cancel)
                .map_err(|error| ServerError::internal(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE elicitations SET status = ?, responded_at = ?
             WHERE thread_id = ? AND status = 'pending'",
        )
        .bind(elicitation_status_str(next))
        .bind(now_ms())
        .bind(thread_id)
        .execute(&self.pool)
        .await?;
        self.publish_thread_state(thread_id).await;
        Ok(u32::try_from(result.rows_affected()).unwrap_or(0))
    }

    // ----- queue -----

    pub async fn queue_status(&self, item_id: &str) -> Result<QueueStatus, ServerError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT status FROM queued_inputs WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&self.pool)
                .await?;
        let value = value.ok_or_else(|| ServerError::internal("queue item not found"))?;
        parse_queue_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown queue status: {value}")))
    }

    pub async fn dispatch_queue_item(
        &self,
        thread_id: &str,
        item_id: &str,
    ) -> Result<QueueStatus, ServerError> {
        self.set_queue_status(thread_id, item_id, QueueAction::Dispatch, None)
            .await
    }

    pub async fn complete_queue_item(
        &self,
        thread_id: &str,
        item_id: &str,
        turn_id: Option<&str>,
    ) -> Result<QueueStatus, ServerError> {
        self.set_queue_status(thread_id, item_id, QueueAction::PromptSucceeded, turn_id)
            .await
    }

    pub async fn fail_queue_item(
        &self,
        thread_id: &str,
        item_id: &str,
    ) -> Result<QueueStatus, ServerError> {
        self.set_queue_status(thread_id, item_id, QueueAction::PromptFailed, None)
            .await
    }

    pub async fn drop_queue_item(
        &self,
        thread_id: &str,
        item_id: &str,
    ) -> Result<QueueStatus, ServerError> {
        let current = self.queue_status(item_id).await?;
        if current != QueueStatus::Queued {
            return Err(ServerError(ApiError::not_found(
                "queued message not found or already sent",
            )));
        }
        self.set_queue_status(thread_id, item_id, QueueAction::Delete, None)
            .await
    }

    pub async fn retry_queue_item(
        &self,
        thread_id: &str,
        item_id: &str,
    ) -> Result<QueueStatus, ServerError> {
        let current = self.queue_status(item_id).await?;
        if QueueMachine::is_terminal(current) {
            return Err(ServerError(ApiError::not_found(
                "queued message not found or already sent",
            )));
        }
        // Retry is an explicit machine transition (superseded/queued -> queued).
        let next = QueueMachine::transition(current, &QueueAction::Retry)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "UPDATE queued_inputs SET status = ?, superseded = 0, updated_at = ?
             WHERE id = ? AND status IN ('queued','superseded')",
        )
        .bind(queue_status_str(next))
        .bind(now_ms())
        .bind(item_id)
        .execute(&self.pool)
        .await?;
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    async fn set_queue_status(
        &self,
        thread_id: &str,
        item_id: &str,
        action: QueueAction,
        turn_id: Option<&str>,
    ) -> Result<QueueStatus, ServerError> {
        let current = self.queue_status(item_id).await?;
        let next = QueueMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        let updated = sqlx::query(
            "UPDATE queued_inputs SET status = ?, turn_id = ?, updated_at = ?
             WHERE id = ? AND status = ?",
        )
        .bind(queue_status_str(next))
        .bind(turn_id)
        .bind(now_ms())
        .bind(item_id)
        .bind(queue_status_str(current))
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(ServerError::internal("queue item changed concurrently"));
        }
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    pub async fn supersede_queue(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
    ) -> Result<u32, ServerError> {
        let next = QueueMachine::transition(QueueStatus::Queued, &QueueAction::Supersede)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE queued_inputs SET status = ?, superseded = 1, turn_id = ?, updated_at = ?
             WHERE thread_id = ? AND status = 'queued'",
        )
        .bind(queue_status_str(next))
        .bind(turn_id)
        .bind(now_ms())
        .bind(thread_id)
        .execute(&self.pool)
        .await?;
        self.publish_thread_state(thread_id).await;
        Ok(u32::try_from(result.rows_affected()).unwrap_or(0))
    }

    // ----- comment -----

    pub async fn comment_status(&self, comment_id: &str) -> Result<CommentStatus, ServerError> {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
            .bind(comment_id)
            .fetch_optional(&self.pool)
            .await?;
        let value = value.ok_or_else(|| ServerError::internal("comment not found"))?;
        parse_comment_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown comment status: {value}")))
    }

    pub async fn set_comment_status(
        &self,
        thread_id: &str,
        comment_id: &str,
        action: CommentAction,
    ) -> Result<CommentStatus, ServerError> {
        let current = self.comment_status(comment_id).await?;
        let next = CommentMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        sqlx::query("UPDATE comments SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
            .bind(comment_status_str(next))
            .bind(now_ms())
            .bind(comment_id)
            .bind(comment_status_str(current))
            .execute(&self.pool)
            .await?;
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    pub async fn mark_comment_delivered(
        &self,
        thread_id: &str,
        comment_id: &str,
        delivered_seq: Option<u64>,
    ) -> Result<CommentStatus, ServerError> {
        let current = self.comment_status(comment_id).await?;
        let next = CommentMachine::transition(current, &CommentAction::Deliver)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        sqlx::query(
            "UPDATE comments SET status = ?, delivered_seq = ?, updated_at = ? WHERE id = ? AND status = ?",
        )
        .bind(comment_status_str(next))
        .bind(delivered_seq.and_then(|value| i64::try_from(value).ok()))
        .bind(now_ms())
        .bind(comment_id)
        .bind(comment_status_str(current))
        .execute(&self.pool)
        .await?;
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    // ----- subagent -----

    pub async fn subagent_status(
        &self,
        _thread_id: &str,
        subagent_id: &str,
    ) -> Result<SubagentStatus, ServerError> {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
            .bind(subagent_id)
            .fetch_optional(&self.pool)
            .await?;
        let value = value.ok_or_else(|| ServerError::internal("subagent not found"))?;
        parse_subagent_status(&value)
            .ok_or_else(|| ServerError::internal(format!("unknown subagent status: {value}")))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_subagent(
        &self,
        thread_id: &str,
        id: &str,
        parent_id: Option<&str>,
        title: &str,
        description: Option<&str>,
        cwd: Option<&str>,
        can_cancel: bool,
        usage_json: Option<&str>,
        action: SubagentAction,
    ) -> Result<SubagentStatus, ServerError> {
        // The creation state is validated by the machine (Create -> Idle).
        let created = SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Create)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO subagents(id, thread_id, parent_id, title, description, status, cwd, can_cancel, usage_json, last_seq, created_at, updated_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description,
                cwd = excluded.cwd, can_cancel = excluded.can_cancel,
                usage_json = excluded.usage_json, updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(thread_id)
        .bind(parent_id)
        .bind(title)
        .bind(description)
        .bind(subagent_status_str(created))
        .bind(cwd)
        .bind(i64::from(can_cancel))
        .bind(usage_json)
        .bind(now_ms())
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        self.set_subagent_status(thread_id, id, action).await
    }

    pub async fn set_subagent_status(
        &self,
        thread_id: &str,
        subagent_id: &str,
        action: SubagentAction,
    ) -> Result<SubagentStatus, ServerError> {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
            .bind(subagent_id)
            .fetch_optional(&self.pool)
            .await?;
        let current = match value {
            Some(value) => parse_subagent_status(&value).ok_or_else(|| {
                ServerError::internal(format!("unknown subagent status: {value}"))
            })?,
            None => SubagentStatus::Idle,
        };
        let next = SubagentMachine::transition(current, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        if next == current {
            return Ok(next);
        }
        sqlx::query("UPDATE subagents SET status = ?, updated_at = ? WHERE id = ?")
            .bind(subagent_status_str(next))
            .bind(now_ms())
            .bind(subagent_id)
            .execute(&self.pool)
            .await?;
        self.publish_thread_state(thread_id).await;
        Ok(next)
    }

    // ----- agent binding (diagnostic, non-authoritative) -----

    pub async fn set_binding_status(
        &self,
        thread_id: &str,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<(), ServerError> {
        sqlx::query(
            "UPDATE agent_bindings SET status = ?, last_error = ?, updated_at = ? WHERE thread_id = ?",
        )
        .bind(status)
        .bind(last_error)
        .bind(now_ms())
        .bind(thread_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ----- creation inserts (initial state validated by the machines) -----

    pub async fn insert_thread(&self, row: &NewThread<'_>) -> Result<ThreadStatus, ServerError> {
        let status = ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::Create)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO threads(id, project_id, parent_thread_id, title, status, agent, model, effort,
                branch, base_branch, base_sha, last_turn_sha, worktree_path, created_by, created_at, updated_at, share_level)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id)
        .bind(row.project_id)
        .bind(row.parent_thread_id)
        .bind(row.title)
        .bind(thread_status_str(status))
        .bind(row.agent)
        .bind(row.model)
        .bind(row.effort)
        .bind(row.branch)
        .bind(row.base_branch)
        .bind(row.base_sha)
        .bind(row.base_sha)
        .bind(row.worktree_path)
        .bind(row.created_by)
        .bind(row.created_at)
        .bind(row.created_at)
        .bind(row.share_level)
        .execute(&self.pool)
        .await?;
        Ok(status)
    }

    pub async fn insert_approval(
        &self,
        approval: &crate::approvals::NewApproval,
        paths_json: &str,
        options_json: &str,
        input_json: Option<&str>,
        created_at: i64,
    ) -> Result<ApprovalStatus, ServerError> {
        let status = ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO approvals(id, thread_id, subagent_id, turn_id, tool_call_id, agent, title, kind,
                command, cwd, paths_json, options_json, input_json, status, created_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&approval.id)
        .bind(&approval.thread_id)
        .bind(approval.subagent_id.as_deref())
        .bind(approval.turn_id.as_deref())
        .bind(approval.tool_call_id.as_deref())
        .bind(&approval.agent)
        .bind(&approval.title)
        .bind(&approval.kind)
        .bind(approval.command.as_deref())
        .bind(approval.cwd.as_deref())
        .bind(paths_json)
        .bind(options_json)
        .bind(input_json)
        .bind(approval_status_str(status))
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(status)
    }

    pub async fn insert_elicitation(
        &self,
        elicitation: &threadbase_shared::ElicitationDto,
        created_at: i64,
    ) -> Result<ElicitationStatus, ServerError> {
        let status =
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Create)
                .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO elicitations(id, thread_id, subagent_id, tool_call_id, agent, message, mode,
                schema_json, blocking, status, created_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(elicitation.id.as_str())
        .bind(elicitation.thread_id.as_str())
        .bind(elicitation.subagent_id.as_ref().map(|value| value.as_str()))
        .bind(elicitation.tool_call_id.as_ref().map(|value| value.as_str()))
        .bind(&elicitation.agent)
        .bind(&elicitation.message)
        .bind(match elicitation.mode {
            threadbase_shared::ElicitationMode::Form => "form",
            threadbase_shared::ElicitationMode::Url => "url",
        })
        .bind(serde_json::to_string(&elicitation.requested_schema).unwrap_or_else(|_| "{}".into()))
        .bind(i64::from(elicitation.blocking))
        .bind(elicitation_status_str(status))
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(status)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_comment(
        &self,
        id: &str,
        thread_id: &str,
        parent_id: Option<&str>,
        author_id: &str,
        body: &str,
        anchor_kind: &str,
        anchor_json: &str,
        status: CommentStatus,
        created_at: i64,
    ) -> Result<CommentStatus, ServerError> {
        let action = match status {
            CommentStatus::Pending => CommentAction::CreatePending,
            CommentStatus::Delivered => CommentAction::CreateDelivered,
            _ => {
                return Err(ServerError::internal(
                    "cannot create a comment in this state",
                ))
            }
        };
        let next = CommentMachine::transition(CommentStatus::Pending, &action)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO comments(id, thread_id, parent_id, author_id, body, anchor_kind, anchor_json,
                anchor_changed, status, created_at, updated_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(id)
        .bind(thread_id)
        .bind(parent_id)
        .bind(author_id)
        .bind(body)
        .bind(anchor_kind)
        .bind(anchor_json)
        .bind(comment_status_str(next))
        .bind(created_at)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(next)
    }

    pub async fn enqueue(
        &self,
        id: &str,
        thread_id: &str,
        author_id: &str,
        text: &str,
        position: i64,
        created_at: i64,
    ) -> Result<QueueStatus, ServerError> {
        let status = QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue)
            .map_err(|error| ServerError::internal(error.to_string()))?;
        sqlx::query(
            "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(thread_id)
        .bind(author_id)
        .bind(text)
        .bind(queue_status_str(status))
        .bind(position)
        .bind(created_at)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(status)
    }

    pub async fn insert_binding(
        &self,
        thread_id: &str,
        agent: &str,
        cwd: &str,
        updated_at: i64,
    ) -> Result<(), ServerError> {
        sqlx::query(
            "INSERT INTO agent_bindings(thread_id, agent, cwd, status, updated_at)
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(thread_id)
        .bind(agent)
        .bind(cwd)
        .bind(INITIAL_BINDING_STATUS)
        .bind(updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Copy the agent binding from a source thread (used by fork).
    pub async fn copy_binding(
        &self,
        new_thread_id: &str,
        cwd: &str,
        source_thread_id: &str,
        updated_at: i64,
    ) -> Result<(), ServerError> {
        sqlx::query(
            "INSERT INTO agent_bindings(thread_id, agent, cwd, status, updated_at)
             SELECT ?, agent, ?, ?, ? FROM agent_bindings WHERE thread_id = ?",
        )
        .bind(new_thread_id)
        .bind(cwd)
        .bind(INITIAL_BINDING_STATUS)
        .bind(updated_at)
        .bind(source_thread_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Row data for [`StateStore::insert_thread`].
pub struct NewThread<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub parent_thread_id: Option<&'a str>,
    pub title: &'a str,
    pub agent: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub branch: &'a str,
    pub base_branch: &'a str,
    pub base_sha: &'a str,
    pub worktree_path: &'a str,
    pub created_by: &'a str,
    pub created_at: i64,
    pub share_level: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;

    fn store(pool: &SqlitePool) -> StateStore {
        let (tx, _) = broadcast::channel(16);
        StateStore::new(pool.clone(), tx)
    }

    #[tokio::test]
    async fn transitions_follow_the_machine() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let state = store(&pool);
        let thread_id = thread.as_str();

        assert_eq!(
            state.thread_status(thread_id).await.unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            state
                .transition_thread(thread_id, ThreadAction::Prompt)
                .await
                .unwrap(),
            ThreadStatus::Running
        );
        assert_eq!(
            state
                .transition_thread(thread_id, ThreadAction::ApprovalPending)
                .await
                .unwrap(),
            ThreadStatus::RequiresAction
        );
        assert_eq!(
            state
                .transition_thread(
                    thread_id,
                    ThreadAction::ApprovalResolved {
                        turn_active: true,
                        remaining_pending: 0
                    }
                )
                .await
                .unwrap(),
            ThreadStatus::Running
        );
        assert_eq!(
            state
                .transition_thread(thread_id, ThreadAction::TurnEnded)
                .await
                .unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            state
                .transition_thread(thread_id, ThreadAction::Archive)
                .await
                .unwrap(),
            ThreadStatus::Archived
        );
        assert!(state
            .transition_thread(thread_id, ThreadAction::Prompt)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn publishes_thread_state_on_change() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let (tx, mut rx) = broadcast::channel(16);
        let state = StateStore::new(pool.clone(), tx);
        state
            .transition_thread(thread.as_str(), ThreadAction::Prompt)
            .await
            .unwrap();
        let message = rx.try_recv().unwrap();
        let ServerWsMessage::ThreadState { state } = message else {
            panic!("expected thread state message");
        };
        assert_eq!(state.status, ThreadStatus::Running);
        assert_eq!(state.pending_approvals, 0);
        assert_eq!(state.pending_elicitations, 0);
        assert_eq!(state.queued_inputs, 0);
        assert_eq!(state.active_subagents, 0);
    }

    #[tokio::test]
    async fn projection_reports_pending_state() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let state = store(&pool);
        sqlx::query(
            "INSERT INTO approvals(id, thread_id, agent, title, kind, paths_json, options_json, status, created_at)
             VALUES('a1', ?, 'codex', 'Write file', 'file_write', '[]', '[]', 'pending', 0)",
        )
        .bind(thread.as_str())
        .execute(&pool)
        .await
        .unwrap();
        let projection = state.projection(thread.as_str()).await.unwrap();
        assert_eq!(projection.status, ThreadStatus::Idle);
        assert_eq!(projection.pending_approvals, 1);
        let summary = state.summary(thread.as_str()).await.unwrap();
        assert!(summary.contains("status=idle"), "{summary}");
        assert!(summary.contains("pending_approvals=1"), "{summary}");
        assert!(summary.contains("pending_elicitations=0"), "{summary}");
        assert!(summary.contains("queued_inputs=0"), "{summary}");
        assert!(summary.contains("active_subagents=0"), "{summary}");
        assert!(summary.contains("current_turn=-"), "{summary}");
    }

    #[tokio::test]
    async fn approval_status_transitions_go_through_the_store() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let state = store(&pool);
        sqlx::query(
            "INSERT INTO approvals(id, thread_id, agent, title, kind, paths_json, options_json, status, created_at)
             VALUES('a1', ?, 'codex', 'Write file', 'file_write', '[]', '[]', 'pending', 0)",
        )
        .bind(thread.as_str())
        .execute(&pool)
        .await
        .unwrap();
        let next = state
            .set_approval_status(thread.as_str(), "a1", ApprovalAction::Allow)
            .await
            .unwrap();
        assert_eq!(next, ApprovalStatus::Allowed);
        assert_eq!(
            state.approval_status("a1").await.unwrap(),
            ApprovalStatus::Allowed
        );
        assert!(state
            .set_approval_status(thread.as_str(), "a1", ApprovalAction::Deny)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn queue_status_transitions_go_through_the_store() {
        let pool = db::connect_memory().await.unwrap();
        let (user_id, _, thread) = seed_thread(&pool).await;
        let state = store(&pool);
        sqlx::query(
            "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
             VALUES('q1', ?, ?, 'hello', 'queued', 1, 0, 0)",
        )
        .bind(thread.as_str())
        .bind(user_id.as_str())
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            state
                .dispatch_queue_item(thread.as_str(), "q1")
                .await
                .unwrap(),
            QueueStatus::Sending
        );
        assert_eq!(
            state
                .complete_queue_item(thread.as_str(), "q1", Some("turn-1"))
                .await
                .unwrap(),
            QueueStatus::Sent
        );
        assert_eq!(state.queue_status("q1").await.unwrap(), QueueStatus::Sent);
    }

    #[tokio::test]
    async fn unknown_status_is_an_error() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        sqlx::query("UPDATE threads SET status = 'nonsense' WHERE id = ?")
            .bind(thread.as_str())
            .execute(&pool)
            .await
            .unwrap();
        let state = store(&pool);
        assert!(state.thread_status(thread.as_str()).await.is_err());
    }
}
