//! Side effects that accompany a thread's lifecycle: rewriting history,
//! automatic titles and error reporting.

use super::{cancel_pending_approvals, cancel_pending_elicitations, user_brief, AgentManager};
use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::EventMeta;
use crate::projection::Projection;
use crate::{bindings, queue, threads};
use sqlx::SqlitePool;
use std::collections::HashSet;
use threadbase_core::state::ThreadAction;
use threadbase_shared::api::SendMessageResponse;
use threadbase_shared::{
    now_ms, Actor, ApiError, CommentAnchor, EventEnvelope, ThreadEvent, ThreadId, ThreadStatus,
};

/// Replace one user prompt and discard everything that followed it.
///
/// The adapter keeps its own conversation state, so a rewind cannot resume the
/// old ACP session: the next prompt starts a fresh one. All view state after
/// the edited message (events, queued input, attention items, anchored
/// comments, subagent rows) is removed together with the tail so the transcript
/// and the agent cannot disagree about what happened.
pub(crate) async fn rewind_and_edit(
    manager: &AgentManager,
    thread_id: &str,
    message_id: &str,
    text: &str,
    user: &CurrentUser,
) -> Result<SendMessageResponse, ServerError> {
    let inner = &manager.inner;
    if crate::setup::busy(&inner.pool, thread_id).await {
        return Err(ServerError(ApiError::conflict(
            "worktree setup is still running; wait for it to finish",
        )));
    }
    let status = threads::status(&inner.pool, thread_id).await?;
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
    let turn = inner
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
    // Without this a worker that already read the old queue head could start
    // an adapter between our shutdown and the truncation, then send the
    // message the rewind is supposed to discard.
    let start_guard = inner.starts.lock(thread_id).await;

    // Stop the current adapter before touching its log. `cancel` also
    // supersedes queued input and clears pending approvals/questions; the
    // unconditional pass below covers an adapter that was already gone.
    if matches!(status, ThreadStatus::Running | ThreadStatus::RequiresAction) {
        if inner.session(thread_id).await.is_some() {
            manager.cancel(thread_id).await?;
        } else {
            let _ = threads::transition(
                &inner.pool,
                &inner.projection,
                thread_id,
                ThreadAction::Cancel,
            )
            .await;
        }
    }
    let pump_done = inner
        .session(thread_id)
        .await
        .map(|session| session.pump_done.clone());
    manager.shutdown_session(thread_id).await;
    if let Some(done) = pump_done {
        // A late completion event from the old adapter must not land after the
        // cut. The timeout keeps a wedged adapter from blocking edit.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), done.notified()).await;
    }
    queue::supersede(&inner.pool, &inner.projection, thread_id, None).await?;
    cancel_pending_approvals(inner, thread_id).await;
    cancel_pending_elicitations(inner, thread_id).await;

    let removed = inner
        .events
        .truncate_from(&ThreadId::from(thread_id.to_owned()), cutoff)
        .await?;
    prune_rewound_comments(&inner.pool, thread_id, &removed).await;
    sqlx::query(
        "DELETE FROM subagents WHERE thread_id = ? AND (created_at >= ? OR updated_at >= ?)",
    )
    .bind(thread_id)
    .bind(turn.ts)
    .bind(turn.ts)
    .execute(&inner.pool)
    .await?;
    sqlx::query("UPDATE threads SET current_turn_id = NULL, updated_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(thread_id)
        .execute(&inner.pool)
        .await?;
    // Notifications about turns and comments that no longer exist would
    // otherwise send people back to a message that is gone.
    let inbox_users: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM inbox WHERE thread_id = ? AND created_at >= ?",
    )
    .bind(thread_id)
    .bind(turn.ts)
    .fetch_all(&inner.pool)
    .await
    .unwrap_or_default();
    sqlx::query("DELETE FROM inbox WHERE thread_id = ? AND created_at >= ?")
        .bind(thread_id)
        .bind(turn.ts)
        .execute(&inner.pool)
        .await?;
    for user_id in inbox_users {
        crate::inbox::publish_count(&inner.pool, &inner.projection, &user_id).await;
    }
    // The old ACP session contains the discarded tail. Clearing the id is what
    // makes `ensure_started` create a new conversation instead of resuming it.
    bindings::clear_session(&inner.pool, thread_id).await?;
    crate::search::reindex_thread(&inner.pool, thread_id).await?;
    inner.projection.publish_rewound(thread_id);
    inner.projection.publish(thread_id).await;
    // The rewind superseded queued input, so subscribers need the new slot
    // list; the events before the cut still describe the old queue.
    let _ = crate::queue::publish(&inner.pool, &inner.events, thread_id).await;
    drop(start_guard);

    let author = user_brief(user);
    manager
        .send_with_authors(thread_id, user, text, None, &[author], &attachments)
        .await
}

/// Drop comments whose anchor no longer exists after a rewind. File and diff
/// anchors survive: they describe the worktree, not a discarded turn.
async fn prune_rewound_comments(pool: &SqlitePool, thread_id: &str, removed: &[EventEnvelope]) {
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

/// Title a still-unnamed thread from the first line of the prompt.
pub async fn auto_title(
    pool: &SqlitePool,
    events: &crate::events::EventStore,
    thread_id: &str,
    text: &str,
) {
    let title: Option<String> = sqlx::query_scalar("SELECT title FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
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
    if sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
        .bind(&next)
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await
        .is_err()
    {
        return;
    }
    let _ = events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            ThreadEvent::ThreadUpdated {
                title: Some(next),
                status: None,
                pinned: None,
            },
            EventMeta::default(),
        )
        .await;
}

/// Mark a thread and its binding as failed, then record the reason as an event.
pub async fn mark_failed(
    pool: &SqlitePool,
    projection: &Projection,
    events: &crate::events::EventStore,
    thread_id: &str,
    message: &str,
) -> Result<(), ServerError> {
    let _ =
        crate::threads::transition(pool, projection, thread_id, ThreadAction::AgentFailed).await;
    let _ = events
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
    let summary: String = message.chars().take(240).collect();
    let _ = crate::notify::notify_watchers(
        pool,
        projection,
        thread_id,
        "error",
        None,
        None,
        &format!("The agent failed to start: {summary}"),
        None,
    )
    .await;
    Ok(())
}
