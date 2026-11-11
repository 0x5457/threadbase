//! Thread lifecycle: worktrees, membership, forks, archive and deletion.

use crate::auth::{audit, CurrentUser};
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use crate::state::{NewThread, StateStore};
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use threadbase_core::state::{parse_subagent_status, parse_thread_status, ReadRules, ThreadAction};
use threadbase_shared::api::{CreateThreadRequest, ForkThreadRequest, UpdateThreadRequest};
use threadbase_shared::{
    now_ms, user_color, ApiError, ChangeStats, Participant, Role, ShareLevel, SubagentStatus,
    ThreadDto, ThreadStatus, UserBrief,
};
use threadbase_workspace::git;
use ulid::Ulid;

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
    "id, project_id, parent_thread_id, title, status, agent, model, effort,
    branch, base_branch, base_sha, last_turn_sha, worktree_path, created_by, created_at, updated_at,
    archived_at, pinned, share_level, last_activity_at";

pub async fn load_row(pool: &SqlitePool, thread_id: &str) -> Result<ThreadRow, ServerError> {
    sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads WHERE id = ?"
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
           AND (? = 1 OR t.created_by = ? OR t.share_level = 'workspace'
                OR EXISTS(SELECT 1 FROM thread_members m WHERE m.thread_id = t.id AND m.user_id = ?))
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(project_id)
    .bind(i64::from(include_archived))
    .bind(i64::from(user.is_admin()))
    .bind(user.user.id.as_str())
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(dto(pool, &row, user).await?);
    }
    Ok(out)
}

pub async fn create(
    pool: &SqlitePool,
    state_store: &StateStore,
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
    state_store
        .insert_thread(&NewThread {
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
            share_level: "invite_only",
        })
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

    sqlx::query(
        "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, 'owner', ?)",
    )
    .bind(&thread_id)
    .bind(user.user.id.as_str())
    .bind(now)
    .execute(pool)
    .await?;
    state_store
        .insert_binding(&thread_id, &agent, &worktree.to_string_lossy(), now)
        .await?;
    state_store
        .transition_thread(&thread_id, ThreadAction::StartSucceeded)
        .await?;

    if let Some(prompt) = request
        .initial_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let id = Ulid::new().to_string();
        state_store
            .enqueue(crate::state::EnqueueInput {
                id: &id,
                thread_id: &thread_id,
                author_id: user.user.id.as_str(),
                text: prompt,
                authors_json: None,
                position: 1,
                created_at: now,
            })
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
    let row = load_row(pool, &thread_id).await?;
    dto(pool, &row, user).await
}

pub async fn update(
    pool: &SqlitePool,
    state_store: &StateStore,
    thread_id: &str,
    user: &CurrentUser,
    request: &UpdateThreadRequest,
) -> Result<ThreadDto, ServerError> {
    let row = load_row(pool, thread_id).await?;
    if request.title.is_some() || request.pinned.is_some() {
        authz::require_write_role(pool, thread_id, user).await?;
    }
    if request.archived.is_some() || request.share_level.is_some() {
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
        state_store.transition_thread(thread_id, action).await?;
    }
    if let Some(model) = request.model.as_deref() {
        sqlx::query("UPDATE threads SET model = ?, updated_at = ? WHERE id = ?")
            .bind(if model.trim().is_empty() {
                None
            } else {
                Some(model.trim())
            })
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    if let Some(effort) = request.effort.as_deref() {
        sqlx::query("UPDATE threads SET effort = ?, updated_at = ? WHERE id = ?")
            .bind(if effort.trim().is_empty() {
                None
            } else {
                Some(effort.trim())
            })
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    if let Some(share) = request.share_level {
        let value = match share {
            ShareLevel::InviteOnly => "invite_only",
            ShareLevel::Workspace => "workspace",
        };
        sqlx::query("UPDATE threads SET share_level = ?, updated_at = ? WHERE id = ?")
            .bind(value)
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    let _ = row;
    let row = load_row(pool, thread_id).await?;
    dto(pool, &row, user).await
}

pub async fn fork(
    pool: &SqlitePool,
    state_store: &StateStore,
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
    let head = if let Some(path) = source_worktree.as_deref() {
        git::head_sha(path)
            .await
            .unwrap_or_else(|_| source.base_sha.clone().unwrap_or_default())
    } else {
        source.base_sha.clone().unwrap_or_default()
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
    state_store
        .insert_thread(&NewThread {
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
            share_level: &source.share_level,
        })
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
    sqlx::query(
        "INSERT INTO thread_members(thread_id, user_id, role, joined_at)
         SELECT ?, user_id, role, ? FROM thread_members WHERE thread_id = ?",
    )
    .bind(&new_id)
    .bind(now)
    .bind(&source.id)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO thread_members(thread_id, user_id, role, joined_at)
         VALUES(?, ?, 'owner', ?)",
    )
    .bind(&new_id)
    .bind(user.user.id.as_str())
    .bind(now)
    .execute(pool)
    .await?;
    state_store
        .copy_binding(&new_id, &worktree.to_string_lossy(), &source.id, now)
        .await?;
    state_store
        .transition_thread(&new_id, ThreadAction::StartSucceeded)
        .await?;
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
    let row = load_row(pool, &new_id).await?;
    dto(pool, &row, user).await
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

pub async fn dto(
    pool: &SqlitePool,
    row: &ThreadRow,
    user: &CurrentUser,
) -> Result<ThreadDto, ServerError> {
    let participants = participants(pool, row).await?;
    let pending_approvals = crate::approvals::count_pending(pool, &row.id).await?;
    let queued_inputs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(&row.id)
    .fetch_one(pool)
    .await?;
    let max_seq: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?")
            .bind(&row.id)
            .fetch_one(pool)
            .await?;
    let last_read: i64 = sqlx::query_scalar(
        "SELECT COALESCE(last_read_seq, 0) FROM thread_reads WHERE thread_id = ? AND user_id = ?",
    )
    .bind(&row.id)
    .bind(user.user.id.as_str())
    .fetch_optional(pool)
    .await?
    .unwrap_or(0);
    let diverged = is_diverged(pool, row).await;
    let changes = crate::diff::stats(pool, row, user)
        .await
        .unwrap_or(ChangeStats {
            files: 0,
            additions: 0,
            deletions: 0,
            seen: 0,
            total: 0,
        });
    Ok(ThreadDto {
        id: row.id.clone().into(),
        project_id: row.project_id.clone().into(),
        parent_thread_id: row.parent_thread_id.clone().map(Into::into),
        title: row.title.clone(),
        status: parse_thread_status(&row.status).unwrap_or(ThreadStatus::Idle),
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
        unread: ReadRules::thread_unread(last_read as u64, max_seq as u64),
        share_level: parse_share(&row.share_level),
        diverged,
        participants,
        pending_approvals,
        queued_inputs: u32::try_from(queued_inputs).unwrap_or(0),
        changes,
    })
}

pub(crate) async fn is_diverged(pool: &SqlitePool, row: &ThreadRow) -> bool {
    let (Some(base), Some(worktree)) = (row.base_sha.as_deref(), row.worktree_path.as_deref())
    else {
        return false;
    };
    let _ = worktree;
    let Ok(project) = crate::projects::row(pool, &row.project_id).await else {
        return false;
    };
    let Ok(head) = threadbase_workspace::git::rev_parse(
        std::path::Path::new(&project.path),
        &project.default_branch,
    )
    .await
    else {
        return false;
    };
    if head == base {
        return false;
    }
    !threadbase_workspace::git::is_ancestor(std::path::Path::new(&project.path), base, &head)
        .await
        .unwrap_or(false)
}

async fn participants(pool: &SqlitePool, row: &ThreadRow) -> Result<Vec<Participant>, ServerError> {
    #[derive(FromRow)]
    struct MemberRow {
        id: String,
        username: String,
        display_name: String,
        avatar_url: Option<String>,
        role: String,
        joined_at: i64,
    }
    let mut rows = sqlx::query_as::<_, MemberRow>(
        "SELECT u.id, u.username, u.display_name, u.avatar_url, m.role, m.joined_at
         FROM thread_members m JOIN users u ON u.id = m.user_id
         WHERE m.thread_id = ? ORDER BY m.joined_at ASC",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        if let Some(creator) = sqlx::query_as::<_, MemberRow>(
            "SELECT id, username, display_name, avatar_url, 'owner' AS role, ? AS joined_at
             FROM users WHERE id = ?",
        )
        .bind(row.created_at)
        .bind(&row.created_by)
        .fetch_optional(pool)
        .await?
        {
            rows.push(creator);
        }
    }
    Ok(rows
        .into_iter()
        .map(|member| {
            let brief = UserBrief {
                id: member.id.clone().into(),
                username: member.username,
                display_name: member.display_name,
                avatar_url: member.avatar_url,
                color: user_color(&member.id).to_owned(),
            };
            Participant {
                user: brief,
                role: parse_role(&member.role),
                online: false,
                last_seen_at: Some(member.joined_at),
            }
        })
        .collect())
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

fn parse_share(value: &str) -> ShareLevel {
    match value {
        "workspace" => ShareLevel::Workspace,
        _ => ShareLevel::InviteOnly,
    }
}

fn parse_role(value: &str) -> Role {
    match value {
        "owner" => Role::Owner,
        "editor" => Role::Editor,
        _ => Role::Viewer,
    }
}

pub async fn list_all(
    pool: &SqlitePool,
    user: &CurrentUser,
    include_archived: bool,
) -> Result<Vec<ThreadDto>, ServerError> {
    let rows = sqlx::query_as::<_, ThreadRow>(&format!(
        "SELECT {THREAD_COLUMNS} FROM threads t
         WHERE (? = 1 OR t.archived_at IS NULL)
           AND (? = 1 OR t.created_by = ? OR t.share_level = 'workspace'
                OR EXISTS(SELECT 1 FROM thread_members m WHERE m.thread_id = t.id AND m.user_id = ?))
         ORDER BY t.pinned DESC, COALESCE(t.last_activity_at, t.updated_at) DESC"
    ))
    .bind(i64::from(include_archived))
    .bind(i64::from(user.is_admin()))
    .bind(user.user.id.as_str())
    .bind(user.user.id.as_str())
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(dto(pool, &row, user).await?);
    }
    Ok(out)
}

pub async fn bundle(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<threadbase_shared::ThreadBundleDto, ServerError> {
    authz::require_read(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    let thread = dto(pool, &row, user).await?;
    let project = projects::get(pool, &row.project_id).await?;
    let subagents = list_subagents(pool, thread_id).await?;
    let approvals = crate::approvals::list_pending_for_thread(pool, thread_id).await?;
    let elicitations = crate::approvals::list_elicitations(pool, thread_id).await?;
    let queue = crate::queue::list_for_thread(pool, thread_id).await?;
    let comments = crate::comments::list_for_thread(pool, thread_id).await?;
    let changes = crate::diff::list(
        pool,
        &row,
        user,
        &threadbase_shared::BaselineDto {
            kind: threadbase_shared::BaselineKind::BaseBranch,
            commit: None,
        },
    )
    .await
    .unwrap_or_else(|_| empty_changes(&row));
    let members = members(pool, &row).await?;
    let invites = invites(pool, thread_id).await?;
    let share = threadbase_shared::ShareStateDto {
        thread_id: thread_id.to_owned().into(),
        share_level: thread.share_level,
        members: members.clone(),
        pending_invites: invites,
        share_token: None,
    };
    Ok(threadbase_shared::ThreadBundleDto {
        thread,
        project,
        subagents,
        approvals,
        elicitations,
        queue,
        comments,
        changes,
        members,
        share,
    })
}

fn empty_changes(row: &ThreadRow) -> threadbase_shared::ChangesDto {
    threadbase_shared::ChangesDto {
        baseline: threadbase_shared::BaselineDto {
            kind: threadbase_shared::BaselineKind::BaseBranch,
            commit: None,
        },
        base_label: row.base_branch.clone(),
        files: Vec::new(),
        stats: ChangeStats {
            files: 0,
            additions: 0,
            deletions: 0,
            seen: 0,
            total: 0,
        },
        commit: row.base_sha.clone(),
    }
}

async fn list_subagents(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<threadbase_shared::SubagentDto>, ServerError> {
    #[derive(FromRow)]
    struct Row {
        id: String,
        thread_id: String,
        parent_id: Option<String>,
        title: String,
        description: Option<String>,
        status: String,
        cwd: Option<String>,
        can_cancel: i64,
        usage_json: Option<String>,
        last_seq: i64,
        created_at: i64,
        updated_at: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, thread_id, parent_id, title, description, status, cwd, can_cancel,
                usage_json, last_seq, created_at, updated_at
         FROM subagents WHERE thread_id = ? ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| threadbase_shared::SubagentDto {
            id: row.id.into(),
            thread_id: row.thread_id.into(),
            parent_id: row.parent_id.map(Into::into),
            title: row.title,
            description: row.description,
            status: parse_subagent_status(&row.status).unwrap_or(SubagentStatus::Idle),
            cwd: row.cwd,
            can_cancel: row.can_cancel != 0,
            usage: row
                .usage_json
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok()),
            last_seq: u64::try_from(row.last_seq).unwrap_or(0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect())
}

async fn members(
    pool: &SqlitePool,
    row: &ThreadRow,
) -> Result<Vec<threadbase_shared::ThreadMemberDto>, ServerError> {
    #[derive(FromRow)]
    struct Row {
        id: String,
        username: String,
        display_name: String,
        avatar_url: Option<String>,
        role: String,
        joined_at: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT u.id, u.username, u.display_name, u.avatar_url, m.role, m.joined_at
         FROM thread_members m JOIN users u ON u.id = m.user_id
         WHERE m.thread_id = ? ORDER BY m.joined_at ASC",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|member| threadbase_shared::ThreadMemberDto {
            user: UserBrief {
                id: member.id.clone().into(),
                username: member.username,
                display_name: member.display_name,
                avatar_url: member.avatar_url,
                color: user_color(&member.id).to_owned(),
            },
            role: parse_role(&member.role),
            joined_at: member.joined_at,
        })
        .collect())
}

async fn invites(
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
    let rows = sqlx::query_as::<_, Row>(
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
            role: parse_role(&row.role),
            email: row.email,
            created_at: row.created_at,
            expires_at: row.expires_at,
            accepted_at: row.accepted_at,
        })
        .collect())
}
