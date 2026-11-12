//! Thread lifecycle: creation, worktree mutations, transitions and deletion.

use super::query::{get, load_row};
use crate::auth::{audit, CurrentUser};
use crate::authz;
use crate::bindings;
use crate::error::ServerError;
use crate::projection::{self, Projection};
use crate::projects;
use crate::queue;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use threadbase_core::state::{ThreadAction, ThreadMachine};
use threadbase_shared::api::{CreateThreadRequest, ForkThreadRequest, UpdateThreadRequest};
use threadbase_shared::{now_ms, ApiError, ThreadDto, ThreadStatus};
use threadbase_workspace::git;
use ulid::Ulid;

pub struct NewThread<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub parent_thread_id: Option<&'a str>,
    pub title: &'a str,
    pub agent: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub branch: &'a str,
    pub base_branch: &'a str,
    pub base_sha: &'a str,
    pub worktree_path: &'a str,
    pub created_by: &'a str,
    pub created_at: i64,
}

pub async fn status(pool: &SqlitePool, thread_id: &str) -> Result<ThreadStatus, ServerError> {
    projection::thread_status(pool, thread_id).await
}

/// Validate and persist a lifecycle transition, then publish the projection.
pub async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    action: ThreadAction,
) -> Result<ThreadStatus, ServerError> {
    let current = status(pool, thread_id).await?;
    let next = ThreadMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    sqlx::query("UPDATE threads SET status = ?, updated_at = ? WHERE id = ?")
        .bind(threadbase_core::state::thread_status_str(next))
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

async fn insert(pool: &SqlitePool, row: &NewThread<'_>) -> Result<ThreadStatus, ServerError> {
    let status = ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO threads(id, project_id, parent_thread_id, title, status, agent, model, effort,
            branch, base_branch, base_sha, last_turn_sha, worktree_path, created_by, created_at,
            updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.id)
    .bind(row.project_id)
    .bind(row.parent_thread_id)
    .bind(row.title)
    .bind(threadbase_core::state::thread_status_str(status))
    .bind(row.agent)
    .bind(row.model)
    .bind(row.effort)
    .bind(row.branch)
    .bind(row.base_branch)
    .bind(row.base_sha)
    .bind(row.base_sha)
    .bind(row.worktree_path)
    .bind(row.created_by)
    .bind(row.created_at)
    .bind(row.created_at)
    .execute(pool)
    .await?;
    Ok(status)
}

pub async fn create(
    pool: &SqlitePool,
    projection: &Projection,
    data_dir: &Path,
    user: &CurrentUser,
    project_id: &str,
    request: &CreateThreadRequest,
) -> Result<ThreadDto, ServerError> {
    let project = projects::row(pool, project_id).await?;
    let project_root = PathBuf::from(&project.path);
    if git::inspect(&project_root).await.is_err() {
        return Err(ServerError(ApiError::validation(
            "project path is no longer a git repository",
        )));
    }
    let base_branch = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&project.default_branch)
        .to_owned();
    let base_sha = resolve_base(&project_root, &base_branch).await?;
    let agent = request
        .agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("codex")
        .to_owned();
    if agent.len() > 40 {
        return Err(ServerError(ApiError::validation("agent id is too long")));
    }
    let thread_id = Ulid::new().to_string();
    let branch = format!("threadbase/{thread_id}");
    let worktree = data_dir.join("worktrees").join(&thread_id);
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "New thread".to_owned());
    let now = now_ms();
    let worktree_path = worktree.to_string_lossy().to_string();
    insert(
        pool,
        &NewThread {
            id: &thread_id,
            project_id,
            parent_thread_id: None,
            title: &title,
            agent: &agent,
            model: request.model.as_deref(),
            effort: request.effort.as_deref(),
            branch: &branch,
            base_branch: &base_branch,
            base_sha: &base_sha,
            worktree_path: &worktree_path,
            created_by: user.user.id.as_str(),
            created_at: now,
        },
    )
    .await?;

    if let Err(error) = git::create_worktree(&project_root, &worktree, &branch, &base_sha).await {
        sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(&thread_id)
            .execute(pool)
            .await?;
        let _ = tokio::fs::remove_dir_all(&worktree).await;
        return Err(ServerError(ApiError::validation(format!(
            "could not create worktree: {error}"
        ))));
    }

    bindings::insert(pool, &thread_id, &agent, &worktree_path, now).await?;
    transition(pool, projection, &thread_id, ThreadAction::StartSucceeded).await?;

    if let Some(prompt) = request
        .initial_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        queue::enqueue(
            pool,
            queue::NewQueuedInput {
                thread_id: &thread_id,
                author_id: user.user.id.as_str(),
                text: prompt,
                authors_json: None,
                created_at: now,
            },
        )
        .await?;
    }
    audit(
        pool,
        Some(user.user.id.as_str()),
        "thread.created",
        Some("thread"),
        Some(&thread_id),
        Some(&project.path),
        None,
    )
    .await;
    get(pool, &thread_id, user).await
}

pub async fn update(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    user: &CurrentUser,
    request: &UpdateThreadRequest,
) -> Result<ThreadDto, ServerError> {
    if request.title.is_some() || request.pinned.is_some() {
        authz::require_write_role(pool, thread_id, user).await?;
    }
    if request.archived.is_some() {
        authz::require_manage_role(pool, thread_id, user).await?;
    }
    if let Some(title) = request.title.as_deref() {
        let title = title.trim();
        if title.is_empty() || title.len() > 160 {
            return Err(ServerError(ApiError::validation(
                "thread title must be between 1 and 160 characters",
            )));
        }
        sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title)
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    if let Some(pinned) = request.pinned {
        sqlx::query("UPDATE threads SET pinned = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(pinned))
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    if let Some(archived) = request.archived {
        let archived_at = if archived { Some(now_ms()) } else { None };
        sqlx::query("UPDATE threads SET archived_at = ?, updated_at = ? WHERE id = ?")
            .bind(archived_at)
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
        let action = if archived {
            ThreadAction::Archive
        } else {
            ThreadAction::Unarchive
        };
        transition(pool, projection, thread_id, action).await?;
    }
    for (column, value) in [
        ("model", request.model.as_deref()),
        ("effort", request.effort.as_deref()),
    ] {
        if let Some(value) = value {
            let value = value.trim();
            let value = (!value.is_empty()).then_some(value);
            sqlx::query(&format!(
                "UPDATE threads SET {column} = ?, updated_at = ? WHERE id = ?"
            ))
            .bind(value)
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
        }
    }
    get(pool, thread_id, user).await
}

pub async fn fork(
    pool: &SqlitePool,
    projection: &Projection,
    data_dir: &Path,
    thread_id: &str,
    user: &CurrentUser,
    request: &ForkThreadRequest,
) -> Result<ThreadDto, ServerError> {
    authz::require_write_role(pool, thread_id, user).await?;
    let source = load_row(pool, thread_id).await?;
    let project = projects::row(pool, &source.project_id).await?;
    let project_root = PathBuf::from(&project.path);
    let source_worktree = source.worktree_path.as_deref().map(PathBuf::from);
    let head = match source_worktree.as_deref() {
        Some(path) => git::head_sha(path)
            .await
            .unwrap_or_else(|_| source.base_sha.clone().unwrap_or_default()),
        None => source.base_sha.clone().unwrap_or_default(),
    };
    if head.is_empty() {
        return Err(ServerError(ApiError::validation(
            "cannot fork a thread without a base commit",
        )));
    }
    let new_id = Ulid::new().to_string();
    let branch = format!("threadbase/{new_id}");
    let worktree = data_dir.join("worktrees").join(&new_id);
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{} (fork)", source.title));
    let now = now_ms();
    let worktree_path = worktree.to_string_lossy().to_string();
    insert(
        pool,
        &NewThread {
            id: &new_id,
            project_id: &source.project_id,
            parent_thread_id: Some(&source.id),
            title: &title,
            agent: &source.agent,
            model: source.model.as_deref(),
            effort: source.effort.as_deref(),
            branch: &branch,
            base_branch: &source.branch,
            base_sha: &head,
            worktree_path: &worktree_path,
            created_by: user.user.id.as_str(),
            created_at: now,
        },
    )
    .await?;
    if let Err(error) = git::create_worktree(&project_root, &worktree, &branch, &head).await {
        sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(&new_id)
            .execute(pool)
            .await?;
        let _ = tokio::fs::remove_dir_all(&worktree).await;
        return Err(ServerError(ApiError::validation(format!(
            "could not create fork worktree: {error}"
        ))));
    }
    if let Some(source_worktree) = source_worktree.as_deref() {
        if let Ok(patch) = git::worktree_patch(source_worktree, None).await {
            if let Err(error) = git::apply_patch(&worktree, &patch).await {
                let _ = git::remove_worktree(&project_root, &worktree, true).await;
                sqlx::query("DELETE FROM threads WHERE id = ?")
                    .bind(&new_id)
                    .execute(pool)
                    .await?;
                return Err(ServerError(ApiError::validation(format!(
                    "could not copy uncommitted changes into the fork: {error}"
                ))));
            }
        }
    }
    bindings::copy(pool, &new_id, &worktree_path, &source.id, now).await?;
    transition(pool, projection, &new_id, ThreadAction::StartSucceeded).await?;
    audit(
        pool,
        Some(user.user.id.as_str()),
        "thread.forked",
        Some("thread"),
        Some(&new_id),
        Some(&source.id),
        None,
    )
    .await;
    get(pool, &new_id, user).await
}

pub async fn delete(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<(), ServerError> {
    authz::require_manage_role(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    let project = projects::row(pool, &row.project_id).await?;
    if let Some(worktree) = row.worktree_path.as_deref() {
        let path = PathBuf::from(worktree);
        if path.exists() {
            git::remove_worktree(Path::new(&project.path), &path, true)
                .await
                .map_err(|error| {
                    ServerError(ApiError::conflict(format!(
                        "could not remove worktree: {error}"
                    )))
                })?;
        }
    }
    let _ = git::delete_branch(Path::new(&project.path), &row.branch, true).await;
    sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(thread_id)
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(user.user.id.as_str()),
        "thread.deleted",
        Some("thread"),
        Some(thread_id),
        Some(&row.title),
        None,
    )
    .await;
    Ok(())
}

async fn resolve_base(project_root: &Path, base_branch: &str) -> Result<String, ServerError> {
    if let Ok(sha) = git::rev_parse(project_root, base_branch).await {
        return Ok(sha);
    }
    if let Ok(sha) = git::rev_parse(project_root, &format!("origin/{base_branch}")).await {
        return Ok(sha);
    }
    Err(ServerError(ApiError::validation(format!(
        "base branch `{base_branch}` was not found"
    ))))
}
