//! Workspace state, settings and search routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::State;
use axum::routing::{get, patch};
use axum::Json;
use axum::Router;
use threadbase_shared::api::SettingsPatch;
use threadbase_shared::{MachineSpec, StateResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/machines", get(machines))
        .route("/api/settings", patch(update_settings))
}

/// The lifecycle catalogue. Unauthenticated on purpose: it describes the
/// product, not any user's data, and agent tooling should be able to read it
/// before a session exists.
async fn machines() -> Json<Vec<MachineSpec>> {
    Json(threadbase_core::state::catalogue())
}

async fn state(
    State(app): State<AppState>,
    user: CurrentUser,
) -> Result<Json<StateResponse>, ServerError> {
    Ok(Json(StateResponse {
        settings: crate::settings::load(&app.pool, &user).await?,
        user: user.user,
        terminal: app.config.terminal,
    }))
}

async fn update_settings(
    State(app): State<AppState>,
    user: CurrentUser,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<StateResponse>, ServerError> {
    Ok(Json(
        crate::settings::update(&app.pool, &user, patch, app.config.terminal).await?,
    ))
}
