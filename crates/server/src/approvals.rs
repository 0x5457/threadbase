//! Approval records and agent permission decisions.

use crate::error::ServerError;
use crate::events::EventStore;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{
    approval_status_str, elicitation_status_str, parse_approval_status, parse_elicitation_status,
    ElicitationAction, ReadRules,
};
use threadbase_shared::{
    Actor, ApprovalDecision, ApprovalDto, ApprovalKind, ApprovalOption, ApprovalScope,
    ApprovalStatus, ToolCallId,
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
    pub input_json: Option<String>,
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
                cwd, paths_json, options_json, input_json, status, decision, scope, decided_by, decided_at, created_at
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

/// Total pending approvals plus elicitations for a thread.
pub async fn count_pending_items(pool: &SqlitePool, thread_id: &str) -> Result<u32, ServerError> {
    let approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    let elicitations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM elicitations WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    Ok(u32::try_from(approvals + elicitations).unwrap_or(0))
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
        input: row.input_json,
        status: parse_approval_status(&row.status).unwrap_or(ApprovalStatus::Pending),
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

#[derive(Debug, Clone)]
pub struct NewApproval {
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
    pub paths: Vec<String>,
    pub options: Vec<ApprovalOption>,
    pub input: Option<String>,
}

pub async fn create(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &crate::events::EventStore,
    approval: NewApproval,
    actor: threadbase_shared::Actor,
    meta: crate::events::EventMeta,
) -> Result<ApprovalDto, ServerError> {
    let now = threadbase_shared::now_ms();
    let paths_json = serde_json::to_string(&approval.paths).unwrap_or_else(|_| "[]".into());
    let options_json = serde_json::to_string(&approval.options).unwrap_or_else(|_| "[]".into());
    state_store
        .insert_approval(
            &approval,
            &paths_json,
            &options_json,
            approval.input.as_deref(),
            now,
        )
        .await?;
    let row = load(pool, &approval.id).await?;
    let dto = to_dto(row);
    events
        .append(
            &approval.thread_id.clone().into(),
            actor,
            threadbase_shared::ThreadEvent::PermissionRequested {
                approval: dto.clone(),
            },
            meta,
        )
        .await?;
    webhook(
        pool,
        "approval.pending",
        &approval.thread_id,
        &format!("Approval requested: {}", approval.title),
    )
    .await;
    notify_thread_members(
        pool,
        &approval.thread_id,
        "approval",
        Some(&approval.id),
        None,
        None,
        &format!("Approval requested: {}", approval.title),
    )
    .await?;
    Ok(dto)
}

pub async fn load(pool: &SqlitePool, approval_id: &str) -> Result<ApprovalRow, ServerError> {
    sqlx::query_as::<_, ApprovalRow>(
        "SELECT id, thread_id, subagent_id, turn_id, tool_call_id, agent, title, kind, command,
                cwd, paths_json, options_json, input_json, status, decision, scope, decided_by, decided_at, created_at
         FROM approvals WHERE id = ?",
    )
    .bind(approval_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ServerError(threadbase_shared::ApiError::not_found("approval not found")))
}

pub async fn decide(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &crate::events::EventStore,
    approval_id: &str,
    user: &crate::auth::CurrentUser,
    decision: threadbase_shared::ApprovalDecision,
    note: Option<&str>,
) -> Result<ApprovalDto, ServerError> {
    let row = load(pool, approval_id).await?;
    if row.status != "pending" {
        return Err(ServerError(threadbase_shared::ApiError::conflict(
            "this request has already been handled",
        )));
    }
    crate::authz::require_write_role(pool, &row.thread_id, user).await?;
    let mut scope = match decision {
        threadbase_shared::ApprovalDecision::AllowOnce => Some("once"),
        threadbase_shared::ApprovalDecision::AllowSession => Some("session"),
        threadbase_shared::ApprovalDecision::AllowThread => Some("thread"),
        threadbase_shared::ApprovalDecision::Deny => None,
    };
    if scope == Some("thread") && row.subagent_id.is_some() {
        let worktree: Option<String> =
            sqlx::query_scalar("SELECT worktree_path FROM threads WHERE id = ?")
                .bind(&row.thread_id)
                .fetch_one(pool)
                .await?;
        if let (Some(cwd), Some(root)) = (row.cwd.as_deref(), worktree.as_deref()) {
            if !threadbase_core::rule_inherits_to_child(root, cwd) {
                scope = Some("once");
            }
        }
    }
    let now = threadbase_shared::now_ms();
    let next_status = state_store
        .decide_approval(
            &row.thread_id,
            approval_id,
            decision,
            scope,
            user.user.id.as_str(),
            now,
            note,
        )
        .await?;
    if matches!(scope, Some("session" | "thread")) {
        let project_id: String = sqlx::query_scalar("SELECT project_id FROM threads WHERE id = ?")
            .bind(&row.thread_id)
            .fetch_one(pool)
            .await?;
        sqlx::query(
            "INSERT INTO approval_rules(id, project_id, thread_id, agent, tool, command_prefix, scope, created_by, created_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(project_id)
        .bind(&row.thread_id)
        .bind(&row.agent)
        .bind(&row.kind)
        .bind(row.command.as_deref())
        .bind(scope)
        .bind(user.user.id.as_str())
        .bind(now)
        .execute(pool)
        .await?;
    }
    let mut resolved = to_dto(load(pool, approval_id).await?);
    resolved.decided_by = Some(user_brief(&user.user));
    events
        .append(
            &row.thread_id.clone().into(),
            threadbase_shared::Actor::user(&user_brief(&user.user)),
            threadbase_shared::ThreadEvent::PermissionResolved {
                approval_id: approval_id.to_owned().into(),
                decision,
                scope: scope.and_then(parse_scope),
                decided_by: Some(user_brief(&user.user)),
            },
            crate::events::EventMeta {
                tool_call_id: row.tool_call_id.clone(),
                subagent_id: row.subagent_id.clone(),
                ..Default::default()
            },
        )
        .await?;
    crate::auth::audit(
        pool,
        Some(user.user.id.as_str()),
        "approval.decided",
        Some("approval"),
        Some(approval_id),
        Some(approval_status_str(next_status)),
        None,
    )
    .await;
    Ok(resolved)
}

pub async fn create_elicitation(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &crate::events::EventStore,
    elicitation: threadbase_shared::ElicitationDto,
    actor: threadbase_shared::Actor,
    meta: crate::events::EventMeta,
) -> Result<threadbase_shared::ElicitationDto, ServerError> {
    state_store
        .insert_elicitation(&elicitation, elicitation.created_at)
        .await?;
    events
        .append(
            &elicitation.thread_id.clone(),
            actor,
            threadbase_shared::ThreadEvent::ElicitationRequested {
                elicitation: elicitation.clone(),
            },
            meta,
        )
        .await?;
    webhook(
        pool,
        "elicitation.pending",
        elicitation.thread_id.as_str(),
        &format!(
            "Question from {}: {}",
            elicitation.agent, elicitation.message
        ),
    )
    .await;
    notify_thread_members(
        pool,
        elicitation.thread_id.as_str(),
        "elicitation",
        None,
        Some(elicitation.id.as_str()),
        None,
        &format!(
            "Question from {}: {}",
            elicitation.agent, elicitation.message
        ),
    )
    .await?;
    Ok(elicitation)
}

pub async fn respond_elicitation(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &crate::events::EventStore,
    elicitation_id: &str,
    user: &crate::auth::CurrentUser,
    action: threadbase_shared::ElicitationAction,
    values: Option<serde_json::Value>,
) -> Result<threadbase_shared::ElicitationDto, ServerError> {
    let row = load_elicitation(pool, elicitation_id).await?;
    if row.status != "pending" {
        return Err(ServerError(threadbase_shared::ApiError::conflict(
            "this question has already been answered",
        )));
    }
    crate::authz::require_write_role(pool, &row.thread_id, user).await?;
    if action == threadbase_shared::ElicitationAction::Accept {
        reject_secret_schema(&row.schema_json)?;
    }
    let machine_action = match action {
        threadbase_shared::ElicitationAction::Accept => ElicitationAction::Accept,
        threadbase_shared::ElicitationAction::Decline => ElicitationAction::Decline,
        threadbase_shared::ElicitationAction::Cancel => ElicitationAction::Cancel,
    };
    let now = threadbase_shared::now_ms();
    let next_status = state_store
        .respond_elicitation(
            &row.thread_id,
            elicitation_id,
            machine_action,
            values.as_ref(),
            user.user.id.as_str(),
            now,
        )
        .await?;
    events
        .append(
            &row.thread_id.clone().into(),
            threadbase_shared::Actor::user(&user_brief(&user.user)),
            threadbase_shared::ThreadEvent::ElicitationResolved {
                elicitation_id: elicitation_id.to_owned().into(),
                action,
                response: values,
                responded_by: Some(user_brief(&user.user)),
            },
            crate::events::EventMeta {
                tool_call_id: row.tool_call_id.clone(),
                subagent_id: row.subagent_id.clone(),
                ..Default::default()
            },
        )
        .await?;
    crate::auth::audit(
        pool,
        Some(user.user.id.as_str()),
        "elicitation.responded",
        Some("elicitation"),
        Some(elicitation_id),
        Some(elicitation_status_str(next_status)),
        None,
    )
    .await;
    let mut dto = to_elicitation_dto(load_elicitation(pool, elicitation_id).await?)?;
    dto.responded_by = Some(user_brief(&user.user));
    dto.responded_at = Some(now);
    Ok(dto)
}

fn reject_secret_schema(schema_json: &str) -> Result<(), ServerError> {
    let lower = schema_json.to_lowercase();
    for marker in [
        "password",
        "api_key",
        "apikey",
        "secret",
        "token",
        "private_key",
    ] {
        if lower.contains(marker) {
            return Err(ServerError(threadbase_shared::ApiError::validation(
                "forms may not request passwords, tokens or secrets",
            )));
        }
    }
    Ok(())
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
            scope: parse_scope(&row.scope).unwrap_or(threadbase_shared::ApprovalScope::Once),
            created_by: threadbase_shared::UserBrief {
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
        return Err(ServerError(threadbase_shared::ApiError::not_found(
            "approval rule not found",
        )));
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
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

#[derive(Debug, sqlx::FromRow)]
struct ElicitationRow {
    id: String,
    thread_id: String,
    subagent_id: Option<String>,
    tool_call_id: Option<String>,
    agent: String,
    message: String,
    mode: String,
    schema_json: String,
    blocking: i64,
    status: String,
    response_json: Option<String>,
    responded_by: Option<String>,
    responder_username: Option<String>,
    responder_name: Option<String>,
    responder_avatar: Option<String>,
    responded_at: Option<i64>,
    created_at: i64,
}

async fn load_elicitation(
    pool: &SqlitePool,
    elicitation_id: &str,
) -> Result<ElicitationRow, ServerError> {
    sqlx::query_as::<_, ElicitationRow>(
        "SELECT e.id, e.thread_id, e.subagent_id, e.tool_call_id, e.agent, e.message, e.mode,
                e.schema_json, e.blocking, e.status, e.response_json, e.responded_by,
                ru.username AS responder_username, ru.display_name AS responder_name,
                ru.avatar_url AS responder_avatar, e.responded_at, e.created_at
         FROM elicitations e LEFT JOIN users ru ON ru.id = e.responded_by
         WHERE e.id = ?",
    )
    .bind(elicitation_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ServerError(threadbase_shared::ApiError::not_found("question not found")))
}

fn to_elicitation_dto(
    row: ElicitationRow,
) -> Result<threadbase_shared::ElicitationDto, ServerError> {
    Ok(threadbase_shared::ElicitationDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        subagent_id: row.subagent_id.map(Into::into),
        tool_call_id: row.tool_call_id.map(Into::into),
        agent: row.agent,
        message: row.message,
        mode: match row.mode.as_str() {
            "url" => threadbase_shared::ElicitationMode::Url,
            _ => threadbase_shared::ElicitationMode::Form,
        },
        requested_schema: serde_json::from_str(&row.schema_json).unwrap_or(serde_json::Value::Null),
        blocking: row.blocking != 0,
        status: parse_elicitation_status(&row.status)
            .unwrap_or(threadbase_shared::ElicitationStatus::Pending),
        response: row
            .response_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        responded_by: row
            .responded_by
            .as_ref()
            .map(|id| threadbase_shared::UserBrief {
                id: id.clone().into(),
                username: row.responder_username.clone().unwrap_or_default(),
                display_name: row.responder_name.clone().unwrap_or_default(),
                avatar_url: row.responder_avatar.clone(),
                color: threadbase_shared::user_color(id).to_owned(),
            }),
        responded_at: row.responded_at,
        created_at: row.created_at,
    })
}

pub async fn list_elicitations(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<threadbase_shared::ElicitationDto>, ServerError> {
    let rows = sqlx::query_as::<_, ElicitationRow>(
        "SELECT e.id, e.thread_id, e.subagent_id, e.tool_call_id, e.agent, e.message, e.mode,
                e.schema_json, e.blocking, e.status, e.response_json, e.responded_by,
                ru.username AS responder_username, ru.display_name AS responder_name,
                ru.avatar_url AS responder_avatar, e.responded_at, e.created_at
         FROM elicitations e LEFT JOIN users ru ON ru.id = e.responded_by
         WHERE e.thread_id = ? ORDER BY e.created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(to_elicitation_dto).collect()
}

async fn notify_thread_members(
    pool: &SqlitePool,
    thread_id: &str,
    kind: &str,
    approval_id: Option<&str>,
    elicitation_id: Option<&str>,
    comment_id: Option<&str>,
    summary: &str,
) -> Result<(), ServerError> {
    let members: Vec<String> =
        sqlx::query_scalar("SELECT user_id FROM thread_members WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_all(pool)
            .await?;
    for user_id in members {
        sqlx::query(
            "INSERT INTO inbox(id, user_id, kind, thread_id, approval_id, elicitation_id, comment_id, summary, created_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&user_id)
        .bind(kind)
        .bind(thread_id)
        .bind(approval_id)
        .bind(elicitation_id)
        .bind(comment_id)
        .bind(summary)
        .bind(threadbase_shared::now_ms())
        .execute(pool)
        .await?;
    }
    Ok(())
}

fn user_brief(user: &threadbase_shared::UserDto) -> threadbase_shared::UserBrief {
    threadbase_shared::UserBrief {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        color: threadbase_shared::user_color(user.id.as_str()).to_owned(),
    }
}

pub async fn list_inbox(
    pool: &SqlitePool,
    user: &crate::auth::CurrentUser,
    unread_only: bool,
) -> Result<threadbase_shared::InboxPageDto, ServerError> {
    #[derive(Debug, sqlx::FromRow)]
    struct Row {
        id: String,
        kind: String,
        thread_id: String,
        thread_title: String,
        project_id: String,
        summary: String,
        created_at: i64,
        read_at: Option<i64>,
        approval_id: Option<String>,
        elicitation_id: Option<String>,
        comment_id: Option<String>,
        actor_id: Option<String>,
        actor_name: Option<String>,
        actor_username: Option<String>,
        actor_avatar: Option<String>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT i.id, i.kind, i.thread_id, t.title AS thread_title, t.project_id,
                i.summary, i.created_at, i.read_at, i.approval_id, i.elicitation_id, i.comment_id,
                i.actor_id, COALESCE(u.display_name, u2.display_name) AS actor_name,
                COALESCE(u.username, u2.username) AS actor_username,
                COALESCE(u.avatar_url, u2.avatar_url) AS actor_avatar
         FROM inbox i
         JOIN threads t ON t.id = i.thread_id
         LEFT JOIN users u ON u.id = i.actor_id
         LEFT JOIN users u2 ON u2.id = t.created_by
         WHERE i.user_id = ? AND (? = 0 OR i.read_at IS NULL)
         ORDER BY i.created_at DESC LIMIT 200",
    )
    .bind(user.user.id.as_str())
    .bind(i64::from(unread_only))
    .fetch_all(pool)
    .await?;
    let mut unread = 0;
    let items = rows
        .into_iter()
        .map(|row| {
            if ReadRules::inbox_unread(row.read_at) {
                unread += 1;
            }
            let actor = row
                .actor_id
                .as_ref()
                .map(|id| threadbase_shared::UserBrief {
                    id: id.clone().into(),
                    username: row.actor_username.clone().unwrap_or_default(),
                    display_name: row.actor_name.clone().unwrap_or_default(),
                    avatar_url: row.actor_avatar.clone(),
                    color: threadbase_shared::user_color(id).to_owned(),
                });
            threadbase_shared::InboxItemDto {
                id: row.id,
                kind: parse_inbox_kind(&row.kind),
                actor,
                thread_id: row.thread_id.into(),
                thread_title: row.thread_title,
                project_id: row.project_id.into(),
                summary: row.summary,
                created_at: row.created_at,
                read: !ReadRules::inbox_unread(row.read_at),
                approval_id: row.approval_id.map(Into::into),
                elicitation_id: row.elicitation_id.map(Into::into),
                comment_id: row.comment_id.map(Into::into),
            }
        })
        .collect();
    Ok(threadbase_shared::InboxPageDto { items, unread })
}

pub async fn mark_inbox_read(
    pool: &SqlitePool,
    user: &crate::auth::CurrentUser,
    thread_id: Option<&str>,
    all: bool,
) -> Result<u32, ServerError> {
    let now = threadbase_shared::now_ms();
    let updated = if all || thread_id.is_none() {
        sqlx::query("UPDATE inbox SET read_at = ? WHERE user_id = ? AND read_at IS NULL")
            .bind(now)
            .bind(user.user.id.as_str())
            .execute(pool)
            .await?
    } else {
        sqlx::query(
            "UPDATE inbox SET read_at = ? WHERE user_id = ? AND thread_id = ? AND read_at IS NULL",
        )
        .bind(now)
        .bind(user.user.id.as_str())
        .bind(thread_id)
        .execute(pool)
        .await?
    };
    Ok(u32::try_from(updated.rows_affected()).unwrap_or(0))
}

fn parse_inbox_kind(value: &str) -> threadbase_shared::InboxKind {
    match value {
        "approval" => threadbase_shared::InboxKind::Approval,
        "elicitation" => threadbase_shared::InboxKind::Elicitation,
        "mention" => threadbase_shared::InboxKind::Mention,
        "invite" => threadbase_shared::InboxKind::Invite,
        "access" => threadbase_shared::InboxKind::Access,
        _ => threadbase_shared::InboxKind::Error,
    }
}

/// Minimal webhook contract: POST JSON once, no retries, never blocks a thread.
/// Reads the thread creator's saved setting, falling back to the environment.
async fn webhook(pool: &SqlitePool, event: &str, thread_id: &str, summary: &str) {
    let mut url = std::env::var("THREADBASE_WEBHOOK_URL").unwrap_or_default();
    if url.trim().is_empty() {
        if let Ok(settings_json) = sqlx::query_scalar::<_, String>(
            "SELECT u.settings_json FROM threads t JOIN users u ON u.id = t.created_by WHERE t.id = ?",
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await
        {
            if let Ok(settings) =
                serde_json::from_str::<threadbase_shared::SettingsDto>(&settings_json)
            {
                url = settings.webhook_url.unwrap_or_default();
            }
        }
    }
    if url.trim().is_empty() {
        return;
    }
    let payload = serde_json::json!({
        "event": event,
        "threadId": thread_id,
        "summary": summary,
        "createdAt": threadbase_shared::now_ms(),
    });
    let event_name = event.to_owned();
    tokio::spawn(async move {
        let result = reqwest::Client::new()
            .post(url)
            .timeout(std::time::Duration::from_secs(5))
            .json(&payload)
            .send()
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, event = %event_name, "webhook delivery failed");
        }
    });
}

#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub scope: threadbase_shared::ApprovalScope,
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
            .ok_or_else(|| {
                ServerError(threadbase_shared::ApiError::not_found("thread not found"))
            })?;
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
            "thread" => threadbase_shared::ApprovalScope::Thread,
            _ => threadbase_shared::ApprovalScope::Session,
        };
        return Ok(Some(MatchedRule { scope, created_by }));
    }
    Ok(None)
}

pub async fn auto_allow(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &EventStore,
    approval_id: &str,
    rule: &MatchedRule,
) -> Result<ApprovalDto, ServerError> {
    let row = load(pool, approval_id).await?;
    if row.status != "pending" {
        // A concurrent decision already handled this approval.
        return Ok(to_dto(row));
    }
    let decided_by = crate::auth::load_user_by_id(pool, &rule.created_by)
        .await?
        .ok_or_else(|| {
            ServerError(threadbase_shared::ApiError::not_found(
                "rule owner not found",
            ))
        })?;
    let decision = match rule.scope {
        threadbase_shared::ApprovalScope::Thread => {
            threadbase_shared::ApprovalDecision::AllowThread
        }
        _ => threadbase_shared::ApprovalDecision::AllowSession,
    };
    let scope = match rule.scope {
        threadbase_shared::ApprovalScope::Thread => "thread",
        _ => "session",
    };
    let now = threadbase_shared::now_ms();
    state_store
        .auto_allow_approval(
            &row.thread_id,
            approval_id,
            decision,
            scope,
            rule.created_by.as_str(),
            now,
        )
        .await?;
    sqlx::query("DELETE FROM inbox WHERE approval_id = ?")
        .bind(approval_id)
        .execute(pool)
        .await?;
    let mut resolved = to_dto(load(pool, approval_id).await?);
    resolved.decided_by = Some(user_brief(&decided_by));
    events
        .append(
            &row.thread_id.clone().into(),
            Actor::system(),
            threadbase_shared::ThreadEvent::PermissionResolved {
                approval_id: approval_id.to_owned().into(),
                decision,
                scope: Some(rule.scope),
                decided_by: Some(user_brief(&decided_by)),
            },
            crate::events::EventMeta {
                tool_call_id: row.tool_call_id.clone(),
                subagent_id: row.subagent_id.clone(),
                ..Default::default()
            },
        )
        .await?;
    crate::auth::audit(
        pool,
        Some(rule.created_by.as_str()),
        "approval.auto_allowed",
        Some("approval"),
        Some(approval_id),
        Some(scope),
        None,
    )
    .await;
    Ok(resolved)
}
