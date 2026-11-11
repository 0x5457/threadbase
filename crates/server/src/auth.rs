//! Tailscale identity authentication.
//!
//! Every request must carry a Tailscale identity forwarded by
//! `tailscale serve` from a loopback peer. The service never binds a public
//! interface and never stores passwords.

use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use sqlx::{FromRow, SqlitePool};
use std::net::SocketAddr;
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{now_ms, ApiError, AuthMeDto, AuthStatusDto, AuthStatusResponse, UserDto};
use ulid::Ulid;

/// Methods reported by `/api/auth/me`.
pub const METHOD_TAILSCALE: &str = "tailscale";
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

        let loopback = is_loopback_peer(parts);

        if loopback {
            if let Some(identity) = tailscale_identity(parts) {
                let user = ensure_tailscale_user(
                    &state.pool,
                    &identity.login,
                    identity.display_name.as_deref(),
                    identity.avatar_url.as_deref(),
                    &state.config.tailscale_admins,
                )
                .await?;
                return Ok(CurrentUser {
                    user,
                    method: METHOD_TAILSCALE,
                });
            }
        }

        // Local development escape hatch: an explicitly configured identity is
        // accepted only from a direct loopback connection with no proxy
        // markers, so `tailscale serve` traffic can never reach this branch.
        if let Some(dev_login) = state.config.dev_identity.as_deref() {
            if loopback && is_direct_local_request(parts) {
                let user = ensure_tailscale_user(
                    &state.pool,
                    dev_login,
                    Some(dev_login),
                    None,
                    &state.config.tailscale_admins,
                )
                .await?;
                return Ok(CurrentUser {
                    user,
                    method: METHOD_DEV,
                });
            }
        }

        Err(ServerError(ApiError::unauthorized(
            "open this workspace through Tailscale",
        )))
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

/// True when the connection bypassed any local reverse proxy.
fn is_direct_local_request(parts: &Parts) -> bool {
    const PROXY_HEADERS: [&str; 5] = [
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "forwarded",
        "tailscale-headers-info",
    ];
    !has_tailscale_identity(&parts.headers)
        && !PROXY_HEADERS
            .iter()
            .any(|name| parts.headers.contains_key(*name))
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

pub async fn auth_status(
    pool: &SqlitePool,
    tailscale_header: bool,
    dev_identity: bool,
) -> Result<AuthStatusResponse, ServerError> {
    Ok(AuthStatusResponse {
        status: AuthStatusDto {
            tailscale_header,
            dev_identity,
            needs_admin: needs_admin(pool).await?,
        },
    })
}

/// True when no Tailscale identity has been promoted to administrator yet.
pub async fn needs_admin(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND tailscale_login IS NOT NULL",
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

/// Creates or refreshes the user for a Tailscale login and applies the admin
/// bootstrap rule.
pub async fn ensure_tailscale_user(
    pool: &SqlitePool,
    login: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    configured_admins: &[String],
) -> Result<UserDto, ServerError> {
    let existing = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE tailscale_login = ?"
    ))
    .bind(login)
    .fetch_optional(pool)
    .await?;

    let user = match existing {
        Some(row) => {
            if let Some(url) = avatar_url {
                sqlx::query("UPDATE users SET avatar_url = ? WHERE id = ? AND (avatar_url IS NULL OR avatar_url <> ?)")
                    .bind(url)
                    .bind(&row.id)
                    .bind(url)
                    .execute(pool)
                    .await?;
            }
            row.into_dto()
        }
        None => create_tailscale_user(pool, login, display_name, avatar_url).await?,
    };

    if user.is_admin || !should_be_admin(pool, login, configured_admins).await? {
        return Ok(user);
    }

    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?")
        .bind(user.id.as_str())
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(user.id.as_str()),
        "user.admin_bootstrapped",
        Some("user"),
        Some(user.id.as_str()),
        Some("tailscale"),
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
    if configured_admins
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(login))
    {
        return Ok(true);
    }
    if !configured_admins.is_empty() {
        return Ok(false);
    }
    // With no configured admins, the first Tailscale identity to sign in
    // becomes the administrator so the instance is never unmanageable.
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND tailscale_login IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(admins == 0)
}

async fn create_tailscale_user(
    pool: &SqlitePool,
    login: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<UserDto, ServerError> {
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
        if let Some(row) = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {USER_COLUMNS} FROM users u WHERE tailscale_login = ?"
        ))
        .bind(login)
        .fetch_optional(pool)
        .await?
        {
            return Ok(row.into_dto());
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
}
