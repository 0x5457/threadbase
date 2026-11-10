//! Comment routes.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::Json;
use axum::Router;
use threadbase_shared::api::{CreateCommentRequest, UpdateCommentRequest};
use threadbase_shared::{ApiError, CommentResponse, CommentsResponse, OkResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/threads/{thread_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/api/comments/{comment_id}",
            patch(update_comment).delete(delete_comment),
        )
        .route("/api/comments/{comment_id}/send", post(send_comment))
}

async fn list_comments(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<CommentsResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(CommentsResponse {
        comments: crate::comments::list_for_thread(&state.pool, &thread_id).await?,
    }))
}

async fn create_comment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<CreateCommentRequest>,
) -> Result<Json<CommentResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let comment = crate::comments::create(
        &state.pool,
        &state.state_store,
        &state.events,
        &thread_id,
        &user,
        &request,
    )
    .await?;
    if request.send {
        if let Err(error) = state
            .agents
            .send(&thread_id, &user, comment.body.as_str(), None)
            .await
        {
            tracing::warn!(%error, "comment sent but agent did not accept it");
        }
    }
    Ok(Json(CommentResponse { comment }))
}

async fn update_comment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(comment_id): Path<String>,
    Json(request): Json<UpdateCommentRequest>,
) -> Result<Json<CommentResponse>, ServerError> {
    Ok(Json(CommentResponse {
        comment: crate::comments::update(
            &state.pool,
            &state.state_store,
            &state.events,
            &comment_id,
            &user,
            &request,
        )
        .await?,
    }))
}

async fn delete_comment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(comment_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    crate::comments::delete(&state.pool, &comment_id, &user).await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn send_comment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(comment_id): Path<String>,
) -> Result<Json<CommentResponse>, ServerError> {
    let comment = crate::comments::load(&state.pool, &comment_id).await?;
    authz::require_write_role(&state.pool, comment.thread_id.as_str(), &user).await?;
    if comment.status == threadbase_shared::CommentStatus::Delivered {
        return Err(ServerError(ApiError::conflict("comment already delivered")));
    }
    state
        .agents
        .send(
            comment.thread_id.as_str(),
            &user,
            comment.body.as_str(),
            None,
        )
        .await
        .map_err(|error| {
            ServerError(ApiError::new(
                threadbase_shared::ErrorCode::AgentUnavailable,
                error.to_string(),
            ))
        })?;
    Ok(Json(CommentResponse {
        comment: crate::comments::mark_delivered(
            &state.pool,
            &state.state_store,
            &state.events,
            &comment_id,
            &user,
            state.events.last_seq(&comment.thread_id).await.ok(),
        )
        .await?,
    }))
}
