//! Thread reads: row loading, list/detail queries and batched DTO assembly.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use crate::subagents;
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use std::collections::HashMap;
use threadbase_shared::{ApiError, Participant, Role, ThreadDto, ThreadStatus, UserBrief};

#[derive(Debug, Clone, FromRow)]
pub struct ThreadRow {
    pub id: String,
    pub project_id: String,
    pub parent_thread_id: Option<String>,
    pub title: String,
    pub status: String,
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub branch: String,
    pub base_branch: String,
    pub base_sha: Option<String>,
    pub last_turn_sha: Option<String>,
    pub worktree_path: Option<String>,
    pub setup_status: String,
    pub setup_error: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub pinned: i64,
    pub last_activity_at: Option<i64>,
    pub has_draft: i64,
}

const THREAD_COLUMNS: &str =
    "t.id, t.project_id, t.parent_thread_id, t.title, t.status, t.agent, t.model, t.effort,
     t.branch, t.base_branch, t.base_sha, t.last_turn_sha, t.worktree_path, t.setup_status,
     t.setup_error, t.created_by, t.created_at, t.updated_at, t.archived_at, t.pinned,
     t.last_activity_at, t.has_draft";

#[derive(Clone, Default)]
struct Aggregate {
    pending_approvals: i64,
}

pub async fn load_row(pool: &SqlitePool, thread_id: &str) -> Result<ThreadRow, ServerError> {
    sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t WHERE t.id = ?"
    ))
    .bind(thread_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ServerError(ApiError::not_found("thread not found")))
}

pub async fn get(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<ThreadDto, ServerError> {
    authz::require_read(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    dto(pool, &row).await
}

pub async fn list(
    pool: &SqlitePool,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<ThreadDto>, ServerError> {
    projects::row(pool, project_id).await?;
    let rows = sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
         WHERE t.project_id = ?
           AND (? = 1 OR t.archived_at IS NULL)
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(project_id)
    .bind(i64::from(include_archived))
    .fetch_all(pool)
    .await?;
    dtos(pool, rows).await
}

/// Thread ids for a project, used to stop their adapters before deletion.
pub async fn ids_for_project(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<String>, ServerError> {
    Ok(
        sqlx::query_scalar("SELECT id FROM threads WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_all(
    pool: &SqlitePool,
    include_archived: bool,
) -> Result<Vec<ThreadDto>, ServerError> {
    let rows = sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
         WHERE (? = 1 OR t.archived_at IS NULL)
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(i64::from(include_archived))
    .fetch_all(pool)
    .await?;
    dtos(pool, rows).await
}

pub async fn dto(pool: &SqlitePool, row: &ThreadRow) -> Result<ThreadDto, ServerError> {
    let ids = [row.id.as_str()];
    let aggregates = aggregates(pool, &ids).await?;
    let participants = creator_participant(pool, &row.created_by, row.created_at)
        .await
        .map(|creator| vec![creator])
        .unwrap_or_default();
    let subagents = subagents::list(pool, &row.id).await?;
    Ok(to_dto(
        row,
        row.id.clone(),
        aggregates.get(&row.id).cloned().unwrap_or_default(),
        participants,
        subagents,
    ))
}

async fn dtos(pool: &SqlitePool, rows: Vec<ThreadRow>) -> Result<Vec<ThreadDto>, ServerError> {
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let aggregates = aggregates(pool, &ids).await?;
    let mut subagents = subagents::list_for_threads(pool, &ids).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let aggregate = aggregates.get(&row.id).cloned().unwrap_or_default();
        let participants = creator_participant(pool, &row.created_by, row.created_at)
            .await
            .map(|creator| vec![creator])
            .unwrap_or_default();
        out.push(to_dto(
            &row,
            row.id.clone(),
            aggregate,
            participants,
            subagents.remove(&row.id).unwrap_or_default(),
        ));
    }
    Ok(out)
}

fn to_dto(
    row: &ThreadRow,
    id: String,
    aggregate: Aggregate,
    participants: Vec<Participant>,
    subagents: Vec<threadbase_shared::SubagentDto>,
) -> ThreadDto {
    let status =
        threadbase_core::state::parse_thread_status(&row.status).unwrap_or(ThreadStatus::Idle);
    ThreadDto {
        id: id.into(),
        project_id: row.project_id.clone().into(),
        parent_thread_id: row.parent_thread_id.clone().map(Into::into),
        title: row.title.clone(),
        status,
        allowed_actions: threadbase_core::state::thread_commands(status),
        agent: row.agent.clone(),
        model: row.model.clone(),
        effort: row.effort.clone(),
        branch: row.branch.clone(),
        base_branch: row.base_branch.clone(),
        base_sha: row.base_sha.clone(),
        worktree_path: row.worktree_path.clone(),
        setup_status: threadbase_core::state::parse_setup_status(&row.setup_status)
            .unwrap_or(threadbase_shared::SetupStatus::None),
        setup_error: row.setup_error.clone(),
        created_by: row.created_by.clone().into(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        archived_at: row.archived_at,
        pinned: row.pinned != 0,
        participants,
        pending_approvals: u32::try_from(aggregate.pending_approvals).unwrap_or(0),
        has_draft: row.has_draft != 0,
        subagents,
    }
}

/// Pending approvals, queued inputs, last event and read cursor for a set of
/// threads in one round trip.
async fn aggregates(
    pool: &SqlitePool,
    ids: &[&str],
) -> Result<HashMap<String, Aggregate>, ServerError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::new(
        "SELECT t.id,
            (SELECT COUNT(*) FROM approvals a
              WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals
         FROM threads t WHERE t.id IN (",
    );
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(")");
    let rows: Vec<AggregateRow> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.id,
                Aggregate {
                    pending_approvals: row.pending_approvals,
                },
            )
        })
        .collect())
}

#[derive(FromRow)]
struct AggregateRow {
    id: String,
    pending_approvals: i64,
}

async fn creator_participant(
    pool: &SqlitePool,
    user_id: &str,
    joined_at: i64,
) -> Option<Participant> {
    let user = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT id, username, display_name, avatar_url FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()??;
    Some(Participant {
        user: UserBrief {
            id: user.0.clone().into(),
            username: user.1,
            display_name: user.2,
            avatar_url: user.3,
            color: threadbase_shared::user_color(&user.0).to_owned(),
        },
        role: Role::Owner,
        online: false,
        last_seen_at: Some(joined_at),
    })
}
