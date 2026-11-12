//! Agent bindings: which adapter and worktree a thread runs in.
//!
//! The binding is diagnostic, not authoritative for lifecycle state; the
//! thread status machine remains the source of truth.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::INITIAL_BINDING_STATUS;
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
    sqlx::query(
        "INSERT INTO agent_bindings(thread_id, agent, cwd, status, updated_at)
         VALUES(?, ?, ?, ?, ?)",
    )
    .bind(thread_id)
    .bind(agent)
    .bind(cwd)
    .bind(INITIAL_BINDING_STATUS)
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
        "INSERT INTO agent_bindings(thread_id, agent, cwd, status, updated_at)
         SELECT ?, agent, ?, ?, ? FROM agent_bindings WHERE thread_id = ?",
    )
    .bind(new_thread_id)
    .bind(cwd)
    .bind(INITIAL_BINDING_STATUS)
    .bind(updated_at)
    .bind(source_thread_id)
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

pub async fn set_status(
    pool: &SqlitePool,
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
