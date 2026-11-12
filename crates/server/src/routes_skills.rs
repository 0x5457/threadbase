//! Skill discovery route for the composer.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use std::path::Path as FsPath;
use threadbase_shared::SkillsResponse;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/projects/{project_id}/skills", get(list_skills))
}

async fn list_skills(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<SkillsResponse>, ServerError> {
    let project = crate::projects::get(&state.pool, &project_id).await?;
    Ok(Json(SkillsResponse {
        skills: crate::skills::list(FsPath::new(&project.path)),
    }))
}
