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
    placement: String,
    pinned: i64,
    snooze_until: Option<i64>,
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
        let placement =
            threadbase_core::state::parse_thread_placement(&row.placement).ok_or_else(|| {
                ServerError::internal(format!("unknown thread placement: {}", row.placement))
            })?;
        let queued_inputs = count(row.queued_inputs);
        let active_subagents = count(row.active_subagents);
        Ok(ThreadState {
            thread_id: ThreadId::from(thread_id.to_owned()),
            revision: self.revision.load(Ordering::SeqCst),
            allowed_actions: threadbase_core::state::thread_commands(status),
            status,
            current_turn: row.current_turn_id,
            pending_approvals: count(row.pending_approvals),
            pending_elicitations: count(row.pending_elicitations),
            queued_inputs,
            active_subagents,
            placement,
            section: threadbase_core::state::section(
                status,
                placement,
                row.pinned != 0,
                threadbase_core::state::is_working(status, queued_inputs, active_subagents),
            ),
            snooze_until: row.snooze_until,
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

    /// Tell subscribed clients that the event tail was deleted and replaced.
    /// They must drop their local log instead of merging the new branch into
    /// the old one.
    pub fn publish_rewound(&self, thread_id: &str) {
        let _ = self.tx.send(ServerWsMessage::ThreadRewound {
            thread_id: ThreadId::from(thread_id.to_owned()),
        });
    }

    /// Push one user's unread count. Sockets filter by user id, so a single
    /// broadcast channel can carry per-user badges without extra plumbing.
    pub fn publish_inbox_count(&self, user_id: &str, unread: u32) {
        let _ = self.tx.send(ServerWsMessage::InboxCount {
            user_id: threadbase_shared::UserId::from(user_id),
            unread,
        });
    }

    /// One sentence about this thread, added to the prompt so the model knows
    /// what else is waiting. `None` when nothing is waiting.
    pub async fn summary(&self, thread_id: &str) -> Option<String> {
        let state = self.read(thread_id).await.ok()?;
        summary_line(
            state.status,
            state.pending_approvals,
            state.pending_elicitations,
            state.queued_inputs,
            state.active_subagents,
        )
    }

    async fn row(&self, thread_id: &str) -> Result<ProjectionRow, ServerError> {
        let row = sqlx::query_as::<_, ProjectionRow>(
            "SELECT t.status, t.current_turn_id, t.placement, t.pinned, t.snooze_until,
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

/// The sentence `summary()` returns. Example:
/// `This thread is running. Also: 1 approval waiting for a person, 2 messages queued.`
fn summary_line(
    status: ThreadStatus,
    approvals: u32,
    questions: u32,
    queued: u32,
    subagents: u32,
) -> Option<String> {
    let mut waiting = Vec::new();
    push_count(
        &mut waiting,
        approvals,
        "approval waiting for a person",
        "approvals waiting for a person",
    );
    push_count(
        &mut waiting,
        questions,
        "question waiting for an answer",
        "questions waiting for an answer",
    );
    push_count(&mut waiting, queued, "message queued", "messages queued");
    push_count(
        &mut waiting,
        subagents,
        "subagent running",
        "subagents running",
    );
    if waiting.is_empty() {
        return None;
    }
    let status = match status {
        ThreadStatus::RequiresAction => "waiting for a person",
        other => thread_status_str(other),
    };
    Some(format!(
        "This thread is {status}. Also: {}.",
        waiting.join(", ")
    ))
}

/// `2 messages queued`: count plus the matching noun.
fn push_count(out: &mut Vec<String>, value: u32, one: &str, many: &str) {
    match value {
        0 => {}
        1 => out.push(format!("1 {one}")),
        n => out.push(format!("{n} {many}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reads_like_a_sentence() {
        assert_eq!(summary_line(ThreadStatus::Idle, 0, 0, 0, 0), None);
        assert_eq!(
            summary_line(ThreadStatus::Running, 1, 0, 2, 0).unwrap(),
            "This thread is running. Also: 1 approval waiting for a person, 2 messages queued."
        );
        assert_eq!(
            summary_line(ThreadStatus::RequiresAction, 0, 1, 0, 1).unwrap(),
            "This thread is waiting for a person. Also: 1 question waiting for an answer, 1 subagent running."
        );
    }
}
