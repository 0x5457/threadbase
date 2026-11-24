//! Agent bindings: which adapter, worktree and ACP session a thread resumes.
//!
//! The row holds identity only. Lifecycle lives in `threads.status` and the
//! in-memory turn slot; there is deliberately no second status column here.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::now_ms;

#[derive(Clone, FromRow)]
pub struct Binding {
    pub agent: String,
    pub cwd: String,
    pub session_id: Option<String>,
}

pub async fn load(pool: &SqlitePool, thread_id: &str) -> Result<Option<Binding>, ServerError> {
    Ok(sqlx::query_as::<_, Binding>(
        "SELECT agent, cwd, session_id FROM agent_bindings WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn insert(
    pool: &SqlitePool,
    thread_id: &str,
    agent: &str,
    cwd: &str,
    updated_at: i64,
) -> Result<(), ServerError> {
    sqlx::query("INSERT INTO agent_bindings(thread_id, agent, cwd, updated_at) VALUES(?, ?, ?, ?)")
        .bind(thread_id)
        .bind(agent)
        .bind(cwd)
        .bind(updated_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Copy the agent identity from a source thread (used by fork).
pub async fn copy(
    pool: &SqlitePool,
    new_thread_id: &str,
    cwd: &str,
    source_thread_id: &str,
    updated_at: i64,
) -> Result<(), ServerError> {
    sqlx::query(
        "INSERT INTO agent_bindings(thread_id, agent, cwd, updated_at)
         SELECT ?, agent, ?, ? FROM agent_bindings WHERE thread_id = ?",
    )
    .bind(new_thread_id)
    .bind(cwd)
    .bind(updated_at)
    .bind(source_thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Completion time (Unix ms) of the newest native turn this thread has seen.
pub async fn native_synced_at(pool: &SqlitePool, thread_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT native_synced_at FROM agent_bindings WHERE thread_id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// The newest native turn this thread has imported or mirrored.
pub async fn native_turn_id(pool: &SqlitePool, thread_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT native_turn_id FROM agent_bindings WHERE thread_id = ?",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
}

pub async fn set_native_turn(
    pool: &SqlitePool,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), ServerError> {
    sqlx::query(
        "UPDATE agent_bindings SET native_turn_id = ?, native_synced_at = ? WHERE thread_id = ?",
    )
    .bind(turn_id)
    .bind(threadbase_shared::now_ms())
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Advance the native cursor and the timestamp fallback in one write.
pub async fn set_native_progress(
    pool: &SqlitePool,
    thread_id: &str,
    synced_at: i64,
    turn_id: Option<&str>,
) -> Result<(), ServerError> {
    sqlx::query(
        "UPDATE agent_bindings SET native_synced_at = ?, native_turn_id = ? WHERE thread_id = ?",
    )
    .bind(synced_at)
    .bind(turn_id)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_native_synced(
    pool: &SqlitePool,
    thread_id: &str,
    at: i64,
) -> Result<(), ServerError> {
    sqlx::query(
        "UPDATE agent_bindings SET native_synced_at = ? WHERE thread_id = ? AND native = 1",
    )
    .bind(at)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a thread as continuing the agent's own session, starting from the
/// completion time of what was just imported.
pub async fn mark_native(
    pool: &SqlitePool,
    thread_id: &str,
    synced_at: i64,
    turn_id: Option<&str>,
) -> Result<(), ServerError> {
    sqlx::query(
        "UPDATE agent_bindings
         SET native = 1, native_synced_at = ?, native_turn_id = ?, peer_active = 0
         WHERE thread_id = ?",
    )
    .bind(synced_at)
    .bind(turn_id)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_session(
    pool: &SqlitePool,
    thread_id: &str,
    session_id: &str,
) -> Result<(), ServerError> {
    sqlx::query("UPDATE agent_bindings SET session_id = ?, updated_at = ? WHERE thread_id = ?")
        .bind(session_id)
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Forget the ACP session so the next prompt starts a new one. A rewound
/// conversation cannot resume the old adapter context: it contains turns the
/// transcript no longer has.
pub async fn clear_session(pool: &SqlitePool, thread_id: &str) -> Result<(), ServerError> {
    sqlx::query("UPDATE agent_bindings SET session_id = NULL, updated_at = ? WHERE thread_id = ?")
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn agent(pool: &SqlitePool, thread_id: &str) -> Result<String, ServerError> {
    let agent: Option<String> =
        sqlx::query_scalar("SELECT agent FROM agent_bindings WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await?;
    agent.ok_or_else(|| ServerError::internal("agent binding is missing"))
}
