//! Thread comments and their semantic anchors.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::EventStore;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;
use std::collections::HashSet;
use threadbase_core::state::{
    comment_action_for, comment_status_str, parse_comment_status, CommentAction, CommentMachine,
};
use threadbase_shared::api::{CreateCommentRequest, UpdateCommentRequest};
use threadbase_shared::{
    now_ms, Actor, ApiError, CommentAnchor, CommentDto, CommentStatus, ThreadEvent, ThreadId,
    UserBrief,
};
use ulid::Ulid;

#[derive(Debug, FromRow)]
struct CommentRow {
    id: String,
    thread_id: String,
    parent_id: Option<String>,
    author_id: String,
    body: String,
    anchor_json: String,
    anchor_changed: i64,
    status: String,
    delivered_seq: Option<i64>,
    created_at: i64,
    updated_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

const SELECT: &str = "SELECT c.id, c.thread_id, c.parent_id, c.author_id, c.body, c.anchor_json,
            c.anchor_changed, c.status, c.delivered_seq, c.created_at, c.updated_at,
            u.username, u.display_name, u.avatar_url
     FROM comments c JOIN users u ON u.id = c.author_id";

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<CommentDto>, ServerError> {
    let mut rows = sqlx::query_as::<_, CommentRow>(&format!(
        "{SELECT} WHERE c.thread_id = ? ORDER BY c.created_at ASC"
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    reconcile_anchors(pool, thread_id, &mut rows).await;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub async fn pending_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<CommentDto>, ServerError> {
    let rows = sqlx::query_as::<_, CommentRow>(&format!(
        "{SELECT} WHERE c.thread_id = ? AND c.status = 'pending' ORDER BY c.created_at ASC"
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    // The send path only needs the stored anchors to build the prompt, so it
    // does not reconcile them; that cost belongs to the thread list read.
    Ok(rows.into_iter().map(to_dto).collect())
}

pub async fn load(pool: &SqlitePool, comment_id: &str) -> Result<CommentDto, ServerError> {
    let row = sqlx::query_as::<_, CommentRow>(&format!("{SELECT} WHERE c.id = ?"))
        .bind(comment_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("comment not found")))?;
    Ok(to_dto(row))
}

pub async fn create(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    thread_id: &str,
    user: &CurrentUser,
    request: &CreateCommentRequest,
) -> Result<CommentDto, ServerError> {
    let body = request.body.trim();
    if body.is_empty() {
        return Err(ServerError(ApiError::validation("comment cannot be empty")));
    }
    if body.len() > 16 * 1024 {
        return Err(ServerError(ApiError::validation("comment is too long")));
    }
    let id = Ulid::new().to_string();
    let now = now_ms();
    let status = if request.send {
        CommentStatus::Delivered
    } else {
        CommentStatus::Pending
    };
    insert(
        pool,
        &id,
        thread_id,
        request.parent_id.as_ref().map(|value| value.as_str()),
        user.user.id.as_str(),
        body,
        anchor_kind(&request.anchor),
        &serde_json::to_string(&request.anchor).unwrap_or_else(|_| "{\"kind\":\"thread\"}".into()),
        status,
        now,
    )
    .await?;
    crate::watchers::watch(pool, thread_id, user.user.id.as_str()).await?;
    let comment = load(pool, &id).await?;
    events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::user(&UserBrief::from_user(&user.user)),
            ThreadEvent::CommentAdded {
                comment: comment.clone(),
            },
            Default::default(),
        )
        .await?;
    notify_mentions(pool, projection, thread_id, body, &id, &user.user).await?;
    Ok(comment)
}

pub async fn update(
    pool: &SqlitePool,
    events: &EventStore,
    comment_id: &str,
    user: &CurrentUser,
    request: &UpdateCommentRequest,
) -> Result<CommentDto, ServerError> {
    let current = load(pool, comment_id).await?;
    if current.author.id != user.user.id
        && !user.is_admin()
        && !crate::authz::require_manage_role(pool, current.thread_id.as_str(), user)
            .await
            .is_ok()
    {
        return Err(ServerError(ApiError::forbidden(
            "only the author or a thread owner can edit this comment",
        )));
    }
    if let Some(body) = request.body.as_deref() {
        let body = body.trim();
        if body.is_empty() {
            return Err(ServerError(ApiError::validation("comment cannot be empty")));
        }
        sqlx::query("UPDATE comments SET body = ?, updated_at = ? WHERE id = ?")
            .bind(body)
            .bind(now_ms())
            .bind(comment_id)
            .execute(pool)
            .await?;
    }
    if let Some(status) = request.status {
        let action = comment_action_for(status)
            .ok_or_else(|| ServerError::internal("cannot transition to draft"))?;
        transition(pool, comment_id, action).await?;
    }
    let comment = load(pool, comment_id).await?;
    events
        .append(
            &ThreadId::from(comment.thread_id.0.clone()),
            Actor::user(&UserBrief::from_user(&user.user)),
            ThreadEvent::CommentUpdated {
                comment: comment.clone(),
            },
            Default::default(),
        )
        .await?;
    Ok(comment)
}

pub async fn delete(
    pool: &SqlitePool,
    comment_id: &str,
    user: &CurrentUser,
) -> Result<(), ServerError> {
    let current = load(pool, comment_id).await?;
    if current.author.id != user.user.id && !user.is_admin() {
        return Err(ServerError(ApiError::forbidden(
            "only the author can delete this comment",
        )));
    }
    let thread_id = current.thread_id.0.clone();
    sqlx::query("DELETE FROM comments WHERE id = ?")
        .bind(comment_id)
        .execute(pool)
        .await?;
    let _ = crate::search::remove_comment(pool, &thread_id, comment_id).await;
    Ok(())
}

pub async fn mark_delivered(
    pool: &SqlitePool,
    events: &EventStore,
    comment_id: &str,
    user: &CurrentUser,
    delivered_seq: Option<u64>,
) -> Result<CommentDto, ServerError> {
    set_delivered(pool, comment_id, delivered_seq).await?;
    let comment = load(pool, comment_id).await?;
    events
        .append(
            &ThreadId::from(comment.thread_id.0.clone()),
            Actor::user(&UserBrief::from_user(&user.user)),
            ThreadEvent::CommentUpdated {
                comment: comment.clone(),
            },
            Default::default(),
        )
        .await?;
    Ok(comment)
}

pub async fn mark_pending_delivered(
    pool: &SqlitePool,
    events: &EventStore,
    pending: &[CommentDto],
    user: &CurrentUser,
    delivered_seq: Option<u64>,
) -> Result<(), ServerError> {
    let Some(first) = pending.first() else {
        return Ok(());
    };
    // One UPDATE for the whole thread; RETURNING tells us which comments were
    // still pending when it ran.
    let updated: Vec<String> = sqlx::query_scalar(
        "UPDATE comments SET status = ?, delivered_seq = ?, updated_at = ?
         WHERE thread_id = ? AND status = ?
         RETURNING id",
    )
    .bind(comment_status_str(CommentStatus::Delivered))
    .bind(delivered_seq.and_then(|value| i64::try_from(value).ok()))
    .bind(now_ms())
    .bind(first.thread_id.as_str())
    .bind(comment_status_str(CommentStatus::Pending))
    .fetch_all(pool)
    .await?;
    let ids: HashSet<&str> = updated.iter().map(String::as_str).collect();
    for comment in pending {
        if !ids.contains(comment.id.as_str()) {
            continue;
        }
        let mut delivered = comment.clone();
        delivered.status = CommentStatus::Delivered;
        delivered.delivered_seq = delivered_seq;
        events
            .append(
                &ThreadId::from(delivered.thread_id.0.clone()),
                Actor::user(&UserBrief::from_user(&user.user)),
                ThreadEvent::CommentUpdated { comment: delivered },
                Default::default(),
            )
            .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert(
    pool: &SqlitePool,
    id: &str,
    thread_id: &str,
    parent_id: Option<&str>,
    author_id: &str,
    body: &str,
    anchor_kind: &str,
    anchor_json: &str,
    status: CommentStatus,
    created_at: i64,
) -> Result<CommentStatus, ServerError> {
    let action = match status {
        CommentStatus::Pending => CommentAction::CreatePending,
        CommentStatus::Delivered => CommentAction::CreateDelivered,
        _ => {
            return Err(ServerError::internal(
                "cannot create a comment in this state",
            ))
        }
    };
    let next = CommentMachine::transition(CommentStatus::Pending, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO comments(id, thread_id, parent_id, author_id, body, anchor_kind, anchor_json,
            anchor_changed, status, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(id)
    .bind(thread_id)
    .bind(parent_id)
    .bind(author_id)
    .bind(body)
    .bind(anchor_kind)
    .bind(anchor_json)
    .bind(comment_status_str(next))
    .bind(created_at)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(next)
}

async fn transition(
    pool: &SqlitePool,
    comment_id: &str,
    action: CommentAction,
) -> Result<CommentStatus, ServerError> {
    let current: Option<String> = sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
        .bind(comment_id)
        .fetch_optional(pool)
        .await?;
    let current = current
        .as_deref()
        .and_then(parse_comment_status)
        .ok_or_else(|| ServerError::internal("unknown comment status"))?;
    let next = CommentMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    sqlx::query("UPDATE comments SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
        .bind(comment_status_str(next))
        .bind(now_ms())
        .bind(comment_id)
        .bind(comment_status_str(current))
        .execute(pool)
        .await?;
    Ok(next)
}

async fn set_delivered(
    pool: &SqlitePool,
    comment_id: &str,
    delivered_seq: Option<u64>,
) -> Result<CommentStatus, ServerError> {
    let current: Option<String> = sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
        .bind(comment_id)
        .fetch_optional(pool)
        .await?;
    let current = current
        .as_deref()
        .and_then(parse_comment_status)
        .ok_or_else(|| ServerError::internal("unknown comment status"))?;
    let next = CommentMachine::transition(current, &CommentAction::Deliver)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    sqlx::query(
        "UPDATE comments SET status = ?, delivered_seq = ?, updated_at = ? WHERE id = ? AND status = ?",
    )
    .bind(comment_status_str(next))
    .bind(delivered_seq.and_then(|value| i64::try_from(value).ok()))
    .bind(now_ms())
    .bind(comment_id)
    .bind(comment_status_str(current))
    .execute(pool)
    .await?;
    Ok(next)
}

/// Hunk ids are positional, so an earlier edit shifts them. The stored text hash
/// finds the same hunk under its new id; when the content is gone the anchor
/// degrades to the file and `anchorChanged` tells the UI.
async fn reconcile_anchors(pool: &SqlitePool, thread_id: &str, rows: &mut [CommentRow]) {
    let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let Ok(CommentAnchor::Diff { path, .. }) =
            serde_json::from_str::<CommentAnchor>(&row.anchor_json)
        else {
            continue;
        };
        by_path.entry(path).or_default().push(index);
    }
    if by_path.is_empty() {
        return;
    }
    let Ok(thread) = crate::threads::load_row(pool, thread_id).await else {
        return;
    };
    let Ok(base) = crate::diff::anchor_base(&thread) else {
        return;
    };
    for (path, indices) in by_path {
        let Ok(hunks) = crate::diff::hunk_index(&base, &path).await else {
            continue;
        };
        for index in indices {
            let row = &mut rows[index];
            let Ok(CommentAnchor::Diff {
                path,
                line,
                side,
                context_hash,
                ..
            }) = serde_json::from_str::<CommentAnchor>(&row.anchor_json)
            else {
                continue;
            };
            let found = context_hash.as_deref().and_then(|hash| hunks.get(hash));
            let next = match found {
                Some(hunk_id) => CommentAnchor::Diff {
                    path,
                    hunk_id: hunk_id.clone(),
                    line,
                    side,
                    context_hash,
                },
                None => CommentAnchor::File {
                    path,
                    line,
                    context_hash,
                },
            };
            let changed = !matches!(next, CommentAnchor::Diff { .. });
            let serialized =
                serde_json::to_string(&next).unwrap_or_else(|_| row.anchor_json.clone());
            let changed = i64::from(changed);
            if (serialized != row.anchor_json || changed != row.anchor_changed)
                && sqlx::query(
                    "UPDATE comments SET anchor_json = ?, anchor_changed = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(&serialized)
                .bind(changed)
                .bind(now_ms())
                .bind(&row.id)
                .execute(pool)
                .await
                .is_ok()
            {
                row.anchor_json = serialized;
                row.anchor_changed = changed;
            }
        }
    }
}

fn to_dto(row: CommentRow) -> CommentDto {
    CommentDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        parent_id: row.parent_id.map(Into::into),
        author: UserBrief {
            id: row.author_id.clone().into(),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            color: threadbase_shared::user_color(&row.author_id).to_owned(),
        },
        body: row.body,
        anchor: serde_json::from_str(&row.anchor_json).unwrap_or(CommentAnchor::Thread),
        anchor_changed: row.anchor_changed != 0,
        status: parse_comment_status(&row.status).unwrap_or(CommentStatus::Pending),
        created_at: row.created_at,
        updated_at: row.updated_at,
        delivered_seq: row
            .delivered_seq
            .and_then(|value| u64::try_from(value).ok()),
    }
}

async fn notify_mentions(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    body: &str,
    comment_id: &str,
    actor: &threadbase_shared::UserDto,
) -> Result<(), ServerError> {
    for word in body.split_whitespace() {
        let Some(username) = word.strip_prefix('@') else {
            continue;
        };
        let username = username.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '_'
        });
        if username.is_empty() {
            continue;
        }
        let Some(target) = crate::auth::load_user_by_username(pool, username).await? else {
            continue;
        };
        if target.id == actor.id {
            continue;
        }
        crate::watchers::watch(pool, thread_id, target.id.as_str()).await?;
        crate::notify::notify_user(
            pool,
            projection,
            crate::notify::InboxItem {
                user_id: target.id.as_str(),
                kind: "mention",
                thread_id,
                actor_id: Some(actor.id.as_str()),
                approval_id: None,
                elicitation_id: None,
                comment_id: Some(comment_id),
                summary: &format!("{} mentioned you", actor.display_name),
            },
        )
        .await?;
    }
    Ok(())
}

fn anchor_kind(anchor: &CommentAnchor) -> &'static str {
    match anchor {
        CommentAnchor::Message { .. } => "message",
        CommentAnchor::ToolCall { .. } => "tool_call",
        CommentAnchor::Elicitation { .. } => "elicitation",
        CommentAnchor::File { .. } => "file",
        CommentAnchor::Diff { .. } => "diff",
        CommentAnchor::Thread => "thread",
    }
}
