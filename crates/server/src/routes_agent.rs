//! Agent input, cancellation and queue routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use threadbase_shared::api::SendMessageRequest;
use threadbase_shared::{ApiError, OkResponse, SendMessageResponse, SubagentLogResponse, ThreadId};

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
        .route(
            "/api/threads/{thread_id}/subagents/{subagent_id}/log",
            get(subagent_log),
        )
        .route("/api/threads/{thread_id}/queue/order", post(reorder_queue))
        .route(
            "/api/threads/{thread_id}/queue/{item_id}",
            delete(drop_queue_item).patch(update_queue_item),
        )
        .route(
            "/api/threads/{thread_id}/queue/{item_id}/restore",
            post(restore_queue_item),
        )
        .route(
            "/api/threads/{thread_id}/queue/{item_id}/steer",
            post(steer_queue_item),
        )
        .route(
            "/api/threads/{thread_id}/attachments",
            post(upload_attachment).layer(DefaultBodyLimit::max(
                crate::attachments::MAX_IMAGE_BYTES + 1,
            )),
        )
        .route("/api/attachments/{attachment_id}", get(read_attachment))
}

async fn send_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let from_draft = request.from_draft.unwrap_or(false);
    let attachment_ids = request.attachment_ids.clone().unwrap_or_default();
    // Serialize peek → send → take so two senders cannot consume the same
    // blocks, and a failed send never loses the draft.
    let _send_guard = if from_draft {
        Some(state.drafts.send_guard(&thread_id).await)
    } else {
        None
    };
    let mut draft_authors = Vec::new();
    let text = if from_draft {
        let (text, editors) = state
            .drafts
            .peek_draft(&thread_id)
            .await
            .map_err(|error| ServerError::internal(error.to_string()))?;
        draft_authors = crate::agents::briefs_by_ids(&state.pool, &editors).await;
        text.trim().to_owned()
    } else {
        request.text.trim().to_owned()
    };
    if text.is_empty() && attachment_ids.is_empty() {
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
    let attachments = crate::attachments::resolve(&state.pool, &thread_id, &attachment_ids).await?;
    let response = state
        .agents
        .send_with_authors(
            &thread_id,
            &user,
            text.as_str(),
            request.delivery,
            &authors,
            &attachments,
        )
        .await?;
    if from_draft {
        state
            .drafts
            .take_draft(&thread_id)
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

async fn subagent_log(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, subagent_id)): Path<(String, String)>,
) -> Result<Json<SubagentLogResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(SubagentLogResponse {
        entries: crate::subagents::transcript(&state.pool, &thread_id, &subagent_id).await?,
    }))
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

/// Rewrite a queued message in place, so it can be fixed without dropping it
/// and losing its queue slot.
async fn update_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
    Json(request): Json<threadbase_shared::api::UpdateQueuedInputRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let text = request.text.trim();
    if text.is_empty() {
        return Err(ServerError(ApiError::validation("message cannot be empty")));
    }
    if text.len() > 64 * 1024 {
        return Err(ServerError(ApiError::validation("message is too long")));
    }
    crate::queue::update_text(&state.pool, &state.projection, &thread_id, &item_id, text).await?;
    let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
    Ok(Json(OkResponse { ok: true }))
}

/// Deliver a queued message into the running turn instead of waiting for the
/// next one. A rejection returns it to the queue unchanged.
async fn steer_queue_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, item_id)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let item = crate::queue::item(&state.pool, &thread_id, &item_id).await?;
    if item.status != threadbase_shared::QueueStatus::Queued {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    let author = crate::agents::briefs_by_ids(&state.pool, std::slice::from_ref(&item.author_id))
        .await
        .pop()
        .ok_or_else(|| ServerError::internal("queue author vanished"))?;
    let authors = item
        .authors_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<threadbase_shared::UserBrief>>(json).ok())
        .filter(|authors| !authors.is_empty())
        .unwrap_or_else(|| vec![author.clone()]);
    let attachments = item
        .attachments_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<threadbase_shared::AttachmentDto>>(json).ok())
        .unwrap_or_default();
    // Claim the row before steering so the queue worker cannot also dispatch
    // it; a failed injection puts it back where it was.
    crate::queue::dispatch(&state.pool, &state.projection, &thread_id, &item_id).await?;
    match state
        .agents
        .steer_queued(&thread_id, &author, &item.text, &authors, &attachments)
        .await
    {
        Ok(()) => {
            crate::queue::complete(&state.pool, &state.projection, &thread_id, &item_id, None)
                .await?;
            let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
            Ok(Json(OkResponse { ok: true }))
        }
        Err(error) if error.0.code == threadbase_shared::ErrorCode::Conflict => {
            // This adapter cannot take input mid-turn (or no turn is running):
            // make the message the next turn and stop the current one. The
            // queue is deliberately not superseded, so the message survives.
            if let Err(release) =
                crate::queue::release(&state.pool, &state.projection, &thread_id, &item_id).await
            {
                tracing::warn!(%release, thread_id, item_id, "could not requeue the queued message");
            }
            crate::queue::reorder(
                &state.pool,
                &state.projection,
                &thread_id,
                std::slice::from_ref(&item_id),
            )
            .await?;
            let cancelled = state.agents.interrupt(&thread_id).await?;
            if !cancelled {
                state.agents.kick_queue(&thread_id);
            }
            let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
            Ok(Json(OkResponse { ok: true }))
        }
        Err(error) => {
            if let Err(release) =
                crate::queue::release(&state.pool, &state.projection, &thread_id, &item_id).await
            {
                tracing::warn!(%release, thread_id, item_id, "could not requeue a steered message");
            }
            let _ = crate::queue::publish(&state.pool, &state.events, &thread_id).await;
            Err(error)
        }
    }
}

/// Store one pasted image. The body is the raw image; the content type names
/// the format. Uploads are scoped to the thread and gated on write access.
async fn upload_attachment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Json<threadbase_shared::AttachmentDto>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let mime_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    if mime_type.trim().is_empty() {
        return Err(ServerError(ApiError::validation("image type is required")));
    }
    let attachment = crate::attachments::create(
        &state.pool,
        &state.config.data_dir,
        &thread_id,
        user.user.id.as_str(),
        &mime_type,
        &bytes,
    )
    .await?;
    Ok(Json(attachment))
}

/// Serve a stored image to anyone who can read its thread.
async fn read_attachment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(attachment_id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
    let stored = crate::attachments::stored(&state.pool, &attachment_id).await?;
    authz::require_read(&state.pool, &stored.thread_id, &user).await?;
    let bytes = tokio::fs::read(&stored.path)
        .await
        .map_err(|error| ServerError::internal(format!("could not read attachment: {error}")))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, stored.dto.mime_type),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_owned(),
            ),
        ],
        bytes,
    ))
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
