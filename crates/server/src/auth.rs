//! Sign-in: GitHub OAuth sessions plus a loopback-only development identity.
//!
//! Nothing about identity is trusted from request headers any more. A browser
//! proves who it is with an `HttpOnly` session cookie (`auth_session`), and the
//! only other accepted identity is an explicitly configured development login
//! reached over a direct loopback connection with no proxy markers. Because the
//! server never binds a public interface, an attacker cannot forge either.

use crate::app::AppState;
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::Method;
use sqlx::{FromRow, SqlitePool};
use std::net::SocketAddr;
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{
    now_ms, ApiError, AuthMeDto, AuthStatusDto, AuthStatusResponse, UserAccess, UserDto,
};
use ulid::Ulid;

/// Methods reported by `/api/auth/me`.
pub const METHOD_GITHUB: &str = "github";
pub const METHOD_DEV: &str = "dev";

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

        if let Some(token) = auth_session::read_cookie(&parts.headers, auth_session::COOKIE) {
            if let Some(session) =
                auth_session::lookup(&state.pool, &token, state.config.session_seconds).await?
            {
                if let Some(user) = load_user_by_id(&state.pool, &session.user_id).await? {
                    enforce_access(parts, &user)?;
                    return Ok(CurrentUser {
                        user,
                        method: method_static(&session.method),
                    });
                }
            }
        }

        // Local development escape hatch: an explicitly configured identity is
        // accepted only from a direct loopback connection with no proxy
        // markers, so a tunnelled request can never reach this branch.
        if let Some(dev_login) = state.config.dev_identity.as_deref() {
            if is_loopback_peer(parts) && is_direct_local_request(parts) {
                let user = ensure_identity_user(
                    &state.pool,
                    &Identity::dev(dev_login),
                    &state.config.admins,
                )
                .await?;
                enforce_access(parts, &user)?;
                return Ok(CurrentUser {
                    user,
                    method: METHOD_DEV,
                });
            }
        }

        Err(ServerError(ApiError::unauthorized("sign in to continue")))
    }
}

fn method_static(value: &str) -> &'static str {
    match value {
        METHOD_DEV => METHOD_DEV,
        _ => METHOD_GITHUB,
    }
}

fn is_loopback_peer(parts: &Parts) -> bool {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|info| info.0.ip().is_loopback())
}

/// True when the connection bypassed any local reverse proxy.
fn is_direct_local_request(parts: &Parts) -> bool {
    const PROXY_HEADERS: [&str; 4] = [
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "forwarded",
    ];
    !PROXY_HEADERS
        .iter()
        .any(|name| parts.headers.contains_key(*name))
}

/// Best-effort client address for audit rows. Traffic arrives from a local
/// terminator (Tailscale Funnel, a reverse proxy), which stamps
/// `x-forwarded-for`; only the closest hop is used.
#[must_use]
pub fn client_ip_for_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next().map(str::trim))
        .filter(|hop| !hop.is_empty())
        .unwrap_or("local")
        .to_owned()
}

/// A verified external identity, ready to be mapped onto a local user.
#[derive(Debug, Clone)]
pub struct Identity {
    pub source: &'static str,
    /// Stable primary key, for example `github:octocat`. GitHub logins are
    /// case-insensitive, so the key is lowercased to keep one account per
    /// person no matter how they capitalize their name.
    pub key: String,
    /// Display handle without the provider prefix.
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl Identity {
    #[must_use]
    pub fn github(login: &str, display_name: Option<&str>, avatar_url: Option<&str>) -> Self {
        Self {
            source: "github",
            key: github_key(login),
            login: login.to_owned(),
            display_name: display_name.map(ToOwned::to_owned),
            avatar_url: avatar_url.map(ToOwned::to_owned),
        }
    }

    #[must_use]
    pub fn dev(login: &str) -> Self {
        Self {
            source: "dev",
            key: format!("dev:{login}"),
            login: login.to_owned(),
            display_name: Some("Development".to_owned()),
            avatar_url: None,
        }
    }
}

/// Canonical identity key for a GitHub login.
#[must_use]
pub fn github_key(login: &str) -> String {
    format!("github:{}", login.trim().to_ascii_lowercase())
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
    is_admin: i64,
    access: String,
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
            access: parse_user_access(&self.access),
            hidden_presence: self.hidden_presence != 0,
            created_at: self.created_at,
        }
    }
}

fn parse_user_access(value: &str) -> UserAccess {
    match value {
        "member" => UserAccess::Member,
        "read_only" => UserAccess::ReadOnly,
        "disabled" => UserAccess::Disabled,
        _ => UserAccess::Pending,
    }
}

const USER_COLUMNS: &str =
    "u.id, u.username, u.display_name, u.avatar_url, u.is_admin, u.access, u.hidden_presence, u.created_at";

/// Endpoints a signed-in account can reach before it has been approved, and
/// after its access has been revoked, so the shell can explain the state.
const SELF_SERVICE_PATHS: [&str; 3] = ["/api/auth/me", "/api/auth/status", "/api/auth/logout"];

/// Endpoints a read-only account may still write: personal preferences and
/// signing out. Everything else that mutates state is refused.
const READ_ONLY_WRITE_PATHS: [&str; 2] = ["/api/auth/logout", "/api/settings"];

/// Refuses requests that the account's access level does not permit.
///
/// This is the single chokepoint: every handler that needs an identity takes
/// `CurrentUser`, so a pending, revoked or read-only account cannot reach a
/// mutating route even if a handler forgets its own check.
fn enforce_access(parts: &Parts, user: &UserDto) -> Result<(), ServerError> {
    let path = parts.uri.path();
    if SELF_SERVICE_PATHS.contains(&path) {
        return Ok(());
    }
    match user.access {
        UserAccess::Member | UserAccess::ReadOnly => {}
        UserAccess::Pending => {
            return Err(ServerError(ApiError::forbidden(
                "your account is waiting for an administrator to approve it",
            )))
        }
        UserAccess::Disabled => {
            return Err(ServerError(ApiError::forbidden(
                "your access to this workspace has been revoked",
            )))
        }
    }
    if user.access == UserAccess::ReadOnly
        && !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS)
        && !READ_ONLY_WRITE_PATHS.contains(&path)
    {
        return Err(ServerError(ApiError::forbidden(
            "your access is read-only, ask an administrator to grant write access",
        )));
    }
    Ok(())
}

pub async fn auth_status(
    pool: &SqlitePool,
    github_ready: bool,
    dev_identity: bool,
) -> Result<AuthStatusResponse, ServerError> {
    Ok(AuthStatusResponse {
        status: AuthStatusDto {
            github: github_ready,
            dev_identity,
            needs_admin: needs_admin(pool).await?,
            login_url: github_ready.then(|| "/api/auth/github/start".to_owned()),
        },
    })
}

/// True when no identity-backed administrator exists yet, so the instance has
/// no owner and the next sign-in bootstraps one.
pub async fn needs_admin(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND identity_key IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(admins == 0)
}

pub async fn me(pool: &SqlitePool, user: &CurrentUser) -> Result<AuthMeDto, ServerError> {
    Ok(AuthMeDto {
        user: user.user.clone(),
        auth_method: user.method.to_owned(),
        needs_admin: needs_admin(pool).await?,
    })
}

/// Sign-in policy for a verified GitHub login.
///
/// Allowed when the login is a configured admin, listed in the allowlist,
/// already known to the database, or when the instance has no administrator
/// yet. A public tunnel therefore cannot be used by strangers unless the
/// operator lists them; it can be used by anyone exactly once, to claim an
/// unowned instance.
pub async fn may_sign_in(
    pool: &SqlitePool,
    login: &str,
    admins: &[String],
    allow: &[String],
) -> Result<bool, ServerError> {
    if matches_login(admins, login) || matches_login(allow, login) {
        return Ok(true);
    }
    let known: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM users WHERE identity_key = ? LIMIT 1")
            .bind(github_key(login))
            .fetch_optional(pool)
            .await?;
    if known.is_some() {
        return Ok(true);
    }
    Ok(needs_admin(pool).await?)
}

fn matches_login(list: &[String], login: &str) -> bool {
    list.iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(login.trim()))
}

/// Creates or refreshes the local user for a verified identity.
pub async fn ensure_identity_user(
    pool: &SqlitePool,
    identity: &Identity,
    admins: &[String],
) -> Result<UserDto, ServerError> {
    let existing = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.id = (
             SELECT id FROM users WHERE identity_key = ?
         )"
    ))
    .bind(&identity.key)
    .fetch_optional(pool)
    .await?;

    let user = match existing {
        Some(row) => {
            let display = identity
                .display_name
                .as_deref()
                .filter(|value| !value.trim().is_empty());
            sqlx::query(
                "UPDATE users SET
                     display_name = COALESCE(?, display_name),
                     avatar_url = COALESCE(?, avatar_url)
                 WHERE id = ? AND (
                     (display_name <> COALESCE(?, display_name)) OR
                     (avatar_url IS NOT ?)
                 )",
            )
            .bind(display)
            .bind(identity.avatar_url.as_deref())
            .bind(&row.id)
            .bind(display)
            .bind(identity.avatar_url.as_deref())
            .execute(pool)
            .await?;
            row.into_dto()
        }
        None => {
            // The first identity on a fresh instance becomes the administrator
            // and is active immediately. Development identities are trusted
            // local loops. Every other new account waits for approval.
            let first_admin = needs_admin(pool).await?;
            let access = if first_admin || identity.source == "dev" {
                UserAccess::Member
            } else {
                UserAccess::Pending
            };
            create_identity_user(pool, identity, access).await?
        }
    };

    if user.is_admin || !should_be_admin(pool, &identity.login, admins).await? {
        return Ok(user);
    }

    sqlx::query("UPDATE users SET is_admin = 1, access = 'member' WHERE id = ?")
        .bind(user.id.as_str())
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(user.id.as_str()),
        "user.admin_bootstrapped",
        Some("user"),
        Some(user.id.as_str()),
        Some(identity.source),
        None,
    )
    .await;
    load_user_by_id(pool, user.id.as_str())
        .await?
        .ok_or_else(|| ServerError::internal("promoted user could not be loaded"))
}

async fn should_be_admin(
    pool: &SqlitePool,
    login: &str,
    configured_admins: &[String],
) -> Result<bool, ServerError> {
    if matches_login(configured_admins, login) {
        return Ok(true);
    }
    if !configured_admins.is_empty() {
        return Ok(false);
    }
    // With no configured admins the first identity to sign in becomes the
    // administrator so the instance is never unmanageable.
    Ok(needs_admin(pool).await?)
}

async fn create_identity_user(
    pool: &SqlitePool,
    identity: &Identity,
    access: UserAccess,
) -> Result<UserDto, ServerError> {
    let now = now_ms();
    let base = sanitize_username(&identity.login);
    let display = identity
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| identity.login.clone());
    for attempt in 0..8 {
        let username = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{}", &Ulid::new().to_string()[20..])
        };
        let id = Ulid::new().to_string();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO users(id, username, display_name, avatar_url, is_admin, access, created_at, identity_source, identity_key)
             VALUES(?, ?, ?, ?, 0, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&username)
        .bind(&display)
        .bind(identity.avatar_url.as_deref())
        .bind(access.as_str())
        .bind(now)
        .bind(identity.source)
        .bind(&identity.key)
        .execute(pool)
        .await?;
        if result.rows_affected() == 1 {
            return load_user_by_id(pool, &id)
                .await?
                .ok_or_else(|| ServerError::internal("created user could not be loaded"));
        }
        let raced: Option<UserDto> = load_user_by_identity(pool, &identity.key).await?;
        if let Some(user) = raced {
            return Ok(user);
        }
    }
    Err(ServerError::internal("could not allocate a username"))
}

pub async fn load_user_by_identity(
    pool: &SqlitePool,
    identity_key: &str,
) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.id = (SELECT id FROM users WHERE identity_key = ?)"
    ))
    .bind(identity_key)
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_dto))
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

pub async fn load_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.username = ? COLLATE NOCASE"
    ))
    .bind(username)
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_dto))
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

pub async fn update_user(
    pool: &SqlitePool,
    actor_id: &str,
    target_id: &str,
    request: &UpdateUserRequest,
) -> Result<UserDto, ServerError> {
    let target = load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))?;
    let loses_admin =
        target.is_admin && (!request.is_admin || request.access != UserAccess::Member);
    if loses_admin {
        let others: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND id <> ? AND access = 'member'",
        )
        .bind(target_id)
        .fetch_one(pool)
        .await?;
        if others == 0 {
            return Err(ServerError(ApiError::conflict(
                "at least one active administrator must remain",
            )));
        }
    }
    if actor_id == target_id && request.access != UserAccess::Member {
        return Err(ServerError(ApiError::conflict(
            "you cannot reduce your own access; ask another administrator",
        )));
    }
    sqlx::query("UPDATE users SET is_admin = ?, access = ? WHERE id = ?")
        .bind(i64::from(request.is_admin))
        .bind(request.access.as_str())
        .bind(target_id)
        .execute(pool)
        .await?;
    // Revoking access must end live sessions; otherwise a disabled account
    // keeps reading until the cookie happens to expire.
    if !request.access.can_read() {
        crate::auth_session::revoke_all_for_user(pool, target_id).await?;
    }
    load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_sanitized() {
        assert_eq!(
            sanitize_username("Ada.Lovelace@example.com"),
            "ada.lovelace"
        );
        assert_eq!(sanitize_username("a b c"), "a-b-c");
        assert!(sanitize_username("x").starts_with("user-"));
    }

    #[test]
    fn identities_are_keyed_by_provider_and_login() {
        assert_eq!(
            Identity::github("octocat", None, None).key,
            "github:octocat"
        );
        assert_eq!(Identity::github("octocat", None, None).source, "github");
        assert_eq!(
            Identity::github("OctoCat", None, None).key,
            "github:octocat",
            "logins must collapse to one account regardless of case"
        );
        assert_eq!(Identity::dev("ada").key, "dev:ada");
    }
}
