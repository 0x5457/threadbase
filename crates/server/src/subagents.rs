//! Subagent records reported by the agent harness.

use crate::error::ServerError;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{
    parse_subagent_status, subagent_status_str, SubagentAction, SubagentMachine,
};
use threadbase_shared::{now_ms, SubagentDto, SubagentStatus};

#[derive(FromRow)]
struct SubagentRow {
    id: String,
    thread_id: String,
    parent_id: Option<String>,
    title: String,
    description: Option<String>,
    status: String,
    cwd: Option<String>,
    can_cancel: i64,
    usage_json: Option<String>,
    last_seq: i64,
    created_at: i64,
    updated_at: i64,
}

pub async fn list(pool: &SqlitePool, thread_id: &str) -> Result<Vec<SubagentDto>, ServerError> {
    let rows = sqlx::query_as::<_, SubagentRow>(
        "SELECT id, thread_id, parent_id, title, description, status, cwd, can_cancel,
                usage_json, last_seq, created_at, updated_at
         FROM subagents WHERE thread_id = ? ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub async fn status_of(
    pool: &SqlitePool,
    subagent_id: &str,
) -> Result<SubagentStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
        .bind(subagent_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("subagent not found"))?;
    parse_subagent_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown subagent status: {value}")))
}

pub struct Upsert<'a> {
    pub thread_id: &'a str,
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub can_cancel: bool,
    pub usage_json: Option<&'a str>,
    pub action: SubagentAction,
}

/// Insert or update the identity fields, then apply the requested transition.
pub async fn upsert(
    pool: &SqlitePool,
    projection: &Projection,
    record: Upsert<'_>,
) -> Result<SubagentStatus, ServerError> {
    let created = SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO subagents(id, thread_id, parent_id, title, description, status, cwd,
            can_cancel, usage_json, last_seq, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title,
            description = excluded.description, cwd = excluded.cwd,
            can_cancel = excluded.can_cancel, usage_json = excluded.usage_json,
            updated_at = excluded.updated_at",
    )
    .bind(record.id)
    .bind(record.thread_id)
    .bind(record.parent_id)
    .bind(record.title)
    .bind(record.description)
    .bind(subagent_status_str(created))
    .bind(record.cwd)
    .bind(i64::from(record.can_cancel))
    .bind(record.usage_json)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    transition(pool, projection, record.thread_id, record.id, record.action).await
}

pub async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    subagent_id: &str,
    action: SubagentAction,
) -> Result<SubagentStatus, ServerError> {
    let current = {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
            .bind(subagent_id)
            .fetch_optional(pool)
            .await?;
        match value {
            Some(value) => parse_subagent_status(&value).ok_or_else(|| {
                ServerError::internal(format!("unknown subagent status: {value}"))
            })?,
            None => SubagentStatus::Idle,
        }
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
        .execute(pool)
        .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

fn to_dto(row: SubagentRow) -> SubagentDto {
    let status = parse_subagent_status(&row.status).unwrap_or(SubagentStatus::Idle);
    SubagentDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        parent_id: row.parent_id.map(Into::into),
        title: row.title,
        description: row.description,
        status,
        cwd: row.cwd,
        can_cancel: row.can_cancel != 0
            && !matches!(status, SubagentStatus::Cancelled | SubagentStatus::Error),
        usage: row
            .usage_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        last_seq: u64::try_from(row.last_seq).unwrap_or(0),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
