//! Server-side sessions carried by an `HttpOnly` cookie.
//!
//! The `auth_sessions` table already existed with a hashed token column, so
//! nothing about the schema is invented here: only tokens and their hashes are.
//! A browser cannot read the cookie value, and a database leak exposes hashes
//! instead of usable tokens.

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use threadbase_shared::now_ms;
use ulid::Ulid;

/// Session cookie name.
pub const COOKIE: &str = "threadbase_session";
/// Short-lived cookie carrying OAuth `state` and the PKCE verifier.
pub const OAUTH_COOKIE: &str = "threadbase_oauth";
/// Lifetime of the OAuth handshake cookie.
pub const OAUTH_COOKIE_SECONDS: i64 = 10 * 60;
/// Refresh a session at most this often, so reads stay cheap.
const TOUCH_INTERVAL_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: String,
    pub method: String,
}

/// 160 random bits from the ULID generator, hex-encoded by `Display`.
fn new_token() -> String {
    format!("{}{}", Ulid::new(), Ulid::new())
}

#[must_use]
pub fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Creates a session and returns the raw token for the cookie. Only the hash is
/// stored, so the token cannot be recovered from the database.
pub async fn create(
    pool: &SqlitePool,
    user_id: &str,
    method: &str,
    ip: Option<&str>,
    user_agent: Option<&str>,
    ttl_seconds: i64,
) -> Result<String, sqlx::Error> {
    let now = now_ms();
    let token = new_token();
    let _ = sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await;
    sqlx::query(
        "INSERT INTO auth_sessions(token_hash, user_id, created_at, expires_at, last_seen_at, user_agent, ip, method)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(hash(&token))
    .bind(user_id)
    .bind(now)
    .bind(now + ttl_seconds * 1000)
    .bind(now)
    .bind(user_agent)
    .bind(ip)
    .bind(method)
    .execute(pool)
    .await?;
    Ok(token)
}

/// Resolves a cookie value to a live session, sliding the expiry forward when
/// the session has not been touched for an hour.
pub async fn lookup(
    pool: &SqlitePool,
    token: &str,
    ttl_seconds: i64,
) -> Result<Option<Session>, sqlx::Error> {
    let now = now_ms();
    let token_hash = hash(token);
    let row: Option<(String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT user_id, method, expires_at, last_seen_at FROM auth_sessions
         WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(&token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let Some((user_id, method, expires_at, last_seen_at)) = row else {
        return Ok(None);
    };
    slide(
        pool,
        &token_hash,
        expires_at,
        last_seen_at,
        ttl_seconds,
        now,
    )
    .await;
    Ok(Some(Session { user_id, method }))
}

/// Slides a live session's expiry forward after a successful lookup. Shared
/// with the joined session+user lookup used by the `CurrentUser` extractor.
pub(crate) async fn slide(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: i64,
    last_seen_at: Option<i64>,
    ttl_seconds: i64,
    now: i64,
) {
    if last_seen_at.is_none_or(|seen| now - seen > TOUCH_INTERVAL_MS) {
        let _ = sqlx::query(
            "UPDATE auth_sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?",
        )
        .bind(now)
        .bind(now + ttl_seconds * 1000)
        .bind(token_hash)
        .execute(pool)
        .await;
    } else if expires_at - now < ttl_seconds * 500 {
        let _ = sqlx::query("UPDATE auth_sessions SET expires_at = ? WHERE token_hash = ?")
            .bind(now + ttl_seconds * 1000)
            .bind(token_hash)
            .execute(pool)
            .await;
    }
}

pub async fn revoke(pool: &SqlitePool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(now_ms())
    .bind(hash(token))
    .execute(pool)
    .await?;
    Ok(())
}

/// Ends every live session for a user. Used when access is revoked so the
/// browser cannot keep reading with a cookie issued before the change.
pub async fn revoke_all_for_user(pool: &SqlitePool, user_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL")
        .bind(now_ms())
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Reads a cookie from the request headers.
#[must_use]
pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all("cookie").iter().find_map(|header| {
        header.to_str().ok().and_then(|value| {
            value.split(';').find_map(|pair| {
                let (key, raw) = pair.split_once('=')?;
                if key.trim() != name {
                    return None;
                }
                let raw = raw.trim().trim_matches('"');
                (!raw.is_empty()).then(|| raw.to_owned())
            })
        })
    })
}

/// `Set-Cookie` value for a session or handshake cookie.
///
/// `SameSite=Lax` keeps the value available on top-level OAuth redirects back
/// from GitHub while still blocking cross-site form posts; every mutating
/// request is additionally checked by the CSRF middleware.
#[must_use]
pub fn cookie_header(name: &str, value: &str, secure: bool, max_age_seconds: i64) -> String {
    cookie_header_at(name, value, secure, max_age_seconds, "/")
}

/// Same, with an explicit path so the OAuth handshake cookie can be scoped to
/// the callback routes only.
#[must_use]
pub fn cookie_header_at(
    name: &str,
    value: &str,
    secure: bool,
    max_age_seconds: i64,
    path: &str,
) -> String {
    let mut cookie =
        format!("{name}={value}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

#[must_use]
pub fn clear_cookie(name: &str, secure: bool) -> String {
    clear_cookie_at(name, secure, "/")
}

#[must_use]
pub fn clear_cookie_at(name: &str, secure: bool, path: &str) -> String {
    cookie_header_at(name, "", secure, 0, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 52);
        assert_eq!(hash(&a).len(), 64);
        assert_ne!(hash(&a), hash(&b));
    }

    #[test]
    fn cookies_parse_from_multiple_headers() {
        let mut headers = HeaderMap::new();
        headers.append("cookie", "other=1".parse().unwrap());
        headers.append(
            "cookie",
            format!("{COOKIE}=abc123; another=2").parse().unwrap(),
        );
        assert_eq!(read_cookie(&headers, COOKIE).as_deref(), Some("abc123"));
        assert_eq!(read_cookie(&headers, "missing"), None);
    }

    #[test]
    fn cookie_attributes_follow_the_secure_flag() {
        let insecure = cookie_header(COOKIE, "t", false, 60);
        assert!(insecure.contains("HttpOnly"));
        assert!(insecure.contains("SameSite=Lax"));
        assert!(!insecure.contains("Secure"));
        let secure = cookie_header(COOKIE, "t", true, 60);
        assert!(secure.contains("; Secure"));
        assert!(clear_cookie(COOKIE, true).contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn sessions_round_trip_and_revoke() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES('u1','ada','Ada',1,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let token = create(&pool, "u1", "github", Some("127.0.0.1"), None, 3600)
            .await
            .unwrap();
        let session = lookup(&pool, &token, 3600).await.unwrap().unwrap();
        assert_eq!(session.user_id, "u1");
        assert_eq!(session.method, "github");

        // The raw token is never stored.
        let stored: Option<String> =
            sqlx::query_scalar("SELECT token_hash FROM auth_sessions LIMIT 1")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some(hash(&token).as_str()));

        revoke(&pool, &token).await.unwrap();
        assert!(lookup(&pool, &token, 3600).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn lookup_slides_expiry_and_last_seen() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES('u1','ada','Ada',1,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let token = create(&pool, "u1", "github", None, None, 3600)
            .await
            .unwrap();
        let token_hash = hash(&token);
        let old_seen = now_ms() - 2 * TOUCH_INTERVAL_MS;
        sqlx::query(
            "UPDATE auth_sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?",
        )
        .bind(old_seen)
        .bind(now_ms() + 1000)
        .bind(&token_hash)
        .execute(&pool)
        .await
        .unwrap();

        let session = lookup(&pool, &token, 3600).await.unwrap().unwrap();
        assert_eq!(session.user_id, "u1");
        let (last_seen, expires): (Option<i64>, i64) = sqlx::query_as(
            "SELECT last_seen_at, expires_at FROM auth_sessions WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(last_seen.unwrap() > old_seen);
        assert!(expires > now_ms() + 3600 * 1000 - 5_000);
    }

    #[tokio::test]
    async fn expired_sessions_are_rejected() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES('u1','ada','Ada',1,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let token = create(&pool, "u1", "github", None, None, -1).await.unwrap();
        assert!(lookup(&pool, &token, 3600).await.unwrap().is_none());
    }
}
