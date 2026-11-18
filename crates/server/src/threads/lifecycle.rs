//! Thread lifecycle: creation, worktree mutations, transitions and deletion.

use super::query::{get, load_row};
use crate::auth::{audit, CurrentUser};
use crate::authz;
use crate::bindings;
use crate::error::ServerError;
use crate::events::{EventMeta, EventStore};
use crate::projection::{self, Projection};
use crate::projects;
use crate::queue;
use crate::watchers;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use threadbase_core::permissions::PermissionPolicy;
use threadbase_core::state::{subagent_status_str, PlacementAction, PlacementMachine};
use threadbase_core::state::{ThreadAction, ThreadMachine};
use threadbase_shared::api::{CreateThreadRequest, ForkThreadRequest, UpdateThreadRequest};
use threadbase_shared::{
    now_ms, Actor, ApiError, PermissionMode, SubagentStatus, ThreadDto, ThreadEvent, ThreadId,
    ThreadPlacement, ThreadStatus, TurnId,
};
use threadbase_workspace::git;
use ulid::Ulid;

/// A snooze without a deadline wakes a day later. The server picks the value so
/// two browsers cannot disagree about what "tomorrow" means.
const SNOOZE_DEFAULT_MS: i64 = 24 * 60 * 60 * 1000;

pub struct NewThread<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub parent_thread_id: Option<&'a str>,
    pub title: &'a str,
    pub agent: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// The thread's own permission choice; `None` inherits the project default.
    pub permission_mode: Option<&'a str>,
    pub branch: &'a str,
    pub base_branch: &'a str,
    pub base_sha: &'a str,
    pub worktree_path: Option<&'a str>,
    pub created_by: &'a str,
    pub created_at: i64,
}

pub async fn status(pool: &SqlitePool, thread_id: &str) -> Result<ThreadStatus, ServerError> {
    projection::thread_status(pool, thread_id).await
}

/// Move a thread between shelves. This is the only writer of `placement`.
///
/// The guard lives here rather than in the machine because it reads live facts
/// (queued input, subagents, unanswered requests) that a pure transition cannot
/// see. A setting that hides work in progress is the failure mode, so settling is
/// refused instead of cancelling turns nobody asked to cancel.
pub async fn place(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    action: PlacementAction,
    until: Option<i64>,
) -> Result<ThreadPlacement, ServerError> {
    let state = projection.read(thread_id).await?;
    if state.status == ThreadStatus::Archived {
        return Err(ServerError(ApiError::conflict(
            "this thread is archived; unarchive it before moving it",
        )));
    }
    // Auto-settling is the same transition as settling, so it gets the same
    // guard: a thread that just started a turn must not be parked by the sweep.
    if matches!(
        action,
        PlacementAction::Settle | PlacementAction::AutoSettle
    ) {
        threadbase_core::state::check_settle_allowed(
            state.status,
            state.queued_inputs,
            state.active_subagents,
            state.pending_approvals + state.pending_elicitations,
        )
        .map_err(|error| ServerError(ApiError::conflict(error.to_string())))?;
    }
    let current = state.placement;
    let next = PlacementMachine::transition(current, &action).map_err(|_| {
        ServerError(ApiError::conflict(PlacementMachine::refusal(
            &action, current,
        )))
    })?;
    if next == current && action != PlacementAction::Snooze {
        return Ok(next);
    }
    let now = now_ms();
    let snooze_until = (next == ThreadPlacement::Snoozed)
        .then(|| until.unwrap_or_else(|| now + SNOOZE_DEFAULT_MS));
    // Settling takes the pin with it: the sidebar sorts pinned work first, and a
    // pin that outlives the shelf would put finished work back on top.
    let clears_pin = next == ThreadPlacement::Settled;
    sqlx::query(
        "UPDATE threads SET placement = ?, placement_at = ?, snooze_until = ?,
            pinned = CASE WHEN ? THEN 0 ELSE pinned END,
            updated_at = ?
          WHERE id = ?",
    )
    .bind(threadbase_core::state::placement_str(next))
    .bind(now)
    .bind(snooze_until)
    .bind(clears_pin)
    .bind(now)
    .bind(thread_id)
    .execute(pool)
    .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

/// A message is a reason to come back: settled and snoozed threads wake before
/// the turn starts, so a prompt never lands in a shelf.
pub async fn wake(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
) -> Result<(), ServerError> {
    let state = projection.read(thread_id).await?;
    let action = match state.placement {
        ThreadPlacement::Active => return Ok(()),
        ThreadPlacement::Snoozed => PlacementAction::Wake,
        ThreadPlacement::Settled => PlacementAction::Unsettle,
    };
    place(pool, projection, thread_id, action, None).await?;
    Ok(())
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

/// Archive or unarchive a thread, writing the status and its `archived_at`
/// marker in one statement.
///
/// They used to be written by two separate statements, so a failure between
/// them left a thread that was filtered out of lists but still reported
/// `running`, or vice versa. `archived_at` is now a projection of the machine
/// state, never an independent switch.
pub async fn set_archived(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    archived: bool,
) -> Result<ThreadStatus, ServerError> {
    let current = status(pool, thread_id).await?;
    let action = if archived {
        ThreadAction::Archive
    } else {
        ThreadAction::Unarchive
    };
    let next = ThreadMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query("UPDATE threads SET status = ?, archived_at = ?, updated_at = ? WHERE id = ?")
        .bind(threadbase_core::state::thread_status_str(next))
        .bind(archived.then(now_ms))
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

/// Resets threads that were mid-turn when the server stopped.
///
/// The agent died with the old process, so the stored status would show work
/// nobody is doing. The dangling turn gets a `TurnError` and the thread goes back
/// to idle. Returns each thread and the turn that was cut, if one had started.
pub async fn reconcile_after_restart(
    pool: &SqlitePool,
    events: &EventStore,
    projection: &Projection,
) -> Result<Vec<(String, Option<String>)>, ServerError> {
    let rows: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, current_turn_id, status FROM threads
          WHERE archived_at IS NULL
            AND (status IN ('preparing', 'running', 'requires_action')
                 OR current_turn_id IS NOT NULL)
          ORDER BY updated_at",
    )
    .fetch_all(pool)
    .await?;
    let mut reconciled = Vec::with_capacity(rows.len());
    for (thread_id, turn_id, status) in rows {
        let interrupted = matches!(status.as_str(), "preparing" | "running" | "requires_action");
        if !interrupted {
            // The turn row committed but the Prompt transition never did, so no
            // prompt reached the adapter. Drop the unstarted turn instead of
            // reporting it as lost; the queue re-runs the input that made it.
            if let Some(turn_id) = turn_id.as_deref() {
                discard_unstarted_turn(pool, &thread_id, turn_id).await;
            }
            sqlx::query(
                "UPDATE threads SET status = ?, current_turn_id = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(threadbase_core::state::thread_status_str(ThreadStatus::Idle))
            .bind(now_ms())
            .bind(&thread_id)
            .execute(pool)
            .await?;
            projection.publish(&thread_id).await;
            reconciled.push((thread_id, None));
            continue;
        }
        let message = "Threadbase restarted before this turn finished.".to_owned();
        let payload = match turn_id.as_deref() {
            Some(turn_id) => ThreadEvent::TurnError {
                turn_id: Some(TurnId::from(turn_id)),
                message,
            },
            None => ThreadEvent::SystemNotice {
                level: "warning".to_owned(),
                message,
            },
        };
        // Best effort: recording the notice must never keep a thread stuck.
        if let Err(error) = events
            .append(
                &ThreadId::from(thread_id.clone()),
                Actor::system(),
                payload,
                EventMeta::default(),
            )
            .await
        {
            tracing::warn!(thread_id, %error, "could not record the restart notice");
        }
        sqlx::query(
            "UPDATE threads SET status = ?, current_turn_id = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(threadbase_core::state::thread_status_str(
            ThreadStatus::Idle,
        ))
        .bind(now_ms())
        .bind(&thread_id)
        .execute(pool)
        .await?;
        let _ = sqlx::query(
            "UPDATE subagents SET status = ?, updated_at = ?
              WHERE thread_id = ? AND status IN (?, ?)",
        )
        .bind(subagent_status_str(SubagentStatus::Error))
        .bind(now_ms())
        .bind(&thread_id)
        .bind(subagent_status_str(SubagentStatus::Running))
        .bind(subagent_status_str(SubagentStatus::RequiresAction))
        .execute(pool)
        .await;
        projection.publish(&thread_id).await;
        reconciled.push((thread_id, turn_id));
    }
    Ok(reconciled)
}

/// Remove the `TurnStarted` event of a turn that was recorded but never
/// prompted. Its prompt never reached the adapter, so keeping the row would
/// show the message twice once the queue re-runs it.
async fn discard_unstarted_turn(pool: &SqlitePool, thread_id: &str, turn_id: &str) {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT seq, payload_json FROM events
          WHERE thread_id = ? AND kind = 'turn_started'
          ORDER BY seq DESC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (seq, payload) in rows {
        let Ok(ThreadEvent::TurnStarted {
            turn_id: started, ..
        }) = serde_json::from_str::<ThreadEvent>(&payload)
        else {
            continue;
        };
        if started.as_str() != turn_id {
            continue;
        }
        let _ = sqlx::query("DELETE FROM events WHERE thread_id = ? AND seq = ?")
            .bind(thread_id)
            .bind(seq)
            .execute(pool)
            .await;
        break;
    }
}

async fn insert(pool: &SqlitePool, row: &NewThread<'_>) -> Result<ThreadStatus, ServerError> {
    let status = ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO threads(id, project_id, parent_thread_id, title, status, agent, model, effort,
            permission_mode, branch, base_branch, base_sha, last_turn_sha, worktree_path,
            created_by, created_at, updated_at, placement_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.id)
    .bind(row.project_id)
    .bind(row.parent_thread_id)
    .bind(row.title)
    .bind(threadbase_core::state::thread_status_str(status))
    .bind(row.agent)
    .bind(row.model)
    .bind(row.effort)
    .bind(row.permission_mode)
    .bind(row.branch)
    .bind(row.base_branch)
    .bind(row.base_sha)
    .bind(row.base_sha)
    .bind(row.worktree_path)
    .bind(row.created_by)
    .bind(row.created_at)
    .bind(row.created_at)
    // A thread starts on the active shelf, and "when it got there" is when it
    // was created; the column default of 0 exists only for the migration.
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
    policy: &PermissionPolicy,
) -> Result<ThreadDto, ServerError> {
    let project = projects::row(pool, project_id).await?;
    let project_root = PathBuf::from(&project.path);
    if let Err(error) = git::inspect(&project_root).await {
        return Err(ServerError(ApiError::validation(
            projects::inspect_message(&error),
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
    let use_worktree = request.worktree.unwrap_or(true);
    let branch = if use_worktree {
        format!("threadbase/{thread_id}")
    } else {
        base_branch.clone()
    };
    let worktree = use_worktree.then(|| data_dir.join("worktrees").join(&thread_id));
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "New thread".to_owned());
    let now = now_ms();
    let worktree_path = worktree
        .as_ref()
        .map(|path| path.to_string_lossy().to_string());
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
            permission_mode: None,
            branch: &branch,
            base_branch: &base_branch,
            base_sha: &base_sha,
            worktree_path: worktree_path.as_deref(),
            created_by: user.user.id.as_str(),
            created_at: now,
        },
    )
    .await?;
    watchers::watch(pool, &thread_id, user.user.id.as_str()).await?;

    if let Some(worktree) = worktree.as_deref() {
        if let Err(error) = git::create_worktree(&project_root, worktree, &branch, &base_sha).await
        {
            sqlx::query("DELETE FROM threads WHERE id = ?")
                .bind(&thread_id)
                .execute(pool)
                .await?;
            let _ = tokio::fs::remove_dir_all(worktree).await;
            return Err(ServerError(ApiError::validation(format!(
                "could not create worktree: {error}"
            ))));
        }
    }

    let agent_cwd = worktree_path.as_deref().unwrap_or(&project.path);
    bindings::insert(pool, &thread_id, &agent, agent_cwd, now).await?;
    transition(pool, projection, &thread_id, ThreadAction::StartSucceeded).await?;
    let _ = crate::search::index_thread(pool, &thread_id).await;

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
                attachments_json: None,
                starting: false,
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
    get(pool, &thread_id, user, policy).await
}

pub async fn update(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    user: &CurrentUser,
    request: &UpdateThreadRequest,
    policy: &PermissionPolicy,
) -> Result<ThreadDto, ServerError> {
    if request.title.is_some() || request.pinned.is_some() {
        authz::require_write_role(pool, thread_id, user).await?;
    }
    if request.archived.is_some() {
        authz::require_manage_role(pool, thread_id, user).await?;
    }
    if request.auto_settle.is_some() || request.permission_mode.is_some() {
        authz::require_write_role(pool, thread_id, user).await?;
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
        let _ = crate::search::index_thread(pool, thread_id).await;
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
        set_archived(pool, projection, thread_id, archived).await?;
    }
    if let Some(mode) = request.permission_mode.as_deref() {
        let mode = mode.trim();
        let value = if mode.is_empty() {
            None
        } else {
            Some(
                mode.parse::<PermissionMode>()
                    .map_err(|_| {
                        ServerError(ApiError::validation(format!(
                            "unknown permission mode: {mode}"
                        )))
                    })?
                    .as_str(),
            )
        };
        sqlx::query("UPDATE threads SET permission_mode = ?, updated_at = ? WHERE id = ?")
            .bind(value)
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
    }
    if let Some(auto_settle) = request.auto_settle {
        sqlx::query("UPDATE threads SET auto_settle = ?, updated_at = ? WHERE id = ?")
            .bind(auto_settle.as_str())
            .bind(now_ms())
            .bind(thread_id)
            .execute(pool)
            .await?;
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
    get(pool, thread_id, user, policy).await
}

/// Where a fork's transcript and files stop.
struct ForkPoint {
    /// Events at or after this sequence belong to the branch the fork leaves
    /// behind. `None` keeps the whole thread as it stands now.
    before_seq: Option<u64>,
    /// The next turn's checkpoint: the commit holding the files right after the
    /// chosen turn. `None` when the source has no snapshot at that point.
    checkpoint: Option<String>,
}

/// Fork after the turn behind `message_id`: transcript before the next turn,
/// files at that turn's checkpoint. The checkpoint of turn N is the snapshot
/// taken before N started, which is the file state right after turn N-1.
async fn fork_point(
    events: &EventStore,
    pool: &SqlitePool,
    thread_id: &ThreadId,
    message_id: Option<&str>,
) -> Result<ForkPoint, ServerError> {
    let Some(message_id) = message_id else {
        return Ok(ForkPoint {
            before_seq: None,
            checkpoint: None,
        });
    };
    let turn = events
        .turn_started(thread_id, message_id)
        .await?
        .ok_or_else(|| {
            ServerError(ApiError::not_found(
                "message not found or it is not a user prompt",
            ))
        })?;
    let next: Option<(i64, String)> = sqlx::query_as(
        "SELECT seq, json_extract(payload_json, '$.turnId') FROM events
          WHERE thread_id = ? AND kind = 'turn_started' AND seq > ?
          ORDER BY seq ASC LIMIT 1",
    )
    .bind(thread_id.as_str())
    .bind(i64::try_from(turn.seq).unwrap_or(i64::MAX))
    .fetch_optional(pool)
    .await?;
    let Some((next_seq, next_turn)) = next else {
        // The chosen prompt is the newest turn: its files are the current ones.
        return Ok(ForkPoint {
            before_seq: None,
            checkpoint: None,
        });
    };
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT sha FROM checkpoints WHERE thread_id = ? AND turn_id = ?")
            .bind(thread_id.as_str())
            .bind(&next_turn)
            .fetch_optional(pool)
            .await?;
    Ok(ForkPoint {
        before_seq: Some(u64::try_from(next_seq).unwrap_or(u64::MAX)),
        checkpoint,
    })
}

/// Carry the copied transcript and the checkpoints of its turns into the fork.
/// The checkpoint commits are ancestors of the fork's start, so per-turn diffs
/// and rewind keep working on the inherited turns.
async fn carry_history(
    pool: &SqlitePool,
    events: &EventStore,
    source: &str,
    target: &str,
    point: &ForkPoint,
) -> Result<(), ServerError> {
    let turn_ids = events
        .copy_transcript(
            &ThreadId::from(source.to_owned()),
            &ThreadId::from(target.to_owned()),
            point.before_seq,
        )
        .await?;
    for turn_id in turn_ids {
        sqlx::query(
            "INSERT INTO checkpoints(thread_id, turn_id, sha, created_at)
             SELECT ?, turn_id, sha, created_at FROM checkpoints
              WHERE thread_id = ? AND turn_id = ?",
        )
        .bind(target)
        .bind(source)
        .bind(&turn_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Undo a half-created fork. The thread row owns the worktree row, events and
/// checkpoints by foreign key, so deleting it is what makes the retry clean.
async fn discard_fork(pool: &SqlitePool, project_root: &Path, worktree: &Path, thread_id: &str) {
    let _ = git::remove_worktree(project_root, worktree, true).await;
    let _ = tokio::fs::remove_dir_all(worktree).await;
    let _ = git::delete_branch(project_root, &format!("threadbase/{thread_id}"), true).await;
    let _ = sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(thread_id)
        .execute(pool)
        .await;
}

#[allow(clippy::too_many_arguments)]
pub async fn fork(
    pool: &SqlitePool,
    events: &EventStore,
    projection: &Projection,
    data_dir: &Path,
    thread_id: &str,
    user: &CurrentUser,
    request: &ForkThreadRequest,
    policy: &PermissionPolicy,
) -> Result<ThreadDto, ServerError> {
    authz::require_write_role(pool, thread_id, user).await?;
    let source = load_row(pool, thread_id).await?;
    let project = projects::row(pool, &source.project_id).await?;
    let project_root = PathBuf::from(&project.path);
    let source_worktree = source.worktree_path.as_deref().map(PathBuf::from);
    let point = fork_point(
        events,
        pool,
        &ThreadId::from(source.id.clone()),
        request.message_id.as_deref(),
    )
    .await?;

    // The fork starts at the turn's checkpoint when there is one. Otherwise it
    // starts from the source's live files, and says so in the copy below when
    // the transcript was cut earlier than those files.
    let (head, base_branch, copy_state) = match point.checkpoint.clone() {
        Some(sha) => (sha, source.base_branch.clone(), None),
        None => match source_worktree.clone() {
            Some(path) => {
                let head = git::head_sha(&path).await.map_err(|error| {
                    ServerError(ApiError::validation(format!(
                        "could not read the source worktree: {error}"
                    )))
                })?;
                (head, source.base_branch.clone(), Some(path))
            }
            None => {
                let head = git::head_sha(&project_root).await.map_err(|error| {
                    ServerError(ApiError::validation(format!(
                        "could not read the project: {error}"
                    )))
                })?;
                let branch = git::current_branch(&project_root).await.map_err(|error| {
                    ServerError::internal(format!("could not read current branch: {error}"))
                })?;
                // A detached project still has files worth forking; the source's
                // recorded branch is the closest honest base in that case.
                let branch = if branch.is_empty() {
                    source.base_branch.clone()
                } else {
                    branch
                };
                (head, branch, Some(project_root.clone()))
            }
        },
    };

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
            // The fork continues this thread, including how much autonomy it
            // was given; inheriting the project default instead could loosen
            // or tighten the permission ladder without anyone asking.
            permission_mode: source.permission_mode.as_deref(),
            branch: &branch,
            base_branch: &base_branch,
            base_sha: &head,
            worktree_path: Some(&worktree_path),
            created_by: user.user.id.as_str(),
            created_at: now,
        },
    )
    .await?;
    if let Err(error) = git::create_worktree(&project_root, &worktree, &branch, &head).await {
        discard_fork(pool, &project_root, &worktree, &new_id).await;
        return Err(ServerError(ApiError::validation(format!(
            "could not create fork worktree: {error}"
        ))));
    }
    if let Some(source_root) = copy_state.as_deref() {
        let copied = async {
            let patch = git::worktree_state_patch(source_root).await?;
            git::apply_patch(&worktree, &patch).await?;
            git::copy_untracked(source_root, &worktree).await
        }
        .await;
        if let Err(error) = copied {
            discard_fork(pool, &project_root, &worktree, &new_id).await;
            return Err(ServerError(ApiError::validation(format!(
                "could not copy the source files into the fork: {error}"
            ))));
        }
    }
    if let Err(error) = carry_history(pool, events, &source.id, &new_id, &point).await {
        discard_fork(pool, &project_root, &worktree, &new_id).await;
        return Err(error);
    }
    if point.before_seq.is_some() && point.checkpoint.is_none() {
        let _ = events
            .append(
                &ThreadId::from(new_id.clone()),
                Actor::system(),
                ThreadEvent::SystemNotice {
                    level: "info".to_owned(),
                    message: "This thread continues the conversation up to the chosen message. \
                              The source thread has no checkpoint for the turn after it, so the \
                              files were copied from the source's current state instead."
                        .to_owned(),
                },
                EventMeta::default(),
            )
            .await;
    }
    bindings::copy(pool, &new_id, &worktree_path, &source.id, now).await?;
    watchers::copy_for_fork(pool, &new_id, source.id.as_str()).await?;
    let _ = crate::search::index_thread(pool, &new_id).await;
    watchers::watch(pool, &new_id, user.user.id.as_str()).await?;
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
    get(pool, &new_id, user, policy).await
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
    if row.worktree_path.is_some() {
        let _ = git::delete_branch(Path::new(&project.path), &row.branch, true).await;
    }
    sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(thread_id)
        .execute(pool)
        .await?;
    let _ = crate::search::remove_thread(pool, thread_id).await;
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
