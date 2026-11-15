//! Full-text search route.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use threadbase_shared::api::SearchResponse;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/search", get(search))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    project_id: Option<String>,
    limit: Option<usize>,
}

async fn search(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ServerError> {
    Ok(Json(
        crate::search::search(
            &state.pool,
            query.q.as_deref().unwrap_or_default(),
            query.project_id.as_deref(),
            query.limit.unwrap_or(20),
        )
        .await?,
    ))
}
