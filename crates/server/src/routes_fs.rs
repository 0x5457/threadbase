//! Local directory browsing for the project picker.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use threadbase_shared::{ApiError, DirectoryEntryDto, DirectoryListingDto};

pub fn router() -> Router<crate::app::AppState> {
    Router::new().route("/api/fs/directories", get(list_directories))
}

#[derive(Debug, Deserialize)]
struct DirectoryQuery {
    path: Option<String>,
}

/// Directories the server may browse and create projects in: the user's home
/// directory plus every registered project root.
pub async fn allowed_roots(state: &crate::app::AppState) -> Result<Vec<PathBuf>, ServerError> {
    let home_root = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.canonicalize().unwrap_or(path))
        .or_else(|| state.config.data_dir.parent().map(Path::to_path_buf));
    let mut roots: Vec<PathBuf> = home_root.into_iter().collect();
    let project_paths: Vec<String> = sqlx::query_scalar("SELECT path FROM projects")
        .fetch_all(&state.pool)
        .await?;
    roots.extend(project_paths.into_iter().map(PathBuf::from));
    Ok(roots)
}

async fn list_directories(
    State(state): State<crate::app::AppState>,
    _user: CurrentUser,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<DirectoryListingDto>, ServerError> {
    let roots = allowed_roots(&state).await?;
    let home_root = roots.first().cloned();
    let requested = query
        .path
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            home_root
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    if requested.is_empty() || requested.contains('\0') {
        return Err(ServerError(ApiError::validation(
            "directory is not available",
        )));
    }
    let path = PathBuf::from(&requested);
    let path = if path.is_file() {
        path.parent().map(Path::to_path_buf).unwrap_or(path)
    } else {
        path
    };
    let path = threadbase_workspace::path::ensure_within_any(&roots, &path)
        .map_err(|_| ServerError(ApiError::validation("directory is not available")))?;
    let path = path
        .canonicalize()
        .map_err(|_| ServerError(ApiError::validation("directory does not exist")))?;
    let is_git = path.join(".git").exists();
    let mut entries = Vec::new();
    let reader = std::fs::read_dir(&path)
        .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | "dist") {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let child = entry.path();
        entries.push(DirectoryEntryDto {
            name,
            path: child.to_string_lossy().to_string(),
            is_git: child.join(".git").exists(),
        });
        if entries.len() >= 200 {
            break;
        }
    }
    entries.sort_by(|left, right| {
        right
            .is_git
            .cmp(&left.is_git)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(Json(DirectoryListingDto {
        path: path.to_string_lossy().to_string(),
        parent: path
            .parent()
            .map(|parent| parent.to_string_lossy().to_string()),
        is_git,
        entries,
    }))
}
