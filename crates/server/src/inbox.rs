//! Inbox listing, pagination and read markers.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::ReadRules;
use threadbase_shared::{now_ms, InboxItemDto, InboxKind, InboxPageDto, UserBrief};

/// Default page size; callers may ask for less but never for an unbounded page.
pub const PAGE: usize = 50;
const MAX_PAGE: usize = 200;

#[derive(FromRow)]
struct InboxRow {
    id: String,
    kind: String,
    thread_id: String,
    thread_title: String,
    project_id: String,
    summary: String,
    created_at: i64,
    read_at: Option<i64>,
    approval_id: Option<String>,
    elicitation_id: Option<String>,
    comment_id: Option<String>,
    actor_id: Option<String>,
    actor_name: Option<String>,
    actor_username: Option<String>,
    actor_avatar: Option<String>,
}

/// One page of inbox rows, newest first. `before` is the id of the last row
/// the caller already has; `has_more` means another page exists before it.
pub async fn list(
    pool: &SqlitePool,
    user: &CurrentUser,
    unread_only: bool,
    before: Option<&str>,
    limit: usize,
) -> Result<InboxPageDto, ServerError> {
    let limit = limit.clamp(1, MAX_PAGE);
    let cursor: Option<(i64, String)> = if let Some(id) = before {
        sqlx::query_as("SELECT created_at, id FROM inbox WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user.user.id.as_str())
            .fetch_optional(pool)
            .await?
    } else {
        None
    };
    let (has_before, cursor_ts, cursor_id) = match cursor {
        Some((ts, id)) => (1_i64, ts, id),
        None => (0_i64, 0_i64, String::new()),
    };
    let rows = sqlx::query_as::<_, InboxRow>(
        "SELECT i.id, i.kind, i.thread_id, t.title AS thread_title, t.project_id,
                i.summary, i.created_at, i.read_at, i.approval_id, i.elicitation_id, i.comment_id,
                i.actor_id, COALESCE(u.display_name, u2.display_name) AS actor_name,
                COALESCE(u.username, u2.username) AS actor_username,
                COALESCE(u.avatar_url, u2.avatar_url) AS actor_avatar
         FROM inbox i
         JOIN threads t ON t.id = i.thread_id
         LEFT JOIN users u ON u.id = i.actor_id
         LEFT JOIN users u2 ON u2.id = t.created_by
         WHERE i.user_id = ? AND (? = 0 OR i.read_at IS NULL)
           AND (? = 0 OR i.created_at < ?
                OR (i.created_at = ? AND i.id < ?))
         ORDER BY i.created_at DESC, i.id DESC
         LIMIT ?",
    )
    .bind(user.user.id.as_str())
    .bind(i64::from(unread_only))
    .bind(has_before)
    .bind(cursor_ts)
    .bind(cursor_ts)
    .bind(cursor_id)
    .bind(i64::try_from(limit + 1).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await?;
    let has_more = rows.len() > limit;
    let unread = unread_count(pool, user.user.id.as_str()).await?;
    let items = rows
        .into_iter()
        .take(limit)
        .map(|row| InboxItemDto {
            id: row.id,
            kind: parse_kind(&row.kind),
            actor: row.actor_id.map(|id| UserBrief {
                id: id.clone().into(),
                username: row.actor_username.unwrap_or_default(),
                display_name: row.actor_name.unwrap_or_default(),
                avatar_url: row.actor_avatar,
                color: threadbase_shared::user_color(&id).to_owned(),
            }),
            thread_id: row.thread_id.into(),
            thread_title: row.thread_title,
            project_id: row.project_id.into(),
            summary: row.summary,
            created_at: row.created_at,
            read: !ReadRules::inbox_unread(row.read_at),
            approval_id: row.approval_id.map(Into::into),
            elicitation_id: row.elicitation_id.map(Into::into),
            comment_id: row.comment_id.map(Into::into),
        })
        .collect();
    Ok(InboxPageDto {
        items,
        unread,
        has_more,
    })
}

pub async fn unread_count(pool: &SqlitePool, user_id: &str) -> Result<u32, ServerError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM inbox WHERE user_id = ? AND read_at IS NULL")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(u32::try_from(count).unwrap_or(0))
}

/// Push the authoritative unread count to that user's live sockets.
pub async fn publish_count(pool: &SqlitePool, projection: &Projection, user_id: &str) {
    if let Ok(unread) = unread_count(pool, user_id).await {
        projection.publish_inbox_count(user_id, unread);
    }
}

pub async fn mark_read(
    pool: &SqlitePool,
    user: &CurrentUser,
    thread_id: Option<&str>,
    all: bool,
) -> Result<u32, ServerError> {
    let updated = if all || thread_id.is_none() {
        sqlx::query("UPDATE inbox SET read_at = ? WHERE user_id = ? AND read_at IS NULL")
            .bind(now_ms())
            .bind(user.user.id.as_str())
            .execute(pool)
            .await?
    } else {
        sqlx::query(
            "UPDATE inbox SET read_at = ? WHERE user_id = ? AND thread_id = ? AND read_at IS NULL",
        )
        .bind(now_ms())
        .bind(user.user.id.as_str())
        .bind(thread_id)
        .execute(pool)
        .await?
    };
    Ok(u32::try_from(updated.rows_affected()).unwrap_or(0))
}

/// Remove approval notifications and refresh each affected user's badge.
pub async fn clear_for_approval(
    pool: &SqlitePool,
    projection: &Projection,
    approval_id: &str,
) -> Result<(), ServerError> {
    let users: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT user_id FROM inbox WHERE approval_id = ?")
            .bind(approval_id)
            .fetch_all(pool)
            .await?;
    sqlx::query("DELETE FROM inbox WHERE approval_id = ?")
        .bind(approval_id)
        .execute(pool)
        .await?;
    for user_id in users {
        publish_count(pool, projection, &user_id).await;
    }
    Ok(())
}

/// Remove elicitation notifications and refresh each affected user's badge.
pub async fn clear_for_elicitation(
    pool: &SqlitePool,
    projection: &Projection,
    elicitation_id: &str,
) -> Result<(), ServerError> {
    let users: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT user_id FROM inbox WHERE elicitation_id = ?")
            .bind(elicitation_id)
            .fetch_all(pool)
            .await?;
    sqlx::query("DELETE FROM inbox WHERE elicitation_id = ?")
        .bind(elicitation_id)
        .execute(pool)
        .await?;
    for user_id in users {
        publish_count(pool, projection, &user_id).await;
    }
    Ok(())
}

fn parse_kind(value: &str) -> InboxKind {
    match value {
        "approval" => InboxKind::Approval,
        "elicitation" => InboxKind::Elicitation,
        "mention" => InboxKind::Mention,
        "completed" => InboxKind::Completed,
        _ => InboxKind::Error,
    }
}
