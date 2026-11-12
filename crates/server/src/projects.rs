//! Project registry and per-project overview.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use threadbase_shared::api::CreateProjectRequest;
use threadbase_shared::{now_ms, ApiError, ProjectDto};
use threadbase_workspace::git;
use threadbase_workspace::git::GitInfo;
use ulid::Ulid;

#[derive(Debug, Clone, FromRow)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
    pub created_at: i64,
    pub active_threads: i64,
    pub waiting_approvals: i64,
    pub last_activity_at: Option<i64>,
}

/// Validation message for a directory that cannot host a project. Names the
/// actual problem: a repository without commits is not the same as no
/// repository at all.
pub(crate) fn inspect_message(error: &git::GitError) -> &'static str {
    match error {
        git::GitError::NoCommits(_) => {
            "the repository has no commits yet; make an initial commit first"
        }
        _ => "the directory is not a git repository",
    }
}

const PROJECT_SELECT: &str = "
    SELECT p.id, p.name, p.path, p.remote_url, p.default_branch, p.created_at,
      (SELECT COUNT(*) FROM threads t WHERE t.project_id = p.id AND t.archived_at IS NULL) AS active_threads,
      (SELECT COUNT(*) FROM approvals a JOIN threads t ON a.thread_id = t.id
         WHERE t.project_id = p.id AND a.status = 'pending') AS waiting_approvals,
      (SELECT MAX(t.updated_at) FROM threads t WHERE t.project_id = p.id) AS last_activity_at
    FROM projects p
";

pub async fn list(pool: &SqlitePool) -> Result<Vec<ProjectDto>, ServerError> {
    let rows =
        sqlx::query_as::<_, ProjectRow>(&format!("{PROJECT_SELECT} ORDER BY p.updated_at DESC"))
            .fetch_all(pool)
            .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let available = path_available(Path::new(&row.path)).await;
        out.push(to_dto(row, available));
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, project_id: &str) -> Result<ProjectDto, ServerError> {
    let row = row(pool, project_id).await?;
    Ok(to_dto(
        row.clone(),
        path_available(Path::new(&row.path)).await,
    ))
}

pub async fn row(pool: &SqlitePool, project_id: &str) -> Result<ProjectRow, ServerError> {
    sqlx::query_as::<_, ProjectRow>(&format!("{PROJECT_SELECT} WHERE p.id = ?"))
        .bind(project_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("project not found")))
}

pub async fn create(
    pool: &SqlitePool,
    user: &CurrentUser,
    request: &CreateProjectRequest,
) -> Result<ProjectDto, ServerError> {
    let raw = request.path.trim();
    if raw.is_empty() {
        return Err(ServerError(ApiError::validation(
            "project path is required",
        )));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(ServerError(ApiError::validation(
            "project path must be absolute",
        )));
    }
    let info: GitInfo = git::inspect(&path)
        .await
        .map_err(|error| ServerError(ApiError::validation(inspect_message(&error))))?;
    let canonical = info.root.to_string_lossy().to_string();
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM projects WHERE path = ?")
        .bind(&canonical)
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        return Err(ServerError(ApiError::conflict(
            "this project has already been added",
        )));
    }
    let name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            info.root
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "Project".to_owned());
    let id = Ulid::new().to_string();
    let now = now_ms();
    let default_branch = info.branch.clone().unwrap_or_else(|| "main".to_owned());
    sqlx::query(
        "INSERT INTO projects(id, name, path, remote_url, default_branch, created_by, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&canonical)
    .bind(info.remote_url.as_deref())
    .bind(&default_branch)
    .bind(user.user.id.as_str())
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    crate::auth::audit(
        pool,
        Some(user.user.id.as_str()),
        "project.created",
        Some("project"),
        Some(&id),
        Some(&canonical),
        None,
    )
    .await;
    get(pool, &id).await
}

pub async fn can_edit(
    pool: &SqlitePool,
    project_id: &str,
    user: &CurrentUser,
) -> Result<bool, ServerError> {
    if user.is_admin() {
        return Ok(true);
    }
    let created_by: Option<String> =
        sqlx::query_scalar("SELECT created_by FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await?;
    Ok(created_by.as_deref() == Some(user.user.id.as_str()))
}

/// Removes a project with everything Threadbase created for it: every thread
/// row, its worktree on disk and its branch. The repository directory itself is
/// the user's and is never touched.
pub async fn delete(pool: &SqlitePool, project_id: &str) -> Result<(), ServerError> {
    let project = row(pool, project_id).await?;
    let threads: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT id, worktree_path, branch FROM threads WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(pool)
            .await?;
    for (_, worktree, branch) in threads {
        let Some(worktree) = worktree.as_deref() else {
            // Threads without a worktree run on the project branch; deleting
            // that branch would delete the user's own branch.
            continue;
        };
        let path = PathBuf::from(worktree);
        if path.exists() {
            let _ =
                threadbase_workspace::git::remove_worktree(Path::new(&project.path), &path, true)
                    .await;
        }
        let _ =
            threadbase_workspace::git::delete_branch(Path::new(&project.path), &branch, true).await;
    }
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update(
    pool: &SqlitePool,
    project_id: &str,
    name: Option<&str>,
    remote_url: Option<&str>,
    path: Option<&str>,
) -> Result<ProjectDto, ServerError> {
    let current = row(pool, project_id).await?;
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ServerError(ApiError::validation(
                "project name must be between 1 and 80 characters",
            )));
        }
        sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now_ms())
            .bind(project_id)
            .execute(pool)
            .await?;
    }
    if let Some(remote_url) = remote_url {
        sqlx::query("UPDATE projects SET remote_url = ?, updated_at = ? WHERE id = ?")
            .bind(remote_url.trim())
            .bind(now_ms())
            .bind(project_id)
            .execute(pool)
            .await?;
    }
    if let Some(path) = path {
        let raw = path.trim();
        if raw.is_empty() {
            return Err(ServerError(ApiError::validation(
                "project path is required",
            )));
        }
        let path_buf = PathBuf::from(raw);
        if !path_buf.is_absolute() {
            return Err(ServerError(ApiError::validation(
                "project path must be absolute",
            )));
        }
        let info = git::inspect(&path_buf)
            .await
            .map_err(|error| ServerError(ApiError::validation(inspect_message(&error))))?;
        let canonical = info.root.to_string_lossy().to_string();
        let duplicate: Option<String> =
            sqlx::query_scalar("SELECT id FROM projects WHERE path = ? AND id != ?")
                .bind(&canonical)
                .bind(project_id)
                .fetch_optional(pool)
                .await?;
        if duplicate.is_some() {
            return Err(ServerError(ApiError::conflict(
                "this project has already been added",
            )));
        }
        sqlx::query(
            "UPDATE projects SET path = ?, default_branch = ?, remote_url = COALESCE(?, remote_url), updated_at = ? WHERE id = ?",
        )
        .bind(&canonical)
        .bind(info.branch.clone().unwrap_or_else(|| current.default_branch.clone()))
        .bind(info.remote_url.as_deref())
        .bind(now_ms())
        .bind(project_id)
        .execute(pool)
        .await?;
        crate::auth::audit(
            pool,
            None,
            "project.located",
            Some("project"),
            Some(project_id),
            Some(&canonical),
            None,
        )
        .await;
    }
    get(pool, project_id).await
}

async fn path_available(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

#[must_use]
pub fn to_dto(row: ProjectRow, available: bool) -> ProjectDto {
    ProjectDto {
        id: row.id.into(),
        name: row.name,
        path: row.path,
        remote_url: row.remote_url,
        default_branch: row.default_branch,
        active_threads: u32::try_from(row.active_threads).unwrap_or(0),
        waiting_approvals: u32::try_from(row.waiting_approvals).unwrap_or(0),
        last_activity_at: row.last_activity_at,
        available,
        created_at: row.created_at,
    }
}
