//! Thread watchers and per-user mute state.
//!
//! Watching is additive and cheap: the creator starts as a watcher, and anyone
//! who sends a prompt, comments or decides an item is added automatically.
//! Notifications go to unmuted watchers, never to the whole workspace.

use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_core::state::WatchMachine;
use threadbase_shared::{now_ms, ApiError, ThreadWatchRequest, ThreadWatchResponse, WatchState};

/// Start watching without changing the mute flag.
pub async fn watch(pool: &SqlitePool, thread_id: &str, user_id: &str) -> Result<(), ServerError> {
    sqlx::query(
        "INSERT INTO thread_watchers(thread_id, user_id, muted, created_at)
         VALUES(?, ?, 0, ?)
         ON CONFLICT(thread_id, user_id) DO NOTHING",
    )
    .bind(thread_id)
    .bind(user_id)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

/// Copy the source thread's watchers into a fork, preserving mute choices.
pub async fn copy_for_fork(
    pool: &SqlitePool,
    new_thread_id: &str,
    source_thread_id: &str,
) -> Result<(), ServerError> {
    sqlx::query(
        "INSERT INTO thread_watchers(thread_id, user_id, muted, created_at)
         SELECT ?, user_id, muted, ? FROM thread_watchers WHERE thread_id = ?
         ON CONFLICT(thread_id, user_id) DO NOTHING",
    )
    .bind(new_thread_id)
    .bind(now_ms())
    .bind(source_thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn state(
    pool: &SqlitePool,
    thread_id: &str,
    user_id: &str,
) -> Result<ThreadWatchResponse, ServerError> {
    let muted: Option<i64> =
        sqlx::query_scalar("SELECT muted FROM thread_watchers WHERE thread_id = ? AND user_id = ?")
            .bind(thread_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(ThreadWatchResponse {
        state: WatchState::from_row(muted),
    })
}

pub async fn set(
    pool: &SqlitePool,
    thread_id: &str,
    user_id: &str,
    request: &ThreadWatchRequest,
) -> Result<ThreadWatchResponse, ServerError> {
    let current = state(pool, thread_id, user_id).await?.state;
    let action = WatchMachine::action_for(current, request.state)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    WatchMachine::transition(current, &action)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    match request.state.muted_value() {
        None => {
            sqlx::query("DELETE FROM thread_watchers WHERE thread_id = ? AND user_id = ?")
                .bind(thread_id)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
        Some(muted) => {
            // The row is the whole state: upserting is idempotent, so a retried
            // request cannot leave `watching` and `muted` disagreeing.
            sqlx::query(
                "INSERT INTO thread_watchers(thread_id, user_id, muted, created_at)
                 VALUES(?, ?, ?, ?)
                 ON CONFLICT(thread_id, user_id) DO UPDATE SET muted = excluded.muted",
            )
            .bind(thread_id)
            .bind(user_id)
            .bind(i64::from(muted))
            .bind(now_ms())
            .execute(pool)
            .await?;
        }
    }
    state(pool, thread_id, user_id).await
}

/// Unmuted watchers; a legacy thread without rows falls back to its creator.
pub async fn recipients(pool: &SqlitePool, thread_id: &str) -> Result<Vec<String>, ServerError> {
    let mut users: Vec<String> =
        sqlx::query_scalar("SELECT user_id FROM thread_watchers WHERE thread_id = ? AND muted = 0")
            .bind(thread_id)
            .fetch_all(pool)
            .await?;
    if users.is_empty() {
        // Fall back only for threads that predate watcher rows, never just
        // because every watcher muted the thread.
        let has_watchers: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM thread_watchers WHERE thread_id = ?)")
                .bind(thread_id)
                .fetch_one(pool)
                .await?;
        if !has_watchers {
            let creator: Option<String> =
                sqlx::query_scalar("SELECT created_by FROM threads WHERE id = ?")
                    .bind(thread_id)
                    .fetch_optional(pool)
                    .await?;
            users.extend(creator);
        }
    }
    Ok(users)
}
