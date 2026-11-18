//! Agent elicitation requests (forms and URLs) and their resolution.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{
    elicitation_status_str, parse_elicitation_status, ElicitationAction as MachineAction,
    ElicitationMachine,
};
use threadbase_shared::{
    now_ms, Actor, ApiError, ElicitationAction, ElicitationDto, ElicitationMode, ElicitationStatus,
    ThreadEvent, UserBrief,
};

#[derive(FromRow)]
pub struct ElicitationRow {
    pub id: String,
    pub thread_id: String,
    pub subagent_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub agent: String,
    pub message: String,
    pub mode: String,
    pub schema_json: String,
    pub blocking: i64,
    pub status: String,
    pub response_json: Option<String>,
    pub responded_by: Option<String>,
    pub responder_username: Option<String>,
    pub responder_name: Option<String>,
    pub responder_avatar: Option<String>,
    pub responded_at: Option<i64>,
    pub created_at: i64,
}

const SELECT: &str =
    "SELECT e.id, e.thread_id, e.subagent_id, e.tool_call_id, e.agent, e.message, e.mode,
            e.schema_json, e.blocking, e.status, e.response_json, e.responded_by,
            ru.username AS responder_username, ru.display_name AS responder_name,
            ru.avatar_url AS responder_avatar, e.responded_at, e.created_at
     FROM elicitations e LEFT JOIN users ru ON ru.id = e.responded_by";

pub async fn load(pool: &SqlitePool, elicitation_id: &str) -> Result<ElicitationRow, ServerError> {
    sqlx::query_as::<_, ElicitationRow>(&format!("{SELECT} WHERE e.id = ?"))
        .bind(elicitation_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("question not found")))
}

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<ElicitationDto>, ServerError> {
    let rows = sqlx::query_as::<_, ElicitationRow>(&format!(
        "{SELECT} WHERE e.thread_id = ? ORDER BY e.created_at ASC"
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub async fn status_of(
    pool: &SqlitePool,
    elicitation_id: &str,
) -> Result<ElicitationStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM elicitations WHERE id = ?")
        .bind(elicitation_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("elicitation not found"))?;
    parse_elicitation_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown elicitation status: {value}")))
}

/// Persist a new pending elicitation. Initial state comes from the machine.
pub async fn insert(
    pool: &SqlitePool,
    elicitation: &ElicitationDto,
) -> Result<ElicitationStatus, ServerError> {
    let status = ElicitationMachine::transition(ElicitationStatus::Pending, &MachineAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO elicitations(id, thread_id, subagent_id, tool_call_id, agent, message, mode,
            schema_json, blocking, status, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(elicitation.id.as_str())
    .bind(elicitation.thread_id.as_str())
    .bind(elicitation.subagent_id.as_ref().map(|value| value.as_str()))
    .bind(
        elicitation
            .tool_call_id
            .as_ref()
            .map(|value| value.as_str()),
    )
    .bind(&elicitation.agent)
    .bind(&elicitation.message)
    .bind(mode_str(elicitation.mode))
    .bind(serde_json::to_string(&elicitation.requested_schema).unwrap_or_else(|_| "{}".into()))
    .bind(i64::from(elicitation.blocking))
    .bind(elicitation_status_str(status))
    .bind(elicitation.created_at)
    .execute(pool)
    .await?;
    Ok(status)
}

pub async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    elicitation_id: &str,
    action: MachineAction,
) -> Result<ElicitationStatus, ServerError> {
    let current = status_of(pool, elicitation_id).await?;
    let next = ElicitationMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    let updated = sqlx::query("UPDATE elicitations SET status = ? WHERE id = ? AND status = ?")
        .bind(elicitation_status_str(next))
        .bind(elicitation_id)
        .bind(elicitation_status_str(current))
        .execute(pool)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError::internal("elicitation changed concurrently"));
    }
    projection.publish(thread_id).await;
    Ok(next)
}

pub struct Resolution<'a> {
    pub action: MachineAction,
    pub response: Option<&'a serde_json::Value>,
    pub responded_by: &'a str,
    pub responded_at: i64,
}

pub async fn respond(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    elicitation_id: &str,
    resolution: Resolution<'_>,
) -> Result<ElicitationStatus, ServerError> {
    let current = status_of(pool, elicitation_id).await?;
    let next = ElicitationMachine::transition(current, &resolution.action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    let updated = sqlx::query(
        "UPDATE elicitations SET status = ?, response_json = ?, responded_by = ?, responded_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(elicitation_status_str(next))
    .bind(resolution.response.map(ToString::to_string))
    .bind(resolution.responded_by)
    .bind(resolution.responded_at)
    .bind(elicitation_id)
    .bind(elicitation_status_str(current))
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError::internal("elicitation changed concurrently"));
    }
    projection.publish(thread_id).await;
    Ok(next)
}

pub async fn cancel_all(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
) -> Result<u32, ServerError> {
    let cancelled =
        ElicitationMachine::transition(ElicitationStatus::Pending, &MachineAction::Cancel)
            .map_err(|error| ServerError::internal(error.to_string()))?;
    let result = sqlx::query(
        "UPDATE elicitations SET status = ?, responded_at = ?
         WHERE thread_id = ? AND status = 'pending'",
    )
    .bind(elicitation_status_str(cancelled))
    .bind(now_ms())
    .bind(thread_id)
    .execute(pool)
    .await?;
    projection.publish(thread_id).await;
    Ok(u32::try_from(result.rows_affected()).unwrap_or(0))
}

pub async fn create(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    elicitation: ElicitationDto,
    actor: Actor,
    meta: EventMeta,
) -> Result<ElicitationDto, ServerError> {
    // Same rule as approvals: an unanswered question pulls the thread back into
    // the active list, even if it was snoozed while the turn ran.
    crate::threads::wake(pool, projection, elicitation.thread_id.as_str()).await?;
    insert(pool, &elicitation).await?;
    projection.publish(elicitation.thread_id.as_str()).await;
    events
        .append(
            &elicitation.thread_id.clone(),
            actor,
            ThreadEvent::ElicitationRequested {
                elicitation: elicitation.clone(),
            },
            meta,
        )
        .await?;
    let summary = crate::notify::elicitation_summary(&elicitation.agent, &elicitation.message);
    crate::notify::webhook(
        pool,
        "elicitation.pending",
        elicitation.thread_id.as_str(),
        &summary,
    )
    .await;
    crate::notify::notify_watchers(
        pool,
        projection,
        elicitation.thread_id.as_str(),
        "elicitation",
        None,
        Some(elicitation.id.as_str()),
        &summary,
        None,
    )
    .await?;
    Ok(elicitation)
}

/// Resolve an elicitation on behalf of a user and record the event.
pub async fn respond_for_user(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    elicitation_id: &str,
    user: &CurrentUser,
    action: ElicitationAction,
    values: Option<serde_json::Value>,
) -> Result<ElicitationDto, ServerError> {
    let row = load(pool, elicitation_id).await?;
    if row.status != "pending" {
        return Err(ServerError(ApiError::conflict(
            "this question has already been answered",
        )));
    }
    crate::authz::require_write_role(pool, &row.thread_id, user).await?;
    crate::watchers::watch(pool, &row.thread_id, user.user.id.as_str()).await?;
    if action == ElicitationAction::Accept {
        reject_secret_schema(&row.schema_json)?;
    }
    let now = now_ms();
    let machine = match action {
        ElicitationAction::Accept => MachineAction::Accept,
        ElicitationAction::Decline => MachineAction::Decline,
        ElicitationAction::Cancel => MachineAction::Cancel,
    };
    let next = respond(
        pool,
        projection,
        &row.thread_id,
        elicitation_id,
        Resolution {
            action: machine,
            response: values.as_ref(),
            responded_by: user.user.id.as_str(),
            responded_at: now,
        },
    )
    .await?;
    crate::inbox::clear_for_elicitation(pool, projection, elicitation_id).await?;
    let actor = UserBrief::from_user(&user.user);
    events
        .append(
            &row.thread_id.clone().into(),
            Actor::user(&actor),
            ThreadEvent::ElicitationResolved {
                elicitation_id: elicitation_id.to_owned().into(),
                action,
                response: values,
                responded_by: Some(actor.clone()),
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
        "elicitation.responded",
        Some("elicitation"),
        Some(elicitation_id),
        Some(elicitation_status_str(next)),
        None,
    )
    .await;
    let mut dto = to_dto(load(pool, elicitation_id).await?);
    dto.responded_by = Some(actor);
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
            return Err(ServerError(ApiError::validation(
                "forms may not request passwords, tokens or secrets",
            )));
        }
    }
    Ok(())
}

fn to_dto(row: ElicitationRow) -> ElicitationDto {
    ElicitationDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        subagent_id: row.subagent_id.map(Into::into),
        tool_call_id: row.tool_call_id.map(Into::into),
        agent: row.agent,
        message: row.message,
        mode: match row.mode.as_str() {
            "url" => ElicitationMode::Url,
            _ => ElicitationMode::Form,
        },
        requested_schema: serde_json::from_str(&row.schema_json).unwrap_or(serde_json::Value::Null),
        blocking: row.blocking != 0,
        status: parse_elicitation_status(&row.status).unwrap_or(ElicitationStatus::Pending),
        response: row
            .response_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        responded_by: row.responded_by.map(|id| UserBrief {
            id: id.clone().into(),
            username: row.responder_username.unwrap_or_default(),
            display_name: row.responder_name.unwrap_or_default(),
            avatar_url: row.responder_avatar,
            color: threadbase_shared::user_color(&id).to_owned(),
        }),
        responded_at: row.responded_at,
        created_at: row.created_at,
    }
}

fn mode_str(mode: ElicitationMode) -> &'static str {
    match mode {
        ElicitationMode::Form => "form",
        ElicitationMode::Url => "url",
    }
}
