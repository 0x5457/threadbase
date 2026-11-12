//! Inbox listing and read markers.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::ReadRules;
use threadbase_shared::{now_ms, InboxItemDto, InboxKind, InboxPageDto, UserBrief};

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

pub async fn list(
    pool: &SqlitePool,
    user: &CurrentUser,
    unread_only: bool,
) -> Result<InboxPageDto, ServerError> {
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
         ORDER BY i.created_at DESC LIMIT 200",
    )
    .bind(user.user.id.as_str())
    .bind(i64::from(unread_only))
    .fetch_all(pool)
    .await?;
    let mut unread = 0;
    let items = rows
        .into_iter()
        .map(|row| {
            if ReadRules::inbox_unread(row.read_at) {
                unread += 1;
            }
            InboxItemDto {
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
            }
        })
        .collect();
    Ok(InboxPageDto { items, unread })
}

pub async fn unread_count(pool: &SqlitePool, user_id: &str) -> Result<u32, ServerError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM inbox WHERE user_id = ? AND read_at IS NULL")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(u32::try_from(count).unwrap_or(0))
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

pub async fn clear_for_approval(pool: &SqlitePool, approval_id: &str) -> Result<(), ServerError> {
    sqlx::query("DELETE FROM inbox WHERE approval_id = ?")
        .bind(approval_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Ask every editor/owner of a thread for edit access.
pub async fn request_access(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<(), ServerError> {
    let members: Vec<(String, String)> =
        sqlx::query_as("SELECT user_id, role FROM thread_members WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_all(pool)
            .await?;
    let actor = UserBrief::from_user(&user.user);
    for (user_id, role) in members {
        if user_id == user.user.id.as_str() || role == "viewer" {
            continue;
        }
        crate::notify::insert(
            pool,
            crate::notify::InboxItem {
                user_id: &user_id,
                kind: "access",
                thread_id,
                actor_id: Some(user.user.id.as_str()),
                approval_id: None,
                elicitation_id: None,
                comment_id: None,
                summary: &format!("{} requested edit access", actor.display_name),
            },
        )
        .await?;
    }
    Ok(())
}

fn parse_kind(value: &str) -> InboxKind {
    match value {
        "approval" => InboxKind::Approval,
        "elicitation" => InboxKind::Elicitation,
        "mention" => InboxKind::Mention,
        "invite" => InboxKind::Invite,
        "access" => InboxKind::Access,
        _ => InboxKind::Error,
    }
}
