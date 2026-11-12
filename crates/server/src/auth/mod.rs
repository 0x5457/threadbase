//! Sign-in: GitHub OAuth sessions plus a loopback-only development identity.
//!
//! Nothing about identity is trusted from request headers any more. A browser
//! proves who it is with an `HttpOnly` session cookie (`auth_session`), and the
//! only other accepted identity is an explicitly configured development login
//! reached over a direct loopback connection with no proxy markers. Because the
//! server never binds a public interface, an attacker cannot forge either.
//!
//! `identity` owns the `users` table and administrator rules, `github_routes`
//! owns the OAuth browser flow, and this file owns the request-facing
//! extractor, access enforcement and audit.

mod github_routes;
mod identity;

use crate::app::AppState;
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::Method;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use threadbase_shared::{
    now_ms, ApiError, AuthMeDto, AuthStatusDto, AuthStatusResponse, UserAccess, UserDto,
};

pub(crate) use github_routes::router as github_router;
pub use identity::{
    ensure_identity_user, list_users, load_user_by_id, load_user_by_identity,
    load_user_by_username, needs_admin, update_user,
};

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
            let now = now_ms();
            let token_hash = auth_session::hash(&token);
            if let Some(session) =
                identity::load_session_user(&state.pool, &token_hash, now).await?
            {
                auth_session::slide(
                    &state.pool,
                    &token_hash,
                    session.expires_at,
                    session.last_seen_at,
                    state.config.session_seconds,
                    now,
                )
                .await;
                enforce_access(parts, &session.user)?;
                return Ok(CurrentUser {
                    user: session.user,
                    method: method_static(&session.method),
                });
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
    .bind(ulid::Ulid::new().to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

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
