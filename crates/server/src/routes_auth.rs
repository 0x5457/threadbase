//! Authentication endpoints: status, current user, logout and administrator
//! user management.
//!
//! Sign-in itself is a GitHub OAuth redirect implemented in `auth`; it ends in
//! an `HttpOnly` session cookie. There is no password endpoint and no identity
//! header to spoof.

use crate::app::AppState;
use crate::auth::{self, CurrentUser};
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{AuthMeDto, AuthStatusResponse, UserListResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(auth::github_router())
        .route("/api/auth/status", get(status))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/users", get(list_users))
        .route("/api/auth/users/{user_id}", patch(update_user))
}

async fn status(State(state): State<AppState>) -> Result<Json<AuthStatusResponse>, ServerError> {
    Ok(Json(
        auth::auth_status(
            &state.pool,
            state.config.github_ready(),
            state.config.dev_identity.is_some(),
        )
        .await?,
    ))
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<AuthMeDto>, ServerError> {
    Ok(Json(auth::me(&state.pool, &user).await?))
}

/// Clears the session even when it already expired, so a stale cookie cannot
/// trap the browser in a sign-in loop.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = auth_session::read_cookie(&headers, auth_session::COOKIE) {
        if let Err(error) = auth_session::revoke(&state.pool, &token).await {
            tracing::warn!(%error, "failed to revoke session");
        }
    }
    let cleared = auth_session::clear_cookie(auth_session::COOKIE, state.config.cookie_secure);
    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&cleared).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    (StatusCode::NO_CONTENT, headers).into_response()
}

async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<UserListResponse>, ServerError> {
    user.require_admin()?;
    Ok(Json(UserListResponse {
        users: auth::list_users(&state.pool).await?,
    }))
}

async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(target_id): axum::extract::Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserListResponse>, ServerError> {
    user.require_admin()?;
    let updated =
        auth::update_user(&state.pool, user.user.id.as_str(), &target_id, &request).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "user.access_changed",
        Some("user"),
        Some(updated.id.as_str()),
        Some(&format!(
            "admin={} access={}",
            request.is_admin,
            request.access.as_str()
        )),
        None,
    )
    .await;
    Ok(Json(UserListResponse {
        users: auth::list_users(&state.pool).await?,
    }))
}
