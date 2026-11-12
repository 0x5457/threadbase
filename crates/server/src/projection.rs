//! Thread-state projection: the single read model pushed to WebSocket clients.
//!
//! Lifecycle transitions are owned by the module responsible for the entity
//! (threads, approvals, queue, …). Every one of them ends by calling
//! [`Projection::publish`], so subscribers always observe one coherent,
//! monotonically revised snapshot.

use crate::error::ServerError;
use crate::locks::KeyedLocks;
use sqlx::{FromRow, SqlitePool};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use threadbase_core::state::{parse_thread_status, thread_status_str};
use threadbase_shared::api::{ServerWsMessage, ThreadState};
use threadbase_shared::{ThreadId, ThreadStatus};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct Projection {
    pool: SqlitePool,
    tx: broadcast::Sender<ServerWsMessage>,
    revision: Arc<AtomicU64>,
    snapshot_locks: KeyedLocks,
}

#[derive(FromRow)]
struct ProjectionRow {
    status: String,
    current_turn_id: Option<String>,
    pending_approvals: i64,
    pending_elicitations: i64,
    queued_inputs: i64,
    active_subagents: i64,
}

impl Projection {
    #[must_use]
    pub fn new(pool: SqlitePool, tx: broadcast::Sender<ServerWsMessage>) -> Self {
        Self {
            pool,
            tx,
            revision: Arc::new(AtomicU64::new(0)),
            snapshot_locks: KeyedLocks::new(),
        }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerWsMessage> {
        self.tx.subscribe()
    }

    /// Read the current projection without allocating a revision. Used for
    /// one-off reads (agent summaries, REST state) where ordering is irrelevant.
    pub async fn read(&self, thread_id: &str) -> Result<ThreadState, ServerError> {
        let row = self.row(thread_id).await?;
        let status = parse_thread_status(&row.status).ok_or_else(|| {
            ServerError::internal(format!("unknown thread status: {}", row.status))
        })?;
        Ok(ThreadState {
            thread_id: ThreadId::from(thread_id.to_owned()),
            revision: self.revision.load(Ordering::SeqCst),
            status,
            current_turn: row.current_turn_id,
            pending_approvals: count(row.pending_approvals),
            pending_elicitations: count(row.pending_elicitations),
            queued_inputs: count(row.queued_inputs),
            active_subagents: count(row.active_subagents),
        })
    }

    /// Read with a fresh, ordered revision. Concurrent snapshots for the same
    /// thread are serialized so subscribers cannot observe regressions.
    pub async fn snapshot(&self, thread_id: &str) -> Result<ThreadState, ServerError> {
        let _guard = self.snapshot_locks.lock(thread_id).await;
        let mut state = self.read(thread_id).await?;
        state.revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(state)
    }

    /// Broadcast the current state. Projection failures are logged and never
    /// surface to the caller: the persisted transition already succeeded.
    pub async fn publish(&self, thread_id: &str) {
        let Ok(state) = self.snapshot(thread_id).await else {
            tracing::warn!(thread_id, "thread state projection failed");
            return;
        };
        let _ = self.tx.send(ServerWsMessage::ThreadState { state });
    }

    /// Compact semantic summary injected at prompt start.
    pub async fn summary(&self, thread_id: &str) -> Option<String> {
        let state = self.read(thread_id).await.ok()?;
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

    async fn row(&self, thread_id: &str) -> Result<ProjectionRow, ServerError> {
        let row = sqlx::query_as::<_, ProjectionRow>(
            "SELECT t.status, t.current_turn_id,
                (SELECT COUNT(*) FROM approvals a
                  WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals,
                (SELECT COUNT(*) FROM elicitations e
                  WHERE e.thread_id = t.id AND e.status = 'pending') AS pending_elicitations,
                (SELECT COUNT(*) FROM queued_inputs q
                  WHERE q.thread_id = t.id AND q.status = 'queued') AS queued_inputs,
                (SELECT COUNT(*) FROM subagents s
                  WHERE s.thread_id = t.id AND s.status IN ('running','requires_action')) AS active_subagents
             FROM threads t WHERE t.id = ?",
        )
        .bind(thread_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            ServerError(threadbase_shared::ApiError::not_found("thread not found"))
        })?;
        Ok(row)
    }
}

fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

/// Status helper shared by the transition writers.
pub async fn thread_status(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<ThreadStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("thread not found"))?;
    parse_thread_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown thread status: {value}")))
}
