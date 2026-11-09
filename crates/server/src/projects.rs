//! Project registry and per-project overview.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use threadbase_shared::api::CreateProjectRequest;
use threadbase_shared::{
    now_ms, ActivityDto, ApiError, ChangeStats, ProjectDto, ProjectOverviewDto,
};
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
    let info: GitInfo = git::inspect(&path).await.map_err(|_| {
        ServerError(ApiError::validation(
            "the directory is not a git repository",
        ))
    })?;
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
        let info = git::inspect(&path_buf).await.map_err(|_| {
            ServerError(ApiError::validation(
                "the directory is not a git repository",
            ))
        })?;
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

pub async fn overview(
    pool: &SqlitePool,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectOverviewDto, ServerError> {
    let project = get(pool, project_id).await?;
    let threads = crate::threads::list(pool, project_id, user, false).await?;
    let mut waiting = Vec::new();
    let mut changes = ChangeStats {
        files: 0,
        additions: 0,
        deletions: 0,
        seen: 0,
        total: 0,
    };
    for thread in &threads {
        waiting.extend(crate::approvals::list_pending_for_thread(pool, thread.id.as_str()).await?);
        changes.files += thread.changes.files;
        changes.additions += thread.changes.additions;
        changes.deletions += thread.changes.deletions;
        changes.seen += thread.changes.seen;
        changes.total += thread.changes.total;
    }
    let activity = recent_activity(pool, project_id).await?;
    Ok(ProjectOverviewDto {
        project,
        threads,
        waiting_approvals: waiting,
        recent_activity: activity,
        change_stats: changes,
    })
}

#[derive(Debug, FromRow)]
struct ActivityRow {
    thread_id: String,
    thread_title: String,
    actor_name: Option<String>,
    actor_id: Option<String>,
    kind: String,
    ts: i64,
}

async fn recent_activity(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<ActivityDto>, ServerError> {
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT e.thread_id, t.title AS thread_title, e.actor_name, e.actor_id, e.kind, e.ts
         FROM events e JOIN threads t ON t.id = e.thread_id
         WHERE t.project_id = ",
    );
    builder.push_bind(project_id);
    builder.push(
        " AND e.kind NOT IN ('message_delta','thinking_delta','tool_call_update','usage_updated')
         ORDER BY e.ts DESC LIMIT 20",
    );
    let rows = builder
        .build_query_as::<ActivityRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let user = row.actor_id.unwrap_or_default();
            ActivityDto {
                thread_id: row.thread_id.into(),
                thread_title: row.thread_title,
                actor: threadbase_shared::UserBrief {
                    id: user.clone().into(),
                    username: row.actor_name.clone().unwrap_or_default(),
                    display_name: row.actor_name.unwrap_or_else(|| "Threadbase".to_owned()),
                    avatar_url: None,
                    color: threadbase_shared::user_color(&user).to_owned(),
                },
                summary: summary_for(&row.kind),
                ts: row.ts,
            }
        })
        .collect())
}

fn summary_for(kind: &str) -> String {
    match kind {
        "turn_started" => "started a turn",
        "message_completed" => "posted a message",
        "tool_call" => "ran a tool",
        "permission_requested" => "requested approval",
        "permission_resolved" => "resolved an approval",
        "turn_ended" => "finished a turn",
        "turn_error" => "hit an error",
        "changes_updated" => "changed files",
        "comment_added" => "left a comment",
        "subagent_updated" => "updated a subagent",
        "thread_updated" => "updated the thread",
        _ => "activity",
    }
    .to_owned()
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
