//! Side effects that accompany a thread's lifecycle: automatic titles and
//! error reporting.

use crate::error::ServerError;
use crate::events::EventMeta;
use crate::projection::Projection;
use sqlx::SqlitePool;
use threadbase_core::state::ThreadAction;
use threadbase_shared::{now_ms, Actor, ThreadEvent, ThreadId};

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
    let _ = crate::bindings::set_status(pool, thread_id, "error", Some(message)).await;
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
    Ok(())
}
