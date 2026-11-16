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
