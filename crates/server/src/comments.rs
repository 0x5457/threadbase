//! Thread comments and their semantic anchors.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::events::EventStore;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{comment_action_for, parse_comment_status};
use threadbase_shared::api::{CreateCommentRequest, UpdateCommentRequest};
use threadbase_shared::{
    now_ms, user_color, Actor, ApiError, BaselineDto, BaselineKind, CommentAnchor, CommentDto,
    CommentStatus, ThreadEvent, ThreadId, UserBrief,
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

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<CommentDto>, ServerError> {
    let mut rows = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.thread_id, c.parent_id, c.author_id, c.body, c.anchor_json,
                c.anchor_changed, c.status, c.delivered_seq, c.created_at, c.updated_at,
                u.username, u.display_name, u.avatar_url
         FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.thread_id = ?
         ORDER BY c.created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    reconcile_anchors(pool, thread_id, &mut rows).await;
    Ok(rows.into_iter().map(dto).collect())
}

/// A diff comment stores both the hunk id and a hash of the hunk text. Hunk
/// ids are positional and shift when an earlier hunk changes. On every read we
/// re-resolve the hash: if the same hunk content exists under a new id we move
/// the anchor; otherwise we degrade it to a file-level anchor and persist that
/// fact so the UI can explain the loss of precision.
async fn reconcile_anchors(pool: &SqlitePool, thread_id: &str, rows: &mut [CommentRow]) {
    if !rows
        .iter()
        .any(|row| row.anchor_json.contains("\"kind\":\"diff\""))
    {
        return;
    }
    let Ok(thread) = crate::threads::load_row(pool, thread_id).await else {
        return;
    };
    let baseline = BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    };
    for row in rows.iter_mut() {
        let Ok(CommentAnchor::Diff {
            path,
            line,
            context_hash,
            ..
        }) = serde_json::from_str::<CommentAnchor>(&row.anchor_json)
        else {
            continue;
        };
        let Ok(diff) = crate::diff::file_diff(&thread, &baseline, &path).await else {
            continue;
        };
        let found = context_hash
            .as_deref()
            .and_then(|hash| diff.hunks.iter().find(|hunk| hunk.context_hash == hash));
        let next = match found {
            Some(hunk) => CommentAnchor::Diff {
                path,
                hunk_id: hunk.id.clone(),
                line,
                context_hash,
            },
            None => CommentAnchor::File {
                path,
                line,
                context_hash,
            },
        };
        let changed = !matches!(next, CommentAnchor::Diff { .. });
        let serialized = serde_json::to_string(&next).unwrap_or_else(|_| row.anchor_json.clone());
        let changed = i64::from(changed);
        if (serialized != row.anchor_json || changed != row.anchor_changed)
            && sqlx::query(
                "UPDATE comments SET anchor_json = ?, anchor_changed = ?, updated_at = ? WHERE id = ?",
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

fn dto(row: CommentRow) -> CommentDto {
    let anchor =
        serde_json::from_str::<CommentAnchor>(&row.anchor_json).unwrap_or(CommentAnchor::Thread);
    CommentDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        parent_id: row.parent_id.map(Into::into),
        author: UserBrief {
            id: row.author_id.clone().into(),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            color: user_color(&row.author_id).to_owned(),
        },
        body: row.body,
        anchor,
        anchor_changed: row.anchor_changed != 0,
        status: parse_comment_status(&row.status).unwrap_or(CommentStatus::Pending),
        created_at: row.created_at,
        updated_at: row.updated_at,
        delivered_seq: row
            .delivered_seq
            .and_then(|value| u64::try_from(value).ok()),
    }
}

pub async fn create(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
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
    let target = if request.send {
        CommentStatus::Delivered
    } else {
        CommentStatus::Pending
    };
    let anchor_json =
        serde_json::to_string(&request.anchor).unwrap_or_else(|_| "{\"kind\":\"thread\"}".into());
    state_store
        .insert_comment(
            &id,
            thread_id,
            request.parent_id.as_ref().map(|value| value.as_str()),
            user.user.id.as_str(),
            body,
            anchor_kind(&request.anchor),
            &anchor_json,
            target,
            now,
        )
        .await?;
    let comment = load(pool, &id).await?;
    events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::user(&brief(&user.user)),
            ThreadEvent::CommentAdded {
                comment: comment.clone(),
            },
            Default::default(),
        )
        .await?;
    notify_mentions(pool, thread_id, body, &id, &user.user).await?;
    Ok(comment)
}

pub async fn update(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
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
        state_store
            .set_comment_status(current.thread_id.as_str(), comment_id, action)
            .await?;
    }
    let comment = load(pool, comment_id).await?;
    events
        .append(
            &ThreadId::from(comment.thread_id.0.clone()),
            Actor::user(&brief(&user.user)),
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
    sqlx::query("DELETE FROM comments WHERE id = ?")
        .bind(comment_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_delivered(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &EventStore,
    comment_id: &str,
    user: &CurrentUser,
    delivered_seq: Option<u64>,
) -> Result<CommentDto, ServerError> {
    let current = load(pool, comment_id).await?;
    state_store
        .mark_comment_delivered(current.thread_id.as_str(), comment_id, delivered_seq)
        .await?;
    let comment = load(pool, comment_id).await?;
    events
        .append(
            &ThreadId::from(comment.thread_id.0.clone()),
            Actor::user(&brief(&user.user)),
            ThreadEvent::CommentUpdated {
                comment: comment.clone(),
            },
            Default::default(),
        )
        .await?;
    Ok(comment)
}

pub async fn load(pool: &SqlitePool, comment_id: &str) -> Result<CommentDto, ServerError> {
    let row = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.thread_id, c.parent_id, c.author_id, c.body, c.anchor_json,
                c.anchor_changed, c.status, c.delivered_seq, c.created_at, c.updated_at,
                u.username, u.display_name, u.avatar_url
         FROM comments c JOIN users u ON u.id = c.author_id WHERE c.id = ?",
    )
    .bind(comment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ServerError(ApiError::not_found("comment not found")))?;
    Ok(dto(row))
}

async fn notify_mentions(
    pool: &SqlitePool,
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
        if let Some(target) = crate::auth::load_user_by_username(pool, username).await? {
            if target.id == actor.id {
                continue;
            }
            sqlx::query(
                "INSERT INTO inbox(id, user_id, kind, thread_id, comment_id, actor_id, summary, created_at)
                 VALUES(?, ?, 'mention', ?, ?, ?, ?, ?)",
            )
            .bind(Ulid::new().to_string())
            .bind(target.id.as_str())
            .bind(thread_id)
            .bind(comment_id)
            .bind(actor.id.as_str())
            .bind(format!("{} mentioned you", actor.display_name))
            .bind(now_ms())
            .execute(pool)
            .await?;
        }
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

fn brief(user: &threadbase_shared::UserDto) -> threadbase_shared::UserBrief {
    threadbase_shared::UserBrief {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        color: user_color(user.id.as_str()).to_owned(),
    }
}

pub async fn pending_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<CommentDto>, ServerError> {
    let mut rows = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.thread_id, c.parent_id, c.author_id, c.body, c.anchor_json,
                c.anchor_changed, c.status, c.delivered_seq, c.created_at, c.updated_at,
                u.username, u.display_name, u.avatar_url
         FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.thread_id = ? AND c.status = 'pending'
         ORDER BY c.created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    reconcile_anchors(pool, thread_id, &mut rows).await;
    Ok(rows.into_iter().map(dto).collect())
}

pub async fn mark_pending_delivered(
    pool: &SqlitePool,
    state_store: &crate::state::StateStore,
    events: &EventStore,
    thread_id: &str,
    user: &CurrentUser,
    delivered_seq: Option<u64>,
) -> Result<(), ServerError> {
    let pending = pending_for_thread(pool, thread_id).await?;
    for comment in pending {
        let comment = mark_delivered(
            pool,
            state_store,
            events,
            comment.id.as_str(),
            user,
            delivered_seq,
        )
        .await?;
        let _ = comment;
    }
    Ok(())
}
