//! Approval records and agent permission decisions.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::{
    ApprovalDecision, ApprovalDto, ApprovalKind, ApprovalOption, ApprovalScope, ApprovalStatus,
    ToolCallId,
};

#[derive(Debug, FromRow)]
pub struct ApprovalRow {
    pub id: String,
    pub thread_id: String,
    pub subagent_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub agent: String,
    pub title: String,
    pub kind: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub paths_json: String,
    pub options_json: String,
    pub status: String,
    pub decision: Option<String>,
    pub scope: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub created_at: i64,
}

pub async fn list_pending_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<ApprovalDto>, ServerError> {
    let rows = sqlx::query_as::<_, ApprovalRow>(
        "SELECT id, thread_id, subagent_id, turn_id, tool_call_id, agent, title, kind, command,
                cwd, paths_json, options_json, status, decision, scope, decided_by, decided_at, created_at
         FROM approvals WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub async fn count_pending(pool: &SqlitePool, thread_id: &str) -> Result<u32, ServerError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(count).unwrap_or(0))
}

#[must_use]
pub fn to_dto(row: ApprovalRow) -> ApprovalDto {
    ApprovalDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        subagent_id: row.subagent_id.map(Into::into),
        turn_id: row.turn_id,
        tool_call_id: row.tool_call_id.map(ToolCallId::from),
        agent: row.agent,
        title: row.title,
        kind: parse_kind(&row.kind),
        command: row.command,
        cwd: row.cwd,
        paths: serde_json::from_str(&row.paths_json).unwrap_or_default(),
        options: serde_json::from_str::<Vec<ApprovalOption>>(&row.options_json).unwrap_or_default(),
        status: parse_status(&row.status),
        decision: row.decision.as_deref().and_then(parse_decision),
        scope: row.scope.as_deref().and_then(parse_scope),
        decided_by: None,
        decided_at: row.decided_at,
        created_at: row.created_at,
    }
}

fn parse_kind(value: &str) -> ApprovalKind {
    match value {
        "command" => ApprovalKind::Command,
        "file_read" => ApprovalKind::FileRead,
        "file_write" => ApprovalKind::FileWrite,
        "network" => ApprovalKind::Network,
        _ => ApprovalKind::Other,
    }
}

fn parse_status(value: &str) -> ApprovalStatus {
    match value {
        "allowed" => ApprovalStatus::Allowed,
        "denied" => ApprovalStatus::Denied,
        "cancelled" => ApprovalStatus::Cancelled,
        _ => ApprovalStatus::Pending,
    }
}

fn parse_decision(value: &str) -> Option<ApprovalDecision> {
    match value {
        "allow_once" => Some(ApprovalDecision::AllowOnce),
        "allow_session" => Some(ApprovalDecision::AllowSession),
        "allow_thread" => Some(ApprovalDecision::AllowThread),
        "deny" => Some(ApprovalDecision::Deny),
        _ => None,
    }
}

fn parse_scope(value: &str) -> Option<ApprovalScope> {
    match value {
        "once" => Some(ApprovalScope::Once),
        "session" => Some(ApprovalScope::Session),
        "thread" => Some(ApprovalScope::Thread),
        _ => None,
    }
}
