//! Lightweight workspace search across projects, threads and recent messages.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_shared::{ProjectId, SearchResultDto, ThreadId};

pub async fn search(
    pool: &SqlitePool,
    user: &CurrentUser,
    query: &str,
) -> Result<Vec<SearchResultDto>, ServerError> {
    let query = query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
    let mut results = Vec::new();
    let projects: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, name, path FROM projects WHERE name LIKE ? ESCAPE '\\' OR path LIKE ? ESCAPE '\\'
         ORDER BY updated_at DESC LIMIT 8",
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    for (id, name, path) in projects {
        results.push(SearchResultDto {
            kind: "project".into(),
            id: id.clone(),
            title: name,
            subtitle: Some(path),
            project_id: Some(ProjectId::from(id)),
            thread_id: None,
        });
    }
    let threads: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.title, t.project_id, p.name FROM threads t JOIN projects p ON p.id = t.project_id
         WHERE t.archived_at IS NULL AND t.title LIKE ? ESCAPE '\\'
           AND (? = 1 OR t.created_by = ? OR t.share_level = 'workspace'
                OR EXISTS(SELECT 1 FROM thread_members m WHERE m.thread_id = t.id AND m.user_id = ?))
         ORDER BY COALESCE(t.last_activity_at, t.updated_at) DESC LIMIT 12",
    )
    .bind(&pattern)
    .bind(i64::from(user.is_admin()))
    .bind(user.user.id.as_str())
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    for (id, title, project_id, project_name) in threads {
        results.push(SearchResultDto {
            kind: "thread".into(),
            id: id.clone(),
            title,
            subtitle: Some(project_name),
            project_id: Some(ProjectId::from(project_id)),
            thread_id: Some(ThreadId::from(id)),
        });
    }
    Ok(results)
}
