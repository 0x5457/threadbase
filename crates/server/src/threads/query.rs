//! Thread reads: row loading, list/detail queries and batched DTO assembly.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use std::collections::HashMap;
use threadbase_core::state::ReadRules;
use threadbase_shared::{
    ApiError, Participant, Role, ShareLevel, ThreadDto, ThreadMemberDto, ThreadStatus, UserBrief,
};

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
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub pinned: i64,
    pub share_level: String,
    pub last_activity_at: Option<i64>,
}

const THREAD_COLUMNS: &str =
    "t.id, t.project_id, t.parent_thread_id, t.title, t.status, t.agent, t.model, t.effort,
     t.branch, t.base_branch, t.base_sha, t.last_turn_sha, t.worktree_path, t.created_by,
     t.created_at, t.updated_at, t.archived_at, t.pinned, t.share_level, t.last_activity_at";

const VISIBLE_TO_USER: &str = "(
    ? = 1 OR t.created_by = ? OR t.share_level = 'workspace'
    OR EXISTS(SELECT 1 FROM thread_members m WHERE m.thread_id = t.id AND m.user_id = ?)
)";

#[derive(Clone, Default)]
struct Aggregate {
    pending_approvals: i64,
    queued_inputs: i64,
    max_seq: i64,
    last_read: i64,
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
    dto(pool, &row, user).await
}

pub async fn list(
    pool: &SqlitePool,
    project_id: &str,
    user: &CurrentUser,
    include_archived: bool,
) -> Result<Vec<ThreadDto>, ServerError> {
    projects::row(pool, project_id).await?;
    let rows = sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
         WHERE t.project_id = ?
           AND (? = 1 OR t.archived_at IS NULL)
           AND {VISIBLE_TO_USER}
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(project_id)
    .bind(i64::from(include_archived))
    .bind(i64::from(user.is_admin()))
    .bind(user.user.id.as_str())
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    dtos(pool, rows, user).await
}

pub async fn list_all(
    pool: &SqlitePool,
    user: &CurrentUser,
    include_archived: bool,
) -> Result<Vec<ThreadDto>, ServerError> {
    let rows = sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
         WHERE (? = 1 OR t.archived_at IS NULL)
           AND {VISIBLE_TO_USER}
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(i64::from(include_archived))
    .bind(i64::from(user.is_admin()))
    .bind(user.user.id.as_str())
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    dtos(pool, rows, user).await
}

pub async fn dto(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
) -> Result<ThreadDto, ServerError> {
    let ids = [row.id.as_str()];
    let aggregates = aggregates(pool, &ids, user.user.id.as_str()).await?;
    let mut participants = participants_by_thread(pool, &ids).await?;
    if participants.get(&row.id).is_none_or(Vec::is_empty) {
        if let Some(creator) = creator_participant(pool, &row.created_by, row.created_at).await {
            participants.insert(row.id.clone(), vec![creator]);
        }
    }
    Ok(to_dto(
        row,
        row.id.clone(),
        aggregates.get(&row.id).cloned().unwrap_or_default(),
        participants.get(&row.id).cloned().unwrap_or_default(),
    ))
}

async fn dtos(
    pool: &SqlitePool,
    rows: Vec<ThreadRow>,
    user: &CurrentUser,
) -> Result<Vec<ThreadDto>, ServerError> {
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let aggregates = aggregates(pool, &ids, user.user.id.as_str()).await?;
    let mut participants = participants_by_thread(pool, &ids).await?;
    for row in &rows {
        if participants.get(&row.id).is_none_or(Vec::is_empty) {
            if let Some(creator) = creator_participant(pool, &row.created_by, row.created_at).await
            {
                participants.insert(row.id.clone(), vec![creator]);
            }
        }
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let aggregate = aggregates.get(&row.id).cloned().unwrap_or_default();
            let members = participants.get(&row.id).cloned().unwrap_or_default();
            to_dto(&row, row.id.clone(), aggregate, members)
        })
        .collect())
}

fn to_dto(
    row: &ThreadRow,
    id: String,
    aggregate: Aggregate,
    participants: Vec<Participant>,
) -> ThreadDto {
    let max_seq = u64::try_from(aggregate.max_seq).unwrap_or(0);
    let last_read = u64::try_from(aggregate.last_read).unwrap_or(0);
    ThreadDto {
        id: id.into(),
        project_id: row.project_id.clone().into(),
        parent_thread_id: row.parent_thread_id.clone().map(Into::into),
        title: row.title.clone(),
        status: threadbase_core::state::parse_thread_status(&row.status)
            .unwrap_or(ThreadStatus::Idle),
        agent: row.agent.clone(),
        model: row.model.clone(),
        effort: row.effort.clone(),
        branch: row.branch.clone(),
        base_branch: row.base_branch.clone(),
        base_sha: row.base_sha.clone(),
        worktree_path: row.worktree_path.clone(),
        created_by: row.created_by.clone().into(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        archived_at: row.archived_at,
        pinned: row.pinned != 0,
        unread: ReadRules::thread_unread(last_read, max_seq),
        share_level: ShareLevel::parse(&row.share_level),
        participants,
        pending_approvals: u32::try_from(aggregate.pending_approvals).unwrap_or(0),
        queued_inputs: u32::try_from(aggregate.queued_inputs).unwrap_or(0),
    }
}

/// Pending approvals, queued inputs, last event and read cursor for a set of
/// threads in one round trip.
async fn aggregates(
    pool: &SqlitePool,
    ids: &[&str],
    user_id: &str,
) -> Result<HashMap<String, Aggregate>, ServerError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::new(
        "SELECT t.id,
            (SELECT COUNT(*) FROM approvals a
              WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals,
            (SELECT COUNT(*) FROM queued_inputs q
              WHERE q.thread_id = t.id AND q.status = 'queued') AS queued_inputs,
            (SELECT COALESCE(MAX(seq), 0) FROM events e
              WHERE e.thread_id = t.id) AS max_seq,
            COALESCE((SELECT last_read_seq FROM thread_reads r
              WHERE r.thread_id = t.id AND r.user_id = ",
    );
    query.push_bind(user_id);
    query.push("), 0) AS last_read FROM threads t WHERE t.id IN (");
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
                    queued_inputs: row.queued_inputs,
                    max_seq: row.max_seq,
                    last_read: row.last_read,
                },
            )
        })
        .collect())
}

#[derive(FromRow)]
struct AggregateRow {
    id: String,
    pending_approvals: i64,
    queued_inputs: i64,
    max_seq: i64,
    last_read: i64,
}

async fn participants_by_thread(
    pool: &SqlitePool,
    ids: &[&str],
) -> Result<HashMap<String, Vec<Participant>>, ServerError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::new(
        "SELECT m.thread_id, u.id, u.username, u.display_name, u.avatar_url, m.role, m.joined_at
         FROM thread_members m JOIN users u ON u.id = m.user_id
         WHERE m.thread_id IN (",
    );
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(") ORDER BY m.joined_at ASC");
    let rows: Vec<MemberRow> = query.build_query_as().fetch_all(pool).await?;
    let mut grouped: HashMap<String, Vec<Participant>> = HashMap::new();
    for row in rows {
        let color = threadbase_shared::user_color(&row.id).to_owned();
        grouped.entry(row.thread_id).or_default().push(Participant {
            user: UserBrief {
                id: row.id.into(),
                username: row.username,
                display_name: row.display_name,
                avatar_url: row.avatar_url,
                color,
            },
            role: Role::parse(&row.role),
            online: false,
            last_seen_at: Some(row.joined_at),
        });
    }
    Ok(grouped)
}

#[derive(FromRow)]
struct MemberRow {
    thread_id: String,
    id: String,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
    role: String,
    joined_at: i64,
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

pub(crate) async fn members(
    pool: &SqlitePool,
    row: &ThreadRow,
) -> Result<Vec<ThreadMemberDto>, ServerError> {
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT m.thread_id, u.id, u.username, u.display_name, u.avatar_url, m.role, m.joined_at
         FROM thread_members m JOIN users u ON u.id = m.user_id
         WHERE m.thread_id = ? ORDER BY m.joined_at ASC",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|member| ThreadMemberDto {
            user: UserBrief {
                id: member.id.clone().into(),
                username: member.username,
                display_name: member.display_name,
                avatar_url: member.avatar_url,
                color: threadbase_shared::user_color(&member.id).to_owned(),
            },
            role: Role::parse(&member.role),
            joined_at: member.joined_at,
        })
        .collect())
}

pub(crate) async fn invites(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<threadbase_shared::InviteDto>, ServerError> {
    #[derive(FromRow)]
    struct Row {
        id: String,
        thread_id: String,
        role: String,
        email: Option<String>,
        created_at: i64,
        expires_at: i64,
        accepted_at: Option<i64>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, thread_id, role, email, created_at, expires_at, accepted_at
         FROM invites WHERE thread_id = ? AND accepted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| threadbase_shared::InviteDto {
            id: row.id.into(),
            thread_id: row.thread_id.into(),
            role: Role::parse(&row.role),
            email: row.email,
            created_at: row.created_at,
            expires_at: row.expires_at,
            accepted_at: row.accepted_at,
        })
        .collect())
}
