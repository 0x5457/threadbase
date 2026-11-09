//! Agent input, cancellation and queue routes.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use sqlx::FromRow;
use threadbase_shared::api::SendMessageRequest;
use threadbase_shared::{
    now_ms, user_color, ApiError, OkResponse, QueueItemDto, QueueResponse, QueueStatus,
    SendMessageResponse, ThreadId, UserBrief,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/messages", post(send_message))
        .route("/api/threads/{thread_id}/cancel", post(cancel))
        .route("/api/threads/{thread_id}/read", post(mark_read))
        .route("/api/threads/{thread_id}/agent-config", get(agent_config))
        .route("/api/threads/{thread_id}/restart", post(restart_agent))
        .route(
            "/api/threads/{thread_id}/subagents/{subagent_id}/cancel",
            post(cancel_subagent),
        )
        .route("/api/threads/{thread_id}/queue", get(list_queue))
        .route(
            "/api/threads/{thread_id}/queue/{item_id}",
            delete(drop_queue_item),
        )
        .route(
            "/api/threads/{thread_id}/queue/{item_id}/retry",
            post(retry_queue_item),
        )
}

async fn send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let text = if request.from_draft.unwrap_or(false) {
        let blocks = state
            .drafts
            .take_blocks(&thread_id)
            .await
            .map_err(|error| ServerError::internal(error.to_string()))?;
        blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    } else {
        request.text.trim().to_owned()
    };
    if text.is_empty() {
        return Err(ServerError(ApiError::validation("message cannot be empty")));
    }
    if text.len() > 64 * 1024 {
        return Err(ServerError(ApiError::validation("message is too long")));
    }
    let pending = crate::comments::pending_for_thread(&state.pool, &thread_id).await?;
    let text = if pending.is_empty() {
        text
    } else {
        let comments = pending
            .iter()
            .map(|comment| {
                let anchor = serde_json::to_string(&comment.anchor).unwrap_or_default();
                format!("- {} (anchor: {})", comment.body, anchor)
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{text}\n\nReview comments to address:\n{comments}")
    };
    let response = state
        .agents
        .send(&thread_id, &user, text.as_str(), request.delivery)
        .await?;
    if response.delivery != threadbase_shared::Delivery::Queued {
        let last_seq = state
            .events
            .last_seq(&ThreadId::from(thread_id.clone()))
            .await
            .ok();
        crate::comments::mark_pending_delivered(
            &state.pool,
            &state.events,
            &thread_id,
            &user,
            last_seq,
        )
        .await?;
    }
    Ok(Json(response))
}

async fn agent_config(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<threadbase_shared::AgentConfigResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let options = state
        .agents
        .config_options(&thread_id)
        .await
        .unwrap_or_default();
    Ok(Json(threadbase_shared::AgentConfigResponse { options }))
}

async fn restart_agent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    state.agents.restart(&thread_id).await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn cancel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    state.agents.cancel(&thread_id).await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn cancel_subagent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, subagent_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    state
        .agents
        .cancel_subagent(&thread_id, &subagent_id)
        .await?;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, FromRow)]
struct QueueRow {
    id: String,
    thread_id: String,
    author_id: String,
    text: String,
    status: String,
    position: i64,
    superseded: i64,
    created_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

async fn list_queue(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<QueueResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let rows = sqlx::query_as::<_, QueueRow>(
        "SELECT q.id, q.thread_id, q.author_id, q.text, q.status, q.position, q.superseded,
                q.created_at, u.username, u.display_name, u.avatar_url
         FROM queued_inputs q JOIN users u ON u.id = q.author_id
         WHERE q.thread_id = ? AND q.status IN ('queued','sending','superseded')
         ORDER BY q.position ASC",
    )
    .bind(&thread_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(QueueResponse {
        items: rows.into_iter().map(queue_dto).collect(),
    }))
}

async fn retry_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let result = sqlx::query(
        "UPDATE queued_inputs SET superseded = 0, updated_at = ?
         WHERE id = ? AND thread_id = ? AND status = 'queued'",
    )
    .bind(now_ms())
    .bind(&item_id)
    .bind(&thread_id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}

async fn drop_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let result = sqlx::query(
        "UPDATE queued_inputs SET status = 'dropped', updated_at = ?
         WHERE id = ? AND thread_id = ? AND status = 'queued'",
    )
    .bind(now_ms())
    .bind(&item_id)
    .bind(&thread_id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}

fn queue_dto(row: QueueRow) -> QueueItemDto {
    QueueItemDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        author: UserBrief {
            id: row.author_id.clone().into(),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            color: user_color(&row.author_id).to_owned(),
        },
        text: row.text,
        status: match row.status.as_str() {
            "sending" => QueueStatus::Sending,
            "sent" => QueueStatus::Sent,
            "superseded" => QueueStatus::Superseded,
            "dropped" => QueueStatus::Dropped,
            _ => QueueStatus::Queued,
        },
        position: u32::try_from(row.position).unwrap_or(0),
        created_at: row.created_at,
        superseded: row.superseded != 0,
    }
}

async fn mark_read(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let last_seq: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?")
            .bind(&thread_id)
            .fetch_one(&state.pool)
            .await?;
    sqlx::query(
        "INSERT INTO thread_reads(thread_id, user_id, last_read_seq, updated_at)
         VALUES(?, ?, ?, ?)
         ON CONFLICT(thread_id, user_id) DO UPDATE SET
            last_read_seq = excluded.last_read_seq, updated_at = excluded.updated_at",
    )
    .bind(&thread_id)
    .bind(user.user.id.as_str())
    .bind(last_seq)
    .bind(now_ms())
    .execute(&state.pool)
    .await?;
    Ok(Json(OkResponse { ok: true }))
}
