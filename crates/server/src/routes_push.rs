//! Web Push subscription endpoints.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::State;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use threadbase_shared::api::{PushSubscriptionRequest, PushUnsubscribeRequest};
use threadbase_shared::{OkResponse, PushConfigResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/push/config", get(push_config))
        .route("/api/push/subscribe", post(subscribe))
        .route("/api/push/unsubscribe", post(unsubscribe))
}

async fn push_config(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<PushConfigResponse>, ServerError> {
    Ok(Json(state.push.config().await))
}

async fn subscribe(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<PushSubscriptionRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    state
        .push
        .subscribe(user.user.id.as_str(), &request)
        .await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn unsubscribe(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<PushUnsubscribeRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    state
        .push
        .unsubscribe(user.user.id.as_str(), &request)
        .await?;
    Ok(Json(OkResponse { ok: true }))
}
