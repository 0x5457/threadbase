//! The MCP endpoint and token management.
//!
//! `/mcp` authenticates with a bearer token and never reads a cookie, so a
//! browser page cannot drive a thread through it. Tokens are created by a
//! signed-in person on the settings page.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::{auth, mcp};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::Value;
use threadbase_shared::api::{CreateMcpTokenRequest, CreatedMcpToken, McpTokensResponse};

pub fn router(enabled: bool) -> Router<AppState> {
    // The guard is a layer, not a check inside each handler: axum parses the body
    // before the handler runs, so an inner check would answer 422 (or 200 with
    // HTML through the SPA fallback) and reveal that the endpoint exists.
    Router::new()
        .route("/mcp", post(handle))
        .route("/api/mcp/tokens", get(list_tokens).post(create_token))
        .route("/api/mcp/tokens/{token_id}", delete(revoke_token))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if enabled {
                    return next.run(request).await;
                }
                let body = Json(serde_json::json!({
                    "error": { "code": "not_found", "message": "not found" }
                }));
                (StatusCode::NOT_FOUND, body).into_response()
            },
        ))
}

async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, ServerError> {
    let principal = mcp::authenticate(&state.pool, &headers).await?;
    // Clients may send one message or a batch; answer in the same shape.
    if let Some(batch) = payload.as_array() {
        let mut replies = Vec::new();
        for message in batch {
            if let Some(reply) = mcp::handle(&state, &principal, message).await {
                replies.push(reply);
            }
        }
        if replies.is_empty() {
            return Ok((StatusCode::ACCEPTED, Json(Value::Null)));
        }
        return Ok((StatusCode::OK, Json(Value::Array(replies))));
    }
    match mcp::handle(&state, &principal, &payload).await {
        Some(reply) => Ok((StatusCode::OK, Json(reply))),
        None => Ok((StatusCode::ACCEPTED, Json(Value::Null))),
    }
}

async fn list_tokens(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<McpTokensResponse>, ServerError> {
    Ok(Json(McpTokensResponse {
        tokens: mcp::list(&state.pool, user.user.id.as_str()).await?,
    }))
}

async fn create_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateMcpTokenRequest>,
) -> Result<Json<CreatedMcpToken>, ServerError> {
    let token = mcp::mint(&state.pool, &user, &request.label, &request.scopes).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "mcp.token_created",
        Some("mcp_token"),
        None,
        Some(&request.label),
        None,
    )
    .await;
    let origin = state
        .config
        .public_origin
        .clone()
        .unwrap_or_else(|| format!("http://{}", state.config.bind));
    Ok(Json(CreatedMcpToken {
        token,
        endpoint: format!("{origin}/mcp"),
    }))
}

async fn revoke_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(token_id): Path<String>,
) -> Result<Json<McpTokensResponse>, ServerError> {
    mcp::revoke(&state.pool, user.user.id.as_str(), &token_id).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "mcp.token_revoked",
        Some("mcp_token"),
        Some(&token_id),
        None,
        None,
    )
    .await;
    Ok(Json(McpTokensResponse {
        tokens: mcp::list(&state.pool, user.user.id.as_str()).await?,
    }))
}
