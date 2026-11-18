//! Guest links: one thread, no account.
//!
//! The thread comes from the link row, never from the request, so a guest
//! cannot address another thread. Guests read the transcript and leave draft
//! messages; a member has to send a draft before it becomes a turn.

use crate::auth_session;
use crate::error::ServerError;
use axum::http::HeaderMap;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::api::GuestMessageDto;
use threadbase_shared::GuestMessageStatus;
use threadbase_shared::{now_ms, ApiError};
use ulid::Ulid;

/// Cookie set after a guest exchanges its link.
pub const COOKIE: &str = "threadbase_guest";
/// How long a link works: 24 hours.
pub const LINK_SECONDS: i64 = 24 * 60 * 60;
/// How many drafts one guest may have waiting.
const MAX_PENDING: i64 = 5;
/// Longest draft accepted, matching what a member can type in one message.
const MAX_TEXT: usize = 8_000;

/// What a guest request resolves to, from its cookie.
#[derive(Debug, Clone, FromRow)]
pub struct GuestContext {
    pub link_id: String,
    pub thread_id: String,
    pub link_expires_at: i64,
    pub session_expires_at: i64,
}

#[must_use]
pub fn enabled(config: &crate::config::Config) -> bool {
    config.guest_links
}

/// Creates a link. The token is returned once, so the caller can build the URL.
pub async fn create_link(
    pool: &SqlitePool,
    thread_id: &str,
    created_by: &str,
) -> Result<(String, i64), ServerError> {
    // One live link per thread: sharing again replaces the previous link.
    revoke_thread_links(pool, thread_id).await?;
    let now = now_ms();
    let token = auth_session::new_token();
    let expires_at = now + LINK_SECONDS * 1000;
    sqlx::query(
        "INSERT INTO guest_links(id, thread_id, token_hash, created_by, created_at, expires_at)
         VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(thread_id)
    .bind(auth_session::hash(&token))
    .bind(created_by)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok((token, expires_at))
}

pub async fn revoke_thread_links(pool: &SqlitePool, thread_id: &str) -> Result<u64, ServerError> {
    let now = now_ms();
    let links = sqlx::query(
        "UPDATE guest_links SET revoked_at = ? WHERE thread_id = ? AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(thread_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE guest_sessions SET expires_at = ?
          WHERE link_id IN (SELECT id FROM guest_links WHERE thread_id = ?)",
    )
    .bind(now)
    .bind(thread_id)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE guest_messages SET status = 'dismissed', decided_at = ? WHERE thread_id = ? AND status = 'pending'")
        .bind(now)
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(links.rows_affected())
}

/// Trades a link token for a browser session. Returns the cookie value and the
/// context, so the first response does not need the cookie it is about to set.
pub async fn exchange(
    pool: &SqlitePool,
    token: &str,
    user_agent: Option<&str>,
) -> Result<(String, GuestContext), ServerError> {
    let now = now_ms();
    let link: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT id, thread_id, expires_at FROM guest_links
          WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(auth_session::hash(token))
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let Some((link_id, thread_id, expires_at)) = link else {
        return Err(ServerError(ApiError::forbidden(
            "this link is no longer valid",
        )));
    };
    let session = auth_session::new_token();
    sqlx::query(
        "INSERT INTO guest_sessions(token_hash, link_id, created_at, expires_at, last_seen_at, user_agent)
         VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(auth_session::hash(&session))
    .bind(&link_id)
    .bind(now)
    .bind(expires_at)
    .bind(now)
    .bind(user_agent)
    .execute(pool)
    .await?;
    Ok((
        session,
        GuestContext {
            link_id,
            thread_id: thread_id.clone(),
            link_expires_at: expires_at,
            session_expires_at: expires_at,
        },
    ))
}

/// The live link for a thread, if any.
pub async fn active_link(pool: &SqlitePool, thread_id: &str) -> Result<Option<i64>, ServerError> {
    let expires_at: Option<i64> = sqlx::query_scalar(
        "SELECT expires_at FROM guest_links
          WHERE thread_id = ? AND revoked_at IS NULL AND expires_at > ?
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(thread_id)
    .bind(now_ms())
    .fetch_optional(pool)
    .await?;
    Ok(expires_at)
}

/// Reads the guest cookie and checks that its link still works.
pub async fn context(pool: &SqlitePool, headers: &HeaderMap) -> Result<GuestContext, ServerError> {
    let Some(token) = auth_session::read_cookie(headers, COOKIE) else {
        return Err(ServerError(ApiError::forbidden(
            "open the link you were given",
        )));
    };
    let now = now_ms();
    let row: Option<GuestContext> = sqlx::query_as(
        "SELECT s.link_id, l.thread_id, l.expires_at AS link_expires_at,
                s.expires_at AS session_expires_at
           FROM guest_sessions s JOIN guest_links l ON l.id = s.link_id
          WHERE s.token_hash = ? AND s.expires_at > ?
            AND l.revoked_at IS NULL AND l.expires_at > ?",
    )
    .bind(auth_session::hash(&token))
    .bind(now)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    row.ok_or_else(|| ServerError(ApiError::forbidden("this link is no longer valid")))
}

/// Stores a draft. Nothing is sent until a member approves it.
pub async fn draft(
    pool: &SqlitePool,
    context: &GuestContext,
    text: &str,
) -> Result<String, ServerError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ServerError(ApiError::validation("write a message first")));
    }
    if text.len() > MAX_TEXT {
        return Err(ServerError(ApiError::validation(
            "that message is too long for a guest note",
        )));
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM guest_messages
          WHERE link_id = ? AND status = 'pending'",
    )
    .bind(&context.link_id)
    .fetch_one(pool)
    .await?;
    if pending >= MAX_PENDING {
        return Err(ServerError(ApiError::conflict(
            "earlier messages are still waiting for a reply; try again once they are handled",
        )));
    }
    let id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO guest_messages(id, thread_id, link_id, text, status, created_at)
         VALUES(?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&id)
    .bind(&context.thread_id)
    .bind(&context.link_id)
    .bind(text)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<GuestMessageDto>, ServerError> {
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, text, status, created_at FROM guest_messages
          WHERE thread_id = ? AND status = 'pending' ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, text, status, created_at)| GuestMessageDto {
            id,
            text,
            status: status.parse().unwrap_or(GuestMessageStatus::Pending),
            created_at,
        })
        .collect())
}

/// Marks a draft as sent and returns its text. Only one caller can win, so two
/// people pressing Send cannot start two turns.
pub async fn claim(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: &str,
    decided_by: &str,
) -> Result<String, ServerError> {
    let now = now_ms();
    let text: Option<String> = sqlx::query_scalar(
        "UPDATE guest_messages SET status = 'sent', decided_at = ?, decided_by = ?
          WHERE id = ? AND thread_id = ? AND status = 'pending'
          RETURNING text",
    )
    .bind(now)
    .bind(decided_by)
    .bind(message_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    text.ok_or_else(|| ServerError(ApiError::conflict("that message is already handled")))
}

/// Puts a claimed draft back when sending failed.
pub async fn release(pool: &SqlitePool, message_id: &str) {
    let _ = sqlx::query(
        "UPDATE guest_messages SET status = 'pending', decided_at = NULL, decided_by = NULL
          WHERE id = ? AND status = 'sent'",
    )
    .bind(message_id)
    .execute(pool)
    .await;
}

pub async fn dismiss(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: &str,
    decided_by: &str,
) -> Result<(), ServerError> {
    let changed = sqlx::query(
        "UPDATE guest_messages SET status = 'dismissed', decided_at = ?, decided_by = ?
          WHERE id = ? AND thread_id = ? AND status = 'pending'",
    )
    .bind(now_ms())
    .bind(decided_by)
    .bind(message_id)
    .bind(thread_id)
    .execute(pool)
    .await?;
    if changed.rows_affected() == 0 {
        return Err(ServerError(ApiError::conflict(
            "that message is already handled",
        )));
    }
    Ok(())
}
