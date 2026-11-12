//! Agent input, cancellation and queue routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use threadbase_shared::api::SendMessageRequest;
use threadbase_shared::{ApiError, OkResponse, SendMessageResponse, ThreadId};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/messages", post(send_message))
        .route("/api/threads/{thread_id}/cancel", post(cancel))
        .route("/api/threads/{thread_id}/agent-config", get(agent_config))
        .route("/api/threads/{thread_id}/restart", post(restart_agent))
        .route(
            "/api/threads/{thread_id}/subagents/{subagent_id}/cancel",
            post(cancel_subagent),
        )
        .route("/api/threads/{thread_id}/queue/order", post(reorder_queue))
        .route(
            "/api/threads/{thread_id}/queue/{item_id}",
            delete(drop_queue_item),
        )
        .route(
            "/api/threads/{thread_id}/queue/{item_id}/restore",
            post(restore_queue_item),
        )
}

async fn send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let from_draft = request.from_draft.unwrap_or(false);
    // Serialize peek → send → take so two senders cannot consume the same
    // blocks, and a failed send never loses the draft.
    let _send_guard = if from_draft {
        Some(state.drafts.send_guard(&thread_id).await)
    } else {
        None
    };
    let mut draft_authors = Vec::new();
    let text = if from_draft {
        let blocks = state
            .drafts
            .peek_blocks(&thread_id)
            .await
            .map_err(|error| ServerError::internal(error.to_string()))?;
        let ids: Vec<String> = blocks.iter().map(|block| block.author.clone()).collect();
        draft_authors = crate::agents::briefs_by_ids(&state.pool, &ids).await;
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
        let message = if from_draft {
            "draft already sent"
        } else {
            "message cannot be empty"
        };
        return Err(ServerError(ApiError::validation(message)));
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
    let authors = if draft_authors.is_empty() {
        vec![crate::agents::user_brief(&user)]
    } else {
        draft_authors
    };
    let response = state
        .agents
        .send_with_authors(&thread_id, &user, text.as_str(), request.delivery, &authors)
        .await?;
    if from_draft {
        state
            .drafts
            .take_blocks(&thread_id)
            .await
            .map_err(|error| ServerError::internal(error.to_string()))?;
    }
    if response.delivery != threadbase_shared::Delivery::Queued {
        let last_seq = state
            .events
            .last_seq(&ThreadId::from(thread_id.clone()))
            .await
            .ok();
        crate::comments::mark_pending_delivered(
            &state.pool,
            &state.events,
            &pending,
            &user,
            last_seq,
        )
        .await?;
    }
    Ok(Json(response))
}

#[derive(Debug, Default, Deserialize)]
struct AgentConfigQuery {
    /// Start the bound adapter when it is not running yet so it can advertise
    /// its model and effort options before the first message.
    ensure: Option<bool>,
}

async fn agent_config(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<AgentConfigQuery>,
) -> Result<Json<threadbase_shared::AgentConfigResponse>, ServerError> {
    let role = authz::require_read(&state.pool, &thread_id, &user).await?;
    if query.ensure.unwrap_or(false) && role.can_write() {
        if let Err(error) = state.agents.ensure_started(&thread_id).await {
            tracing::warn!(thread_id, %error, "could not start agent to read config");
        }
    }
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

async fn drop_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    crate::queue::remove(&state.pool, &state.projection, &thread_id, &item_id).await?;
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}

async fn restore_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    crate::queue::restore(&state.pool, &state.projection, &thread_id, &item_id).await?;
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderQueueRequest {
    item_ids: Vec<String>,
}

async fn reorder_queue(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<ReorderQueueRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    crate::queue::reorder(
        &state.pool,
        &state.projection,
        &thread_id,
        &request.item_ids,
    )
    .await?;
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}
