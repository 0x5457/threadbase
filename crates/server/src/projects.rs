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
    pub verify_command: Option<String>,
    pub setup_command: Option<String>,
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
    SELECT p.id, p.name, p.path, p.remote_url, p.default_branch, p.verify_command,
      p.setup_command, p.created_at,
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
    let path = project_path(request.path.trim())?;
    register(pool, user, &path, request.name.as_deref()).await
}

/// Create the project directory and initialize a repository in it (or finish
/// initializing an existing empty one), then register it. `roots` are the
/// directories the server may write in, mirroring the directory picker.
pub async fn create_repository(
    pool: &SqlitePool,
    user: &CurrentUser,
    request: &CreateProjectRequest,
    roots: &[PathBuf],
) -> Result<ProjectDto, ServerError> {
    let path = project_path(request.path.trim())?;
    let target = prepare_repository(&path, request.name.as_deref(), roots).await?;
    register(pool, user, &target, request.name.as_deref()).await
}

/// Validated path shared by both creation paths.
fn project_path(raw: &str) -> Result<PathBuf, ServerError> {
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
    Ok(path)
}

/// Make `path` a repository Threadbase can use. The directory is created when
/// it is missing; an empty directory or a repository without commits is
/// initialized in place. Anything else is refused before touching it.
async fn prepare_repository(
    path: &Path,
    name: Option<&str>,
    roots: &[PathBuf],
) -> Result<PathBuf, ServerError> {
    let folder = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('.'))
        .ok_or_else(|| {
            ServerError(ApiError::validation(
                "project directory name is required and cannot start with a dot",
            ))
        })?;
    let parent = path
        .parent()
        .ok_or_else(|| ServerError(ApiError::validation("project path is not valid")))?;
    let parent = threadbase_workspace::path::ensure_within_any(roots, parent)
        .map_err(|_| ServerError(ApiError::validation("directory is not available")))?;
    let parent = parent
        .canonicalize()
        .map_err(|_| ServerError(ApiError::validation("parent directory does not exist")))?;
    let target = parent.join(folder);
    let mut created: Vec<PathBuf> = Vec::new();

    match tokio::fs::symlink_metadata(&target).await {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(ServerError(ApiError::validation(
                "project path exists and is not a directory",
            )));
        }
        Ok(_) => {
            let mut entries = tokio::fs::read_dir(&target).await.map_err(|error| {
                ServerError(ApiError::validation(format!(
                    "could not read the directory: {error}"
                )))
            })?;
            while let Some(entry) = entries.next_entry().await.map_err(|error| {
                ServerError(ApiError::validation(format!(
                    "could not read the directory: {error}"
                )))
            })? {
                if entry.file_name().to_str() == Some(".git") {
                    continue;
                }
                return Err(ServerError(ApiError::validation(
                    "the directory is not empty; create the project elsewhere or add it as it is",
                )));
            }
            if target.join(".git").exists() {
                match git::inspect(&target).await {
                    Ok(_) => {
                        return Err(ServerError(ApiError::validation(
                            "this directory is already a git repository; add it instead",
                        )))
                    }
                    // A repository without commits is exactly what init fixes.
                    Err(git::GitError::NoCommits(_)) => {}
                    // A broken repository is reinitialized below.
                    Err(_) => {}
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir(&target).await.map_err(|error| {
                ServerError(ApiError::validation(format!(
                    "could not create the directory: {error}"
                )))
            })?;
            created.push(target.clone());
        }
        Err(error) => {
            return Err(ServerError(ApiError::validation(format!(
                "could not inspect the directory: {error}"
            ))));
        }
    }

    let heading = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| folder.to_owned());
    if let Err(error) = initialize_repository(&target, &heading, &mut created).await {
        discard(&created).await;
        return Err(error);
    }
    target.canonicalize().map_err(|error| {
        ServerError(ApiError::validation(format!(
            "could not resolve the project path: {error}"
        )))
    })
}

/// `created` records what this call added so a failure can be undone without
/// touching anything the user already had.
async fn initialize_repository(
    target: &Path,
    heading: &str,
    created: &mut Vec<PathBuf>,
) -> Result<(), ServerError> {
    if !target.join(".git").exists() {
        git::init_repository(target)
            .await
            .map_err(|error| ServerError(ApiError::validation(error.to_string())))?;
        created.push(target.join(".git"));
    }
    let readme = target.join("README.md");
    if !readme.exists() {
        tokio::fs::write(&readme, format!("# {heading}\n"))
            .await
            .map_err(|error| {
                ServerError(ApiError::validation(format!(
                    "could not write README.md: {error}"
                )))
            })?;
        created.push(readme);
    }
    git::commit_all(target, "Initial commit")
        .await
        .map(|_| ())
        .map_err(|error| {
            ServerError(ApiError::validation(format!(
                "could not create the initial commit: {error}"
            )))
        })
}

async fn discard(created: &[PathBuf]) {
    for path in created.iter().rev() {
        if path.is_dir() {
            let _ = tokio::fs::remove_dir_all(path).await;
        } else {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

async fn register(
    pool: &SqlitePool,
    user: &CurrentUser,
    path: &Path,
    requested_name: Option<&str>,
) -> Result<ProjectDto, ServerError> {
    let info: GitInfo = git::inspect(path)
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
    let name = requested_name
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
    for (thread_id, worktree, branch) in threads {
        let _ = crate::search::remove_thread(pool, &thread_id).await;
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
    verify_command: Option<&str>,
    setup_command: Option<&str>,
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
    for (column, command) in [
        ("verify_command", verify_command),
        ("setup_command", setup_command),
    ] {
        if let Some(command) = command {
            let command = command.trim();
            let value = (!command.is_empty()).then_some(command);
            sqlx::query(&format!(
                "UPDATE projects SET {column} = ?, updated_at = ? WHERE id = ?"
            ))
            .bind(value)
            .bind(now_ms())
            .bind(project_id)
            .execute(pool)
            .await?;
        }
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
        verify_command: row.verify_command,
        setup_command: row.setup_command,
        active_threads: u32::try_from(row.active_threads).unwrap_or(0),
        waiting_approvals: u32::try_from(row.waiting_approvals).unwrap_or(0),
        last_activity_at: row.last_activity_at,
        available,
        created_at: row.created_at,
    }
}
