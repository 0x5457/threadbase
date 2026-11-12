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
    ensure_identity_user, ensure_pending_identity, list_users, load_user_by_id,
    load_user_by_identity, load_user_by_username, needs_admin, update_user,
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

/// Best-effort client address for audit rows and rate limiting.
///
/// `x-forwarded-for` is only honoured when the direct peer is loopback, which
/// is where a local terminator (Tailscale Funnel, a reverse proxy) sits. A
/// remote peer cannot spoof its address through the header; its socket address
/// is used instead. Only the closest forwarded hop is used.
#[must_use]
pub fn client_ip_for_headers(headers: &axum::http::HeaderMap, peer: Option<SocketAddr>) -> String {
    let peer_ip = peer.map(|addr| addr.ip());
    if peer_ip.is_some_and(|ip| ip.is_loopback()) {
        if let Some(hop) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|hop| !hop.is_empty())
        {
            return hop.to_owned();
        }
    }
    peer_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "local".to_owned())
}

/// A verified external identity, ready to be mapped onto a local user.
#[derive(Debug, Clone)]
pub struct Identity {
    pub source: &'static str,
    /// Stable primary key, for example `github:12345`. GitHub's numeric account
    /// id never changes; the login is only a mutable label that can be renamed
    /// or re-registered by someone else.
    pub key: String,
    /// Display handle without the provider prefix.
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl Identity {
    #[must_use]
    pub fn github(
        id: i64,
        login: &str,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Self {
        Self {
            source: "github",
            key: github_key(id),
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

/// Canonical identity key for a GitHub account id.
#[must_use]
pub fn github_key(id: i64) -> String {
    format!("github:{id}")
}

/// Pre-migration identity key, derived from the mutable GitHub login. Used to
/// find and upgrade rows created before numeric ids were stored, and for
/// provisional accounts created when an invite only knows the login.
#[must_use]
pub(crate) fn github_login_key(login: &str) -> String {
    format!("github:{}", login.trim().to_ascii_lowercase())
}

/// Provisional identity for an invited login that has not signed in yet.
///
/// The numeric id is unknown until the first sign-in, so the row is keyed by
/// the login; `ensure_identity_user` upgrades it in place when that login
/// presents its numeric id.
#[must_use]
pub fn github_invited(login: &str, display_name: Option<&str>) -> Identity {
    Identity {
        source: "github",
        key: github_login_key(login),
        login: login.trim().to_owned(),
        display_name: display_name.map(ToOwned::to_owned),
        avatar_url: None,
    }
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

/// Sign-in policy for a verified GitHub identity.
///
/// Allowed when a configured admin or allowlist entry names the login
/// (case-insensitive, trimmed), the canonical `github:<id>` key, or the bare
/// numeric id; when an account already exists for `github:<id>` or for the
/// pre-migration `github:<login>` key; or when the instance has no administrator
/// yet. The first unowned instance can therefore be claimed once, by anyone.
pub async fn may_sign_in(
    pool: &SqlitePool,
    id: i64,
    login: &str,
    admins: &[String],
    allow: &[String],
) -> Result<bool, ServerError> {
    if matches_identity(admins, id, login) || matches_identity(allow, id, login) {
        return Ok(true);
    }
    let known: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM users WHERE identity_key IN (?, ?) LIMIT 1")
            .bind(github_key(id))
            .bind(github_login_key(login))
            .fetch_optional(pool)
            .await?;
    if known.is_some() {
        return Ok(true);
    }
    Ok(needs_admin(pool).await?)
}

pub(crate) fn matches_identity(list: &[String], id: i64, login: &str) -> bool {
    let key = github_key(id);
    let bare_id = id.to_string();
    list.iter().any(|candidate| {
        let candidate = candidate.trim();
        candidate.eq_ignore_ascii_case(login.trim())
            || candidate.eq_ignore_ascii_case(&key)
            || candidate == bare_id
    })
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
    fn identity_keys_use_the_immutable_github_id() {
        let identity = Identity::github(12345, "octocat", None, None);
        assert_eq!(identity.key, "github:12345");
        assert_eq!(identity.login, "octocat");
        assert_eq!(identity.source, "github");
        // A rename keeps the same account key.
        assert_eq!(
            Identity::github(12345, "renamed", None, None).key,
            "github:12345"
        );
        // A different account that re-registers the freed login gets a
        // different key, so it cannot reach the original account.
        assert_ne!(
            Identity::github(67890, "octocat", None, None).key,
            identity.key
        );
        assert_eq!(Identity::dev("ada").key, "dev:ada");
    }

    #[test]
    fn forwarded_for_is_only_trusted_from_loopback_peers() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.2".parse().unwrap());
        let loopback: SocketAddr = "127.0.0.1:5555".parse().unwrap();
        assert_eq!(
            client_ip_for_headers(&headers, Some(loopback)),
            "203.0.113.9"
        );
        let public: SocketAddr = "198.51.100.7:5555".parse().unwrap();
        assert_eq!(
            client_ip_for_headers(&headers, Some(public)),
            "198.51.100.7"
        );
        assert_eq!(client_ip_for_headers(&headers, None), "local");
    }

    #[tokio::test]
    async fn sign_in_policy_accepts_ids_and_legacy_keys() {
        let pool = crate::db::connect_memory().await.unwrap();
        let owner = ensure_identity_user(&pool, &Identity::github(1, "owner", None, None), &[])
            .await
            .unwrap();
        assert!(owner.is_admin);

        assert!(!may_sign_in(&pool, 2, "stranger", &[], &[]).await.unwrap());
        assert!(
            may_sign_in(&pool, 2, "stranger", &["github:2".to_owned()], &[])
                .await
                .unwrap()
        );
        assert!(may_sign_in(&pool, 3, "stranger", &[], &["3".to_owned()])
            .await
            .unwrap());
        assert!(
            may_sign_in(&pool, 4, "Invited", &[], &["invited".to_owned()])
                .await
                .unwrap()
        );

        let known = ensure_identity_user(&pool, &Identity::github(5, "known", None, None), &[])
            .await
            .unwrap();
        assert_eq!(known.access, UserAccess::Pending);
        // The account is found by immutable id even after a login rename.
        assert!(may_sign_in(&pool, 5, "renamed-login", &[], &[])
            .await
            .unwrap());

        // A pre-migration row is still recognised by its login key.
        sqlx::query("UPDATE users SET identity_key = 'github:known' WHERE id = ?")
            .bind(known.id.as_str())
            .execute(&pool)
            .await
            .unwrap();
        assert!(may_sign_in(&pool, 5, "known", &[], &[]).await.unwrap());
    }

    #[tokio::test]
    async fn configured_admins_can_be_named_by_github_id() {
        let pool = crate::db::connect_memory().await.unwrap();
        let _owner = ensure_identity_user(&pool, &Identity::github(1, "owner", None, None), &[])
            .await
            .unwrap();

        let by_key = ensure_identity_user(
            &pool,
            &Identity::github(9, "renamed-admin", None, None),
            &["github:9".to_owned()],
        )
        .await
        .unwrap();
        assert!(by_key.is_admin);
        assert_eq!(by_key.access, UserAccess::Member);

        let by_bare_id = ensure_identity_user(
            &pool,
            &Identity::github(10, "another-admin", None, None),
            &["10".to_owned()],
        )
        .await
        .unwrap();
        assert!(by_bare_id.is_admin);
    }
}
