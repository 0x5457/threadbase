//! Local password auth, opaque database-backed sessions and Tailscale identity.

use crate::error::ServerError;
use crate::state::AppState;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::net::SocketAddr;
use threadbase_shared::api::{LoginRequest, SetupRequest, UpdateUserRequest};
use threadbase_shared::{
    now_ms, ApiError, AuthMeDto, AuthStatusDto, AuthStatusResponse, ErrorCode, UserDto,
};
use ulid::Ulid;

pub const SESSION_COOKIE: &str = "threadbase_session";
const SESSION_DAYS: i64 = 30;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user: UserDto,
    pub method: &'static str,
}

impl CurrentUser {
    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.user.is_admin
    }

    pub fn require_admin(&self) -> Result<(), ServerError> {
        if self.user.is_admin {
            Ok(())
        } else {
            Err(ServerError(ApiError::forbidden("admin access required")))
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ServerError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<CurrentUser>() {
            return Ok(user.clone());
        }
        if let Some(token) = cookie_value(&parts.headers, SESSION_COOKIE) {
            if let Some(user) = session_user(&state.pool, &token).await? {
                return Ok(CurrentUser {
                    user,
                    method: "session",
                });
            }
        }
        if let Some(identity) = tailscale_identity(parts) {
            if is_loopback_peer(parts) {
                let user = provision_tailscale_user(
                    &state.pool,
                    &identity.login,
                    identity.display_name.as_deref(),
                    identity.avatar_url.as_deref(),
                )
                .await?;
                return Ok(CurrentUser {
                    user,
                    method: "tailscale",
                });
            }
        }
        Err(ServerError(ApiError::unauthorized("sign in to continue")))
    }
}

#[derive(Debug, Clone)]
pub struct TailscaleIdentity {
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Tailscale serve forwards identity headers. Only trust them from a
/// loopback peer; the process never binds a public interface.
#[must_use]
pub fn tailscale_identity(parts: &Parts) -> Option<TailscaleIdentity> {
    let login = header_string(&parts.headers, "tailscale-user-login")
        .or_else(|| header_string(&parts.headers, "tailscale-user"))?;
    let login = login.trim().to_owned();
    if login.is_empty() || login.len() > 320 {
        return None;
    }
    let display_name = header_string(&parts.headers, "tailscale-user-name")
        .filter(|value| !value.trim().is_empty());
    let avatar_url = header_string(&parts.headers, "tailscale-user-profile-pic")
        .filter(|value| !value.trim().is_empty());
    Some(TailscaleIdentity {
        login,
        display_name,
        avatar_url,
    })
}

#[must_use]
pub fn has_tailscale_identity(headers: &axum::http::HeaderMap) -> bool {
    headers.contains_key("tailscale-user-login") || headers.contains_key("tailscale-user")
}

fn header_string(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn is_loopback_peer(parts: &Parts) -> bool {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|info| info.0.ip().is_loopback())
}

pub fn client_ip(parts: &Parts) -> String {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
    is_admin: i64,
    tailscale_login: Option<String>,
    hidden_presence: i64,
    created_at: i64,
}

impl UserRow {
    fn into_dto(self) -> UserDto {
        UserDto {
            id: self.id.into(),
            username: self.username,
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            is_admin: self.is_admin != 0,
            tailscale_login: self.tailscale_login,
            hidden_presence: self.hidden_presence != 0,
            created_at: self.created_at,
        }
    }
}

const USER_COLUMNS: &str =
    "u.id, u.username, u.display_name, u.avatar_url, u.is_admin, u.tailscale_login, u.hidden_presence, u.created_at";

pub async fn needs_setup(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE password_hash IS NOT NULL")
            .fetch_one(pool)
            .await?;
    Ok(count == 0)
}

pub async fn auth_status(
    pool: &SqlitePool,
    tailscale_header: bool,
) -> Result<AuthStatusResponse, ServerError> {
    Ok(AuthStatusResponse {
        status: AuthStatusDto {
            needs_setup: needs_setup(pool).await?,
            tailscale_header,
            local_login: true,
        },
    })
}

pub async fn me(pool: &SqlitePool, user: &CurrentUser) -> Result<AuthMeDto, ServerError> {
    Ok(AuthMeDto {
        user: user.user.clone(),
        auth_method: user.method.to_owned(),
        tailscale_available: user.method == "tailscale",
        needs_setup: needs_setup(pool).await?,
    })
}

pub async fn setup_admin(
    pool: &SqlitePool,
    request: &SetupRequest,
) -> Result<UserDto, ServerError> {
    if !needs_setup(pool).await? {
        return Err(ServerError(ApiError::conflict("setup already completed")));
    }
    validate_username(&request.username)?;
    validate_password(&request.password)?;
    let display_name = request.display_name.trim();
    if display_name.is_empty() || display_name.len() > 80 {
        return Err(ServerError(ApiError::validation(
            "display name must be between 1 and 80 characters",
        )));
    }
    let id = Ulid::new().to_string();
    let hash = hash_password(&request.password)?;
    let now = now_ms();
    let inserted = sqlx::query(
        "INSERT INTO users(id, username, display_name, password_hash, is_admin, created_at)
         SELECT ?, ?, ?, ?, 1, ?
         WHERE NOT EXISTS (SELECT 1 FROM users WHERE password_hash IS NOT NULL)",
    )
    .bind(&id)
    .bind(request.username.trim())
    .bind(display_name)
    .bind(hash)
    .bind(now)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ServerError(ApiError::conflict("setup already completed")));
    }
    load_user_by_id(pool, &id)
        .await?
        .ok_or_else(|| ServerError::internal("created admin could not be loaded"))
}

pub async fn login(pool: &SqlitePool, request: &LoginRequest) -> Result<UserDto, ServerError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = ? COLLATE NOCASE")
            .bind(request.username.trim())
            .fetch_optional(pool)
            .await?;
    let Some((id, Some(hash))) = row else {
        return Err(ServerError(ApiError::unauthorized(
            "invalid username or password",
        )));
    };
    if !verify_password(&request.password, &hash) {
        return Err(ServerError(ApiError::unauthorized(
            "invalid username or password",
        )));
    }
    load_user_by_id(pool, &id)
        .await?
        .ok_or_else(|| ServerError::internal("user disappeared during login"))
}

pub async fn issue_session(
    pool: &SqlitePool,
    user_id: &str,
    ip: &str,
    user_agent: Option<&str>,
) -> Result<String, ServerError> {
    let token = format!("{}{}", Ulid::new(), Ulid::new());
    let token_hash = token_hash(&token);
    let now = now_ms();
    let expires = now + SESSION_DAYS * 24 * 60 * 60 * 1000;
    sqlx::query(
        "INSERT INTO auth_sessions(token_hash, user_id, created_at, expires_at, last_seen_at, user_agent, ip)
         VALUES(?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&token_hash)
    .bind(user_id)
    .bind(now)
    .bind(expires)
    .bind(now)
    .bind(user_agent)
    .bind(ip)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn revoke_session(pool: &SqlitePool, token: &str) -> Result<(), ServerError> {
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(now_ms())
    .bind(token_hash(token))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_all_sessions(pool: &SqlitePool, user_id: &str) -> Result<(), ServerError> {
    sqlx::query("UPDATE auth_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL")
        .bind(now_ms())
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn session_user(pool: &SqlitePool, token: &str) -> Result<Option<UserDto>, ServerError> {
    let hash = token_hash(token);
    let row: Option<UserRow> = sqlx::query_as(&format!(
        "SELECT {USER_COLUMNS} FROM users u
         JOIN auth_sessions s ON s.user_id = u.id
         WHERE s.token_hash = ? AND s.revoked_at IS NULL AND s.expires_at > ?"
    ))
    .bind(hash)
    .bind(now_ms())
    .fetch_optional(pool)
    .await?;
    if row.is_some() {
        sqlx::query("UPDATE auth_sessions SET last_seen_at = ? WHERE token_hash = ?")
            .bind(now_ms())
            .bind(token_hash(token))
            .execute(pool)
            .await?;
    }
    Ok(row.map(UserRow::into_dto))
}

async fn provision_tailscale_user(
    pool: &SqlitePool,
    login: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<UserDto, ServerError> {
    if let Some(user) = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE tailscale_login = ?"
    ))
    .bind(login)
    .fetch_optional(pool)
    .await?
    {
        return Ok(user.into_dto());
    }
    let now = now_ms();
    let base = sanitize_username(login);
    let display = display_name
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| login.to_owned());
    for attempt in 0..8 {
        let username = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{}", &Ulid::new().to_string()[20..])
        };
        let id = Ulid::new().to_string();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO users(id, username, display_name, avatar_url, tailscale_login, is_admin, created_at)
             VALUES(?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(&id)
        .bind(&username)
        .bind(&display)
        .bind(avatar_url)
        .bind(login)
        .bind(now)
        .execute(pool)
        .await?;
        if result.rows_affected() == 1 {
            return load_user_by_id(pool, &id).await?.ok_or_else(|| {
                ServerError::internal("created tailscale user could not be loaded")
            });
        }
        if let Some(user) = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {USER_COLUMNS} FROM users u WHERE tailscale_login = ?"
        ))
        .bind(login)
        .fetch_optional(pool)
        .await?
        {
            return Ok(user.into_dto());
        }
    }
    Err(ServerError::internal("could not allocate a username"))
}

pub async fn load_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users u WHERE id = ?"))
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(UserRow::into_dto),
    )
}

pub async fn list_users(pool: &SqlitePool) -> Result<Vec<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u ORDER BY u.created_at ASC"
    ))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(UserRow::into_dto)
    .collect())
}

pub async fn update_user_admin(
    pool: &SqlitePool,
    target_id: &str,
    request: &UpdateUserRequest,
) -> Result<UserDto, ServerError> {
    let result = sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?")
        .bind(i64::from(request.is_admin))
        .bind(target_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("user not found")));
    }
    load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))
}

#[must_use]
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_DAYS * 24 * 60 * 60,
        if secure { "; Secure" } else { "" }
    )
}

#[must_use]
pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

#[must_use]
pub fn cookies_secure(headers: &axum::http::HeaderMap, public_origin: Option<&str>) -> bool {
    if public_origin.is_some_and(|origin| origin.starts_with("https://")) {
        return true;
    }
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
}

#[must_use]
pub fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == name {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

pub async fn audit(
    pool: &SqlitePool,
    actor_id: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    detail: Option<&str>,
    ip: Option<&str>,
) {
    let result = sqlx::query(
        "INSERT INTO audit_log(id, actor_id, action, target_type, target_id, detail_json, ip, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .bind(ip)
    .bind(now_ms())
    .execute(pool)
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, action, "failed to write audit entry");
    }
}

fn hash_password(password: &str) -> Result<String, ServerError> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|error| ServerError::internal(format!("password hashing failed: {error}")))?;
    Ok(hash.to_string())
}

fn verify_password(password: &str, encoded: &str) -> bool {
    let Ok(hash) = PasswordHash::new(encoded) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

fn validate_username(username: &str) -> Result<(), ServerError> {
    let username = username.trim();
    if username.len() < 3 || username.len() > 32 {
        return Err(ServerError(ApiError::validation(
            "username must be between 3 and 32 characters",
        )));
    }
    if !username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(ServerError(ApiError::validation(
            "username may contain letters, digits, dot, dash and underscore",
        )));
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), ServerError> {
    if password.len() < 10 {
        return Err(ServerError(ApiError::validation(
            "password must be at least 10 characters",
        )));
    }
    if password.len() > 256 {
        return Err(ServerError(ApiError::validation("password is too long")));
    }
    Ok(())
}

fn sanitize_username(login: &str) -> String {
    let base = login.split('@').next().unwrap_or(login);
    let mut output: String = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    output = output.trim_matches('-').to_owned();
    if output.len() < 3 {
        output = format!("user-{output}");
    }
    output.truncate(24);
    output
}

fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

#[must_use]
pub fn error_code_for_missing_setup() -> ErrorCode {
    ErrorCode::SetupRequired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_passwords() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn cookie_parser_ignores_other_values() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("other=1; {SESSION_COOKIE}=abc; third=2")
                .parse()
                .unwrap(),
        );
        assert_eq!(
            cookie_value(&headers, SESSION_COOKIE).as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn usernames_are_sanitized() {
        assert_eq!(
            sanitize_username("Ada.Lovelace@example.com"),
            "ada.lovelace"
        );
        assert_eq!(sanitize_username("a b c"), "a-b-c");
        assert!(sanitize_username("x").starts_with("user-"));
    }
}
