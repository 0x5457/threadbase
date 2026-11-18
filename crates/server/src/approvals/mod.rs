//! Agent permission requests, standing rules and user decisions.

mod rules;

pub use rules::{delete_rule, list_rules, match_rule, MatchedRule};

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{
    approval_decision_str, approval_status_str, parse_approval_status, ApprovalAction,
    ApprovalMachine,
};
use threadbase_shared::{
    now_ms, Actor, ApiError, ApprovalDecision, ApprovalDto, ApprovalKind, ApprovalOption,
    ApprovalScope, ApprovalStatus, PermissionMode, ThreadEvent, ToolCallId, UserBrief,
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

const SELECT: &str =
    "SELECT id, thread_id, subagent_id, turn_id, tool_call_id, agent, title, kind, command,
            cwd, paths_json, options_json, input_json, status, decision, scope, decided_by,
            decided_at, created_at
     FROM approvals";

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

pub async fn load(pool: &SqlitePool, approval_id: &str) -> Result<ApprovalRow, ServerError> {
    sqlx::query_as::<_, ApprovalRow>(&format!("{SELECT} WHERE id = ?"))
        .bind(approval_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("approval not found")))
}

pub async fn list_pending_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<ApprovalDto>, ServerError> {
    let rows = sqlx::query_as::<_, ApprovalRow>(&format!(
        "{SELECT} WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC"
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_dto).collect())
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

pub async fn status_of(
    pool: &SqlitePool,
    approval_id: &str,
) -> Result<ApprovalStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM approvals WHERE id = ?")
        .bind(approval_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("approval not found"))?;
    parse_approval_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown approval status: {value}")))
}

/// Persist a new pending approval. Initial state comes from the machine.
pub async fn insert(
    pool: &SqlitePool,
    approval: &NewApproval,
) -> Result<ApprovalStatus, ServerError> {
    let status = ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO approvals(id, thread_id, subagent_id, turn_id, tool_call_id, agent, title,
            kind, command, cwd, paths_json, options_json, input_json, status, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&approval.id)
    .bind(&approval.thread_id)
    .bind(approval.subagent_id.as_deref())
    .bind(approval.turn_id.as_deref())
    .bind(approval.tool_call_id.as_deref())
    .bind(&approval.agent)
    .bind(&approval.title)
    .bind(&approval.kind)
    .bind(approval.command.as_deref())
    .bind(approval.cwd.as_deref())
    .bind(serde_json::to_string(&approval.paths).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&approval.options).unwrap_or_else(|_| "[]".into()))
    .bind(approval.input.as_deref())
    .bind(approval_status_str(status))
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(status)
}

pub async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    approval_id: &str,
    action: ApprovalAction,
) -> Result<ApprovalStatus, ServerError> {
    let current = status_of(pool, approval_id).await?;
    let next = ApprovalMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    let updated = sqlx::query("UPDATE approvals SET status = ? WHERE id = ? AND status = ?")
        .bind(approval_status_str(next))
        .bind(approval_id)
        .bind(approval_status_str(current))
        .execute(pool)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError::internal("approval changed concurrently"));
    }
    projection.publish(thread_id).await;
    Ok(next)
}

pub struct Decision<'a> {
    pub decision: ApprovalDecision,
    pub scope: Option<&'a str>,
    /// `None` when a policy decided, not a person.
    pub decided_by: Option<&'a str>,
    pub decided_at: i64,
    pub note: Option<&'a str>,
}

pub async fn apply_decision(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    approval_id: &str,
    decision: &Decision<'_>,
) -> Result<ApprovalStatus, ServerError> {
    let action = match decision.decision {
        ApprovalDecision::AllowOnce
        | ApprovalDecision::AllowSession
        | ApprovalDecision::AllowThread => ApprovalAction::Allow,
        ApprovalDecision::Deny => ApprovalAction::Deny,
    };
    let current = status_of(pool, approval_id).await?;
    let next = ApprovalMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    let updated = sqlx::query(
        "UPDATE approvals SET status = ?, decision = ?, scope = ?, decided_by = ?, decided_at = ?,
            note = ? WHERE id = ? AND status = ?",
    )
    .bind(approval_status_str(next))
    .bind(approval_decision_str(decision.decision))
    .bind(decision.scope)
    .bind(decision.decided_by)
    .bind(decision.decided_at)
    .bind(decision.note)
    .bind(approval_id)
    .bind(approval_status_str(current))
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError::internal("approval changed concurrently"));
    }
    projection.publish(thread_id).await;
    Ok(next)
}

/// Record a request, append its event and notify members.
pub async fn create(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    approval: NewApproval,
    actor: Actor,
    meta: EventMeta,
) -> Result<ApprovalDto, ServerError> {
    // A request for a person must never stay in a shelf: snoozing a running
    // turn is legal, and the moment it asks something it belongs on screen.
    crate::threads::wake(pool, projection, &approval.thread_id).await?;
    insert(pool, &approval).await?;
    let dto = to_dto(load(pool, &approval.id).await?);
    events
        .append(
            &approval.thread_id.clone().into(),
            actor,
            ThreadEvent::PermissionRequested {
                approval: dto.clone(),
            },
            meta,
        )
        .await?;
    let summary = crate::notify::approval_summary(&approval.title);
    crate::notify::webhook(pool, "approval.pending", &approval.thread_id, &summary).await;
    crate::notify::notify_watchers(
        pool,
        projection,
        &approval.thread_id,
        "approval",
        Some(&approval.id),
        None,
        &summary,
        None,
    )
    .await?;
    // The newly-created row is pending, so the projection changed too.
    projection.publish(&approval.thread_id).await;
    Ok(dto)
}

/// Decide an approval on behalf of a user, persist a standing rule when the
/// scope is session/thread, and append the resolution event.
pub async fn decide(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    approval_id: &str,
    user: &CurrentUser,
    decision: ApprovalDecision,
    note: Option<&str>,
) -> Result<ApprovalDto, ServerError> {
    let row = load(pool, approval_id).await?;
    if row.status != "pending" {
        return Err(ServerError(ApiError::conflict(
            "this request has already been handled",
        )));
    }
    crate::authz::require_write_role(pool, &row.thread_id, user).await?;
    crate::watchers::watch(pool, &row.thread_id, user.user.id.as_str()).await?;
    let scope = resolve_scope(pool, &row, decision).await?;
    let now = now_ms();
    let next = apply_decision(
        pool,
        projection,
        &row.thread_id,
        approval_id,
        &Decision {
            decision,
            scope: scope.as_deref(),
            decided_by: Some(user.user.id.as_str()),
            decided_at: now,
            note,
        },
    )
    .await?;
    crate::inbox::clear_for_approval(pool, projection, approval_id).await?;
    if matches!(scope.as_deref(), Some("session" | "thread")) {
        rules::save_rule(
            pool,
            &row,
            scope.as_deref().unwrap_or_default(),
            user.user.id.as_str(),
            now,
        )
        .await?;
    }
    let actor = UserBrief::from_user(&user.user);
    events
        .append(
            &row.thread_id.clone().into(),
            Actor::user(&actor),
            ThreadEvent::PermissionResolved {
                approval_id: approval_id.to_owned().into(),
                decision,
                scope: scope.as_deref().and_then(parse_scope),
                decided_by: Some(actor.clone()),
            },
            EventMeta {
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
        Some(approval_status_str(next)),
        None,
    )
    .await;
    let mut resolved = to_dto(load(pool, approval_id).await?);
    resolved.decided_by = Some(actor);
    Ok(resolved)
}

/// A subagent may only inherit a thread-scoped rule that covers its worktree.
async fn resolve_scope(
    pool: &SqlitePool,
    row: &ApprovalRow,
    decision: ApprovalDecision,
) -> Result<Option<String>, ServerError> {
    let mut scope = match decision {
        ApprovalDecision::AllowOnce => Some("once".to_owned()),
        ApprovalDecision::AllowSession => Some("session".to_owned()),
        ApprovalDecision::AllowThread => Some("thread".to_owned()),
        ApprovalDecision::Deny => None,
    };
    if scope.as_deref() == Some("thread") && row.subagent_id.is_some() {
        let worktree: Option<String> =
            sqlx::query_scalar("SELECT worktree_path FROM threads WHERE id = ?")
                .bind(&row.thread_id)
                .fetch_one(pool)
                .await?;
        if let (Some(cwd), Some(root)) = (row.cwd.as_deref(), worktree.as_deref()) {
            if !threadbase_core::rule_inherits_to_child(root, cwd) {
                scope = Some("once".to_owned());
            }
        }
    }
    Ok(scope)
}

/// Why a request did not need a person.
///
/// A standing rule is a deliberate "always this" from one user and is recorded
/// as their decision. A mode is the thread's posture and is recorded as the
/// system's, so the transcript never claims a person approved something they
/// never saw.
#[derive(Debug, Clone)]
pub enum AutoApproval {
    Rule(MatchedRule),
    Mode(PermissionMode),
}

pub async fn auto_allow(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    approval_id: &str,
    reason: &AutoApproval,
) -> Result<ApprovalDto, ServerError> {
    let row = load(pool, approval_id).await?;
    if row.status != "pending" {
        return Ok(to_dto(row));
    }
    let (decision, scope, scope_dto, decided_by, note) = match reason {
        AutoApproval::Rule(rule) => {
            let decision = match rule.scope {
                ApprovalScope::Thread => ApprovalDecision::AllowThread,
                _ => ApprovalDecision::AllowSession,
            };
            let scope = match rule.scope {
                ApprovalScope::Thread => "thread",
                _ => "session",
            };
            (
                decision,
                Some(scope),
                Some(rule.scope),
                Some(rule.created_by.clone()),
                "matched standing rule",
            )
        }
        AutoApproval::Mode(mode) => (
            ApprovalDecision::AllowOnce,
            Some("once"),
            None,
            None,
            match mode {
                PermissionMode::AutoEdit => "allowed by auto-accept edits",
                _ => "allowed by full access",
            },
        ),
    };
    apply_decision(
        pool,
        projection,
        &row.thread_id,
        approval_id,
        &Decision {
            decision,
            scope,
            decided_by: decided_by.as_deref(),
            decided_at: now_ms(),
            note: Some(note),
        },
    )
    .await?;
    crate::inbox::clear_for_approval(pool, projection, approval_id).await?;
    let decider = match decided_by.as_deref() {
        Some(id) => crate::auth::load_user_by_id(pool, id)
            .await?
            .map(|user| UserBrief::from_user(&user)),
        None => None,
    };
    events
        .append(
            &row.thread_id.clone().into(),
            Actor::system(),
            ThreadEvent::PermissionResolved {
                approval_id: approval_id.to_owned().into(),
                decision,
                scope: scope_dto,
                decided_by: decider.clone(),
            },
            EventMeta {
                tool_call_id: row.tool_call_id.clone(),
                subagent_id: row.subagent_id.clone(),
                ..Default::default()
            },
        )
        .await?;
    crate::auth::audit(
        pool,
        decided_by.as_deref(),
        "approval.auto_allowed",
        Some("approval"),
        Some(approval_id),
        scope,
        None,
    )
    .await;
    let mut resolved = to_dto(load(pool, approval_id).await?);
    resolved.decided_by = decider;
    Ok(resolved)
}

fn to_dto(row: ApprovalRow) -> ApprovalDto {
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

pub(super) fn parse_scope(value: &str) -> Option<ApprovalScope> {
    match value {
        "once" => Some(ApprovalScope::Once),
        "session" => Some(ApprovalScope::Session),
        "thread" => Some(ApprovalScope::Thread),
        _ => None,
    }
}
