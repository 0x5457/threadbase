//! Thread reads: row loading, list/detail queries and batched DTO assembly.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use crate::subagents;
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use std::collections::HashMap;
use threadbase_core::permissions::PermissionPolicy;
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
    pub placement: String,
    pub placement_at: i64,
    pub snooze_until: Option<i64>,
    pub auto_settle: String,
    pub permission_mode: Option<String>,
    /// Joined from the owning project; `None` means the project inherits too.
    pub project_permission_mode: Option<String>,
}

const THREAD_COLUMNS: &str =
    "t.id, t.project_id, t.parent_thread_id, t.title, t.status, t.agent, t.model, t.effort,
     t.branch, t.base_branch, t.base_sha, t.last_turn_sha, t.worktree_path, t.setup_status,
     t.setup_error, t.created_by, t.created_at, t.updated_at, t.archived_at, t.pinned,
     t.last_activity_at, t.has_draft, t.placement, t.placement_at, t.snooze_until,
     t.auto_settle, t.permission_mode, p.permission_mode AS project_permission_mode";

/// Threads always carry their project's mode, so resolution never needs a
/// second query per row.
fn select(where_clause: &str) -> String {
    format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
           LEFT JOIN projects p ON p.id = t.project_id
         {where_clause}"
    )
}

#[derive(Clone, Default)]
struct Aggregate {
    pending_approvals: i64,
    pending_elicitations: i64,
    queued_inputs: i64,
    active_subagents: i64,
}

pub async fn load_row(pool: &SqlitePool, thread_id: &str) -> Result<ThreadRow, ServerError> {
    sqlx::query_as::<_, ThreadRow>(&select("WHERE t.id = ?"))
        .bind(thread_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("thread not found")))
}

pub async fn get(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
    policy: &PermissionPolicy,
) -> Result<ThreadDto, ServerError> {
    authz::require_read(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    dto(pool, &row, policy).await
}

pub async fn list(
    pool: &SqlitePool,
    project_id: &str,
    include_archived: bool,
    policy: &PermissionPolicy,
) -> Result<Vec<ThreadDto>, ServerError> {
    projects::row(pool, project_id).await?;
    let rows = sqlx::query_as::<_, ThreadRow>(&select(
        "WHERE t.project_id = ?
           AND (? = 1 OR t.archived_at IS NULL)
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC",
    ))
    .bind(project_id)
    .bind(i64::from(include_archived))
    .fetch_all(pool)
    .await?;
    let mut threads = dtos(pool, rows, policy).await?;
    sort_for_sidebar(&mut threads);
    Ok(threads)
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
    policy: &PermissionPolicy,
) -> Result<Vec<ThreadDto>, ServerError> {
    let rows = sqlx::query_as::<_, ThreadRow>(&select(
        "WHERE (? = 1 OR t.archived_at IS NULL)
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC",
    ))
    .bind(i64::from(include_archived))
    .fetch_all(pool)
    .await?;
    let mut threads = dtos(pool, rows, policy).await?;
    sort_for_sidebar(&mut threads);
    Ok(threads)
}

/// The mode in force for one thread. Used on the approval path, where the only
/// thing that matters is which posture to apply, not the whole DTO.
pub async fn permission_mode(
    pool: &SqlitePool,
    thread_id: &str,
    policy: &PermissionPolicy,
) -> Result<threadbase_shared::PermissionMode, ServerError> {
    let row = load_row(pool, thread_id).await?;
    Ok(policy.resolve(
        row.permission_mode
            .as_deref()
            .and_then(|value| value.parse().ok()),
        row.project_permission_mode
            .as_deref()
            .and_then(|value| value.parse().ok()),
    ))
}

pub async fn dto(
    pool: &SqlitePool,
    row: &ThreadRow,
    policy: &PermissionPolicy,
) -> Result<ThreadDto, ServerError> {
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
        policy,
    ))
}

async fn dtos(
    pool: &SqlitePool,
    rows: Vec<ThreadRow>,
    policy: &PermissionPolicy,
) -> Result<Vec<ThreadDto>, ServerError> {
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
            policy,
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
    policy: &PermissionPolicy,
) -> ThreadDto {
    let status =
        threadbase_core::state::parse_thread_status(&row.status).unwrap_or(ThreadStatus::Idle);
    let placement = threadbase_core::state::parse_thread_placement(&row.placement)
        .unwrap_or(threadbase_shared::ThreadPlacement::Active);
    let queued_inputs = u32::try_from(aggregate.queued_inputs).unwrap_or(0);
    let active_subagents = u32::try_from(aggregate.active_subagents).unwrap_or(0);
    let pending_approvals = u32::try_from(aggregate.pending_approvals).unwrap_or(0);
    let pending_elicitations = u32::try_from(aggregate.pending_elicitations).unwrap_or(0);
    let working = threadbase_core::state::is_working(status, queued_inputs, active_subagents);
    let thread_mode = row.permission_mode.as_deref().and_then(|v| v.parse().ok());
    let project_mode = row
        .project_permission_mode
        .as_deref()
        .and_then(|v| v.parse().ok());
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
        placement,
        section: threadbase_core::state::section(status, placement, row.pinned != 0, working),
        placement_at: row.placement_at,
        snooze_until: row.snooze_until,
        auto_settle: row
            .auto_settle
            .parse()
            .unwrap_or(threadbase_shared::AutoSettle::Default),
        permission_mode: policy.resolve(thread_mode, project_mode),
        permission_mode_override: thread_mode,
        participants,
        pending_approvals,
        pending_elicitations,
        has_draft: row.has_draft != 0,
        subagents,
    }
}

/// Order the sidebar the way it is read: section, then work that needs a person,
/// then the most recent activity. The server owns this so every client and every
/// device agrees, and so a change to the rule is one edit rather than five.
fn sort_for_sidebar(threads: &mut [ThreadDto]) {
    threads.sort_by(|left, right| {
        let rank = threadbase_core::state::section_rank(left.section)
            .cmp(&threadbase_core::state::section_rank(right.section));
        if rank != std::cmp::Ordering::Equal {
            return rank;
        }
        let attention = |thread: &ThreadDto| {
            threadbase_core::state::needs_attention(
                thread.status,
                thread.pending_approvals + thread.pending_elicitations,
            )
        };
        attention(right)
            .cmp(&attention(left))
            .then_with(|| {
                let at = |thread: &ThreadDto| thread.placement_at.max(thread.updated_at);
                at(right).cmp(&at(left))
            })
            .then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
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
              WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals,
            (SELECT COUNT(*) FROM elicitations e
              WHERE e.thread_id = t.id AND e.status = 'pending') AS pending_elicitations,
            (SELECT COUNT(*) FROM queued_inputs q
              WHERE q.thread_id = t.id AND q.status = 'queued') AS queued_inputs,
            (SELECT COUNT(*) FROM subagents s
              WHERE s.thread_id = t.id AND s.status IN ('running','requires_action')) AS active_subagents
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
                    pending_elicitations: row.pending_elicitations,
                    queued_inputs: row.queued_inputs,
                    active_subagents: row.active_subagents,
                },
            )
        })
        .collect())
}

#[derive(FromRow)]
struct AggregateRow {
    id: String,
    pending_approvals: i64,
    pending_elicitations: i64,
    queued_inputs: i64,
    active_subagents: i64,
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
