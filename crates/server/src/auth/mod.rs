//! Sign-in: Tailscale identity, plus a loopback-only development identity.
//!
//! There is no password and no OAuth app. The browser sends an `HttpOnly`
//! session cookie (`auth_session`), and that cookie can only be created by a
//! request that `tailscaled` proxied from the tailnet: the server listens on
//! loopback and `tailscale serve` is the only way in. Signing in is one step:
//! the tailnet login becomes a local account, and that login is the name
//! everyone sees.
//!
//! `identity` owns the `users` table and administrator rules, `tailscale` owns
//! the CLI calls that answer "who is this", and this file owns the extractor,
//! access checks and audit.

mod identity;
mod tailscale;

use crate::app::AppState;
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::Method;
use sqlx::SqlitePool;
use std::net::{IpAddr, SocketAddr};
use threadbase_shared::{
    now_ms, ApiError, AuthMeDto, AuthStatusDto, AuthStatusResponse, UserAccess, UserDto,
};

pub use identity::{
    ensure_identity_user, ensure_pending_identity, list_users, load_user_briefs, load_user_by_id,
    load_user_by_identity, load_user_by_username, needs_admin, update_user,
};
pub use tailscale::{present as tailnet_present, whois_agrees as tailnet_whois_agrees};

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
        _ => METHOD_TAILSCALE,
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
/// `axum-client-ip` only offers extractors that trust a header unconditionally
/// or use the socket address, so it cannot express the conditional rule below;
/// the check therefore stays explicit here.
///
/// `x-forwarded-for` is only honoured when the direct peer is loopback, which
/// is where a local terminator (Tailscale Funnel, a reverse proxy) sits. A
/// remote peer cannot spoof its address through the header; its socket address
/// is used instead. The rightmost hop is the address appended by the trusted
/// local proxy; an unparseable value falls back to the peer address.
#[must_use]
pub fn client_ip_for_headers(headers: &axum::http::HeaderMap, peer: Option<SocketAddr>) -> String {
    let peer_ip = peer.map(|addr| addr.ip());
    if peer_ip.is_some_and(|ip| ip.is_loopback()) {
        if let Some(hop) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit(',').next())
            .map(str::trim)
            .and_then(|hop| hop.parse::<IpAddr>().ok())
        {
            return hop.to_string();
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
    /// Stable primary key, for example `tailscale:ada@example.com`.
    pub key: String,
    /// Display handle without the provider prefix.
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl Identity {
    /// A tailnet user. The login is Tailscale's `LoginName`: an email, or a handle
    /// like `someone@github`. It stays the same while the account exists, so it
    /// is used as the key and as the display name.
    #[must_use]
    pub fn tailscale(login: &str, display_name: Option<&str>, avatar_url: Option<&str>) -> Self {
        let login = login.trim();
        Self {
            source: "tailscale",
            key: tailscale_key(login),
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

/// Canonical identity key for a tailnet login.
#[must_use]
pub fn tailscale_key(login: &str) -> String {
    format!("tailscale:{}", login.trim().to_ascii_lowercase())
}

/// Provisional identity for a tailnet login that has not signed in yet, used when
/// someone is added to a thread by name.
#[must_use]
pub fn tailscale_invited(login: &str, display_name: Option<&str>) -> Identity {
    Identity::tailscale(login, display_name, None)
}

fn matches_login(list: &[String], login: &str) -> bool {
    list.iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(login.trim()))
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

/// What the shell needs before it can sign anyone in.
///
/// `tailscale` tells the client this request arrived with a tailnet identity, so
/// it should exchange it for a session instead of showing anything. There is no
/// login URL: nothing to redirect to, nothing to type.
pub async fn auth_status(
    pool: &SqlitePool,
    tailscale_identity: Option<&str>,
    dev_identity: bool,
) -> Result<AuthStatusResponse, ServerError> {
    Ok(AuthStatusResponse {
        status: AuthStatusDto {
            tailscale: tailscale_identity.is_some(),
            identity: tailscale_identity.map(ToOwned::to_owned),
            dev_identity,
            needs_admin: needs_admin(pool).await?,
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
    fn a_tailnet_login_is_the_account_key() {
        let identity = Identity::tailscale("ada@example.com", Some("Ada"), None);
        assert_eq!(identity.key, "tailscale:ada@example.com");
        assert_eq!(identity.login, "ada@example.com");
        assert_eq!(identity.source, "tailscale");
        // Logins are case-insensitive in the tailnet; one account, not two.
        assert_eq!(
            Identity::tailscale("Ada@Example.com", None, None).key,
            identity.key
        );
        assert_eq!(Identity::dev("ada").key, "dev:ada");
    }

    #[test]
    fn forwarded_for_is_only_trusted_from_loopback_peers() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.2".parse().unwrap());
        let loopback: SocketAddr = "127.0.0.1:5555".parse().unwrap();
        assert_eq!(client_ip_for_headers(&headers, Some(loopback)), "10.0.0.2");
        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
        assert_eq!(client_ip_for_headers(&headers, Some(loopback)), "127.0.0.1");
        let public: SocketAddr = "198.51.100.7:5555".parse().unwrap();
        assert_eq!(
            client_ip_for_headers(&headers, Some(public)),
            "198.51.100.7"
        );
        assert_eq!(client_ip_for_headers(&headers, None), "local");
    }

    #[tokio::test]
    async fn a_configured_admin_is_named_by_their_tailnet_login() {
        let pool = crate::db::connect_memory().await.unwrap();
        let _owner = ensure_identity_user(
            &pool,
            &Identity::tailscale("owner@example.com", None, None),
            &[],
        )
        .await
        .unwrap();

        let admin = ensure_identity_user(
            &pool,
            &Identity::tailscale("ADA@example.com", None, None),
            &["ada@example.com".to_owned()],
        )
        .await
        .unwrap();
        assert!(admin.is_admin);
        assert_eq!(admin.access, UserAccess::Member);
    }
}
