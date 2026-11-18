//! Authentication endpoints: status, the tailnet exchange, current user and
//! administrator user management.
//!
//! Sign-in is one POST: `tailscale serve` proved who the browser is, and the
//! server trades that for an `HttpOnly` session cookie. There is no password, no
//! OAuth app and no login page.

use crate::app::AppState;
use crate::auth::{self, CurrentUser};
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use std::net::SocketAddr;
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{AuthMeDto, AuthStatusResponse, UserListResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(status))
        .route("/api/auth/tailscale", post(tailscale_sign_in))
        .route("/api/auth/me", get(me))
        .route("/api/auth/users", get(list_users))
        .route("/api/auth/users/{user_id}", patch(update_user))
}

async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
) -> Result<Json<AuthStatusResponse>, ServerError> {
    let presentment = auth::tailnet_present(&headers, Some(peer.0));
    Ok(Json(
        auth::auth_status(
            &state.pool,
            presentment
                .as_ref()
                .map(|presentment| presentment.identity.login.as_str()),
            state.config.dev_identity.is_some(),
        )
        .await?,
    ))
}

/// Exchange the tailnet identity `tailscale serve` proved for a session cookie.
///
/// The identity is checked against `tailscale whois` whenever the proxy forwards
/// the client address, so a local process cannot promote itself by sending the
/// three headers with a spoofed address.
async fn tailscale_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: ConnectInfo<SocketAddr>,
) -> Result<Response, ServerError> {
    let presentment = auth::tailnet_present(&headers, Some(peer.0)).ok_or_else(|| {
        ServerError(threadbase_shared::ApiError::unauthorized(
            "this request did not arrive through tailscale serve",
        ))
    })?;
    if let Some(client) = presentment.client {
        match auth::tailnet_whois_agrees(client, &presentment.identity.login) {
            Some(true) => {}
            Some(false) => {
                tracing::warn!(
                    %client,
                    claimed = %presentment.identity.login,
                    "refused a tailnet identity that whois attributes to someone else"
                );
                return Err(ServerError(threadbase_shared::ApiError::forbidden(
                    "that tailnet address belongs to a different account",
                )));
            }
            None => {
                return Err(ServerError(threadbase_shared::ApiError::forbidden(
                    "this request did not come from a tailnet peer",
                )))
            }
        }
    }
    let user = auth::ensure_identity_user(&state.pool, &presentment.identity, &state.config.admins)
        .await?;
    let token = auth_session::create(
        &state.pool,
        user.id.as_str(),
        auth::METHOD_TAILSCALE,
        Some(&auth::client_ip_for_headers(&headers, Some(peer.0))),
        headers
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        state.config.session_seconds,
    )
    .await?;
    let cookie = auth_session::cookie_header(
        auth_session::COOKIE,
        &token,
        state.config.cookie_secure,
        state.config.session_seconds,
    );
    let mut response_headers = HeaderMap::new();
    response_headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    Ok((
        response_headers,
        Json(
            auth::me(
                &state.pool,
                &auth::CurrentUser {
                    user,
                    method: auth::METHOD_TAILSCALE,
                },
            )
            .await?,
        ),
    )
        .into_response())
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<AuthMeDto>, ServerError> {
    Ok(Json(auth::me(&state.pool, &user).await?))
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
