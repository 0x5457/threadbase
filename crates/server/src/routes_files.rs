//! Read-only worktree file browsing.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use std::path::PathBuf;
use threadbase_shared::{ApiError, FileContentResponseDto, FileSearchResponse, FilesResponseDto};

/// A directory listing is a page, not the whole filesystem. Huge generated
/// folders otherwise ship megabytes of JSON and make the client freeze before
/// virtualization can help.
const MAX_DIRECTORY_ENTRIES: usize = 5_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/files", get(list_files))
        .route("/api/threads/{thread_id}/files/search", get(search_files))
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
    let mut entries = threadbase_workspace::files::list(&root, &path)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    let truncated = entries.len() > MAX_DIRECTORY_ENTRIES;
    entries.truncate(MAX_DIRECTORY_ENTRIES);
    Ok(Json(FilesResponseDto {
        path,
        entries,
        truncated,
    }))
}

#[derive(Debug, Deserialize)]
struct FileSearchQuery {
    q: Option<String>,
    limit: Option<usize>,
}

async fn search_files(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<FileSearchResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = crate::threads::load_row(&state.pool, &thread_id).await?;
    let root = PathBuf::from(
        row.worktree_path
            .ok_or_else(|| ServerError(ApiError::conflict("thread worktree is not available")))?,
    );
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let files =
        threadbase_workspace::files::search(&root, query.q.as_deref().unwrap_or_default(), limit)
            .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    Ok(Json(FileSearchResponse { files }))
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
