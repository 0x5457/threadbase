//! Workspace state, settings and search routes.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::{get, patch};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use threadbase_shared::api::SettingsPatch;
use threadbase_shared::{SearchResponseDto, StateResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/settings", patch(update_settings))
        .route("/api/search", get(search))
}

async fn state(
    State(app): State<AppState>,
    user: CurrentUser,
) -> Result<Json<StateResponse>, ServerError> {
    Ok(Json(StateResponse {
        settings: crate::settings::load(&app.pool, &user).await?,
        user: user.user,
    }))
}

async fn update_settings(
    State(app): State<AppState>,
    user: CurrentUser,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<StateResponse>, ServerError> {
    Ok(Json(
        crate::settings::update(&app.pool, &user, patch).await?,
    ))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

async fn search(
    State(app): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponseDto>, ServerError> {
    Ok(Json(SearchResponseDto {
        results: crate::search::search(&app.pool, &user, query.q.as_deref().unwrap_or("")).await?,
    }))
}
