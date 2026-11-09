//! Read-only worktree file browsing.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use std::path::PathBuf;
use threadbase_shared::{ApiError, FileContentResponseDto, FilesResponseDto};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/files", get(list_files))
        .route("/api/threads/{thread_id}/file", get(read_file))
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    path: Option<String>,
}

async fn list_files(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FilesResponseDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = crate::threads::load_row(&state.pool, &thread_id).await?;
    let root = PathBuf::from(
        row.worktree_path
            .ok_or_else(|| ServerError(ApiError::conflict("thread worktree is not available")))?,
    );
    let path = query.path.unwrap_or_default();
    let entries = threadbase_workspace::files::list(&root, &path)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    Ok(Json(FilesResponseDto { path, entries }))
}

async fn read_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileContentResponseDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = crate::threads::load_row(&state.pool, &thread_id).await?;
    let root = PathBuf::from(
        row.worktree_path
            .ok_or_else(|| ServerError(ApiError::conflict("thread worktree is not available")))?,
    );
    let path = query
        .path
        .ok_or_else(|| ServerError(ApiError::validation("file path is required")))?;
    let file = threadbase_workspace::files::read(&root, &path)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    Ok(Json(FileContentResponseDto { file }))
}
