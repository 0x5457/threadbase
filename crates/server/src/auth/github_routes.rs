//! Browser-facing GitHub OAuth routes: start the redirect and handle the
//! callback. The protocol details live in `auth_github`; this module only
//! wires them to HTTP, cookies and the user table.

use crate::app::AppState;
use crate::auth::{self, client_ip_for_headers, Identity, METHOD_GITHUB};
use crate::auth_github;
use crate::auth_session;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use serde::Deserialize;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use url::form_urlencoded;

/// Handshake cookie is only presented to the callback.
const HANDSHAKE_PATH: &str = "/api/auth/github";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/github/start", get(github_start))
        .route("/api/auth/github/callback", get(github_callback))
}

async fn github_start(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !RateLimit::allow_start(&headers, Some(peer)) {
        return redirect_error("too many sign-in attempts, wait a minute");
    }
    let Some(oauth) = state.config.github.as_ref() else {
        return redirect_error("github sign-in is not configured");
    };
    let Some(callback) = state.config.callback_url() else {
        return redirect_error("THREADBASE_PUBLIC_ORIGIN is not set");
    };
    let state_value = auth_github::new_state();
    let verifier = auth_github::new_verifier();
    let url = match auth_github::authorize_url(oauth, &callback, &state_value, &verifier) {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(%error, "github authorization URL could not be built");
            return redirect_error("github sign-in is not configured");
        }
    };

    let mut response_headers = HeaderMap::new();
    response_headers.append(
        header::SET_COOKIE,
        cookie(auth_session::cookie_header_at(
            auth_session::OAUTH_COOKIE,
            &format!("{state_value}:{verifier}"),
            state.config.cookie_secure,
            auth_session::OAUTH_COOKIE_SECONDS,
            HANDSHAKE_PATH,
        )),
    );
    response_headers.append(
        header::LOCATION,
        HeaderValue::from_str(&url).unwrap_or(HeaderValue::from_static("/")),
    );
    (StatusCode::SEE_OTHER, response_headers).into_response()
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn github_callback(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    if !RateLimit::allow_callback(&headers, Some(peer)) {
        return redirect_error("too many sign-in attempts, wait a minute");
    }
    let mut cookies = HeaderMap::new();
    cookies.append(
        header::SET_COOKIE,
        cookie(auth_session::clear_cookie_at(
            auth_session::OAUTH_COOKIE,
            state.config.cookie_secure,
            HANDSHAKE_PATH,
        )),
    );

    if let Some(error) = query.error.as_deref() {
        tracing::warn!(error, "github authorization was not granted");
        return with_cookies(
            redirect_error(
                query
                    .error_description
                    .as_deref()
                    .unwrap_or("github authorization was not granted"),
            ),
            cookies,
        );
    }
    let (Some(code), Some(returned_state)) = (query.code.as_deref(), query.state.as_deref()) else {
        return with_cookies(redirect_error("github callback was incomplete"), cookies);
    };
    let handshake = auth_session::read_cookie(&headers, auth_session::OAUTH_COOKIE);
    let Some((expected_state, verifier)) =
        handshake.as_deref().and_then(|value| value.split_once(':'))
    else {
        return with_cookies(
            redirect_error("sign-in session expired, try again"),
            cookies,
        );
    };
    if expected_state != returned_state {
        return with_cookies(redirect_error("sign-in state did not match"), cookies);
    }
    let (Some(oauth), Some(callback)) = (state.config.github.as_ref(), state.config.callback_url())
    else {
        return with_cookies(redirect_error("github sign-in is not configured"), cookies);
    };

    let token = match auth_github::exchange(oauth, &callback, code, verifier).await {
        Ok(token) => token,
        Err(error) => {
            tracing::warn!(%error, "github code exchange failed");
            return with_cookies(redirect_error("github sign-in failed"), cookies);
        }
    };
    let profile = match auth_github::profile(&token).await {
        Ok(profile) => profile,
        Err(error) => {
            tracing::warn!(%error, "github profile request failed");
            return with_cookies(redirect_error("github sign-in failed"), cookies);
        }
    };

    let allowed = match auth::may_sign_in(
        &state.pool,
        profile.id,
        &profile.login,
        &state.config.admins,
        &state.config.github_allow,
    )
    .await
    {
        Ok(allowed) => allowed,
        Err(error) => {
            tracing::error!(%error, "sign-in policy check failed");
            return with_cookies(redirect_error("sign-in failed"), cookies);
        }
    };
    if !allowed {
        tracing::warn!(login = %profile.login, "github login is not allowed on this instance");
        return with_cookies(
            redirect_error("this github account is not invited"),
            cookies,
        );
    }

    let identity = Identity::github(
        profile.id,
        &profile.login,
        profile.display_name.as_deref(),
        profile.avatar_url.as_deref(),
    );
    let user = match auth::ensure_identity_user(&state.pool, &identity, &state.config.admins).await
    {
        Ok(user) => user,
        Err(error) => {
            tracing::error!(%error, "could not map github identity onto a user");
            return with_cookies(redirect_error("sign-in failed"), cookies);
        }
    };

    let ip = client_ip_for_headers(&headers, Some(peer));
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let token = match auth_session::create(
        &state.pool,
        user.id.as_str(),
        METHOD_GITHUB,
        Some(&ip),
        user_agent,
        state.config.session_seconds,
    )
    .await
    {
        Ok(token) => token,
        Err(error) => {
            tracing::error!(%error, "could not create a session");
            return with_cookies(redirect_error("sign-in failed"), cookies);
        }
    };
    auth::audit(
        &state.pool,
        Some(user.id.as_str()),
        "user.signed_in",
        Some("user"),
        Some(user.id.as_str()),
        Some(METHOD_GITHUB),
        Some(&ip),
    )
    .await;

    cookies.append(
        header::SET_COOKIE,
        cookie(auth_session::cookie_header(
            auth_session::COOKIE,
            &token,
            state.config.cookie_secure,
            state.config.session_seconds,
        )),
    );
    let mut response_headers = HeaderMap::new();
    response_headers.append(header::LOCATION, HeaderValue::from_static("/"));
    with_cookies(
        (StatusCode::SEE_OTHER, response_headers).into_response(),
        cookies,
    )
}

fn cookie(value: String) -> HeaderValue {
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn with_cookies(mut response: Response, cookies: HeaderMap) -> Response {
    response.headers_mut().extend(cookies);
    response
}

/// Sends the browser back to the shell with a human-readable reason, so the
/// sign-in screen can explain what happened instead of showing raw JSON.
fn redirect_error(message: &str) -> Response {
    let encoded: String = form_urlencoded::byte_serialize(message.as_bytes()).collect();
    let mut headers = HeaderMap::new();
    headers.append(
        header::LOCATION,
        HeaderValue::from_str(&format!("/?auth_error={encoded}"))
            .unwrap_or(HeaderValue::from_static("/?auth_error=sign-in+failed")),
    );
    (StatusCode::SEE_OTHER, headers).into_response()
}

/// Coarse budget for the unauthenticated auth endpoints: 30 requests per
/// client IP per bucket within a 60-second window.
const RATE_LIMIT: u32 = 30;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// The two unauthenticated endpoints keep separate quotas, so hammering one
/// cannot exhaust the other's budget for the same client.
#[derive(Clone, Copy, Debug)]
enum AuthEndpoint {
    Start,
    Callback,
}

fn rate_quota() -> Quota {
    Quota::with_period(RATE_LIMIT_WINDOW / RATE_LIMIT)
        .expect("rate limit window is nonzero")
        .allow_burst(NonZeroU32::new(RATE_LIMIT).expect("rate limit is nonzero"))
}

/// Coarse rate limit for the two unauthenticated auth endpoints: they are
/// reachable from a public tunnel, and each one can trigger an outbound GitHub
/// request.
struct RateLimit;

struct RateLimiters {
    start: DefaultKeyedRateLimiter<String>,
    callback: DefaultKeyedRateLimiter<String>,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            start: RateLimiter::keyed(rate_quota()),
            callback: RateLimiter::keyed(rate_quota()),
        }
    }
}

fn rate_limiters() -> &'static RateLimiters {
    static LIMITERS: OnceLock<RateLimiters> = OnceLock::new();
    LIMITERS.get_or_init(RateLimiters::default)
}

impl RateLimit {
    fn allow_start(headers: &HeaderMap, peer: Option<SocketAddr>) -> bool {
        Self::allow(AuthEndpoint::Start, headers, peer)
    }

    fn allow_callback(headers: &HeaderMap, peer: Option<SocketAddr>) -> bool {
        Self::allow(AuthEndpoint::Callback, headers, peer)
    }

    fn allow(endpoint: AuthEndpoint, headers: &HeaderMap, peer: Option<SocketAddr>) -> bool {
        let limiters = rate_limiters();
        let limiter = match endpoint {
            AuthEndpoint::Start => &limiters.start,
            AuthEndpoint::Callback => &limiters.callback,
        };
        // Forwarded addresses are client-controlled when the direct peer is a
        // local proxy, so every value becomes a key. Retain only buckets that
        // are still inside their window instead of letting the map grow.
        if PRUNE_COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(PRUNE_EVERY)
        {
            limiters.start.retain_recent();
            limiters.callback.retain_recent();
        }
        allow_key(limiter, &client_ip_for_headers(headers, peer))
    }
}

const PRUNE_EVERY: u64 = 1024;
static PRUNE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn allow_key(limiter: &DefaultKeyedRateLimiter<String>, key: &str) -> bool {
    if limiter.check_key(&key.to_owned()).is_err() {
        tracing::warn!("auth endpoint rate limit reached");
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use governor::clock::FakeRelativeClock;
    use governor::state::keyed::DefaultKeyedStateStore;

    type TestLimiter = RateLimiter<
        String,
        DefaultKeyedStateStore<String>,
        FakeRelativeClock,
        governor::middleware::NoOpMiddleware<governor::nanos::Nanos>,
    >;

    fn test_limiter(clock: FakeRelativeClock) -> TestLimiter {
        RateLimiter::new(rate_quota(), DefaultKeyedStateStore::default(), clock)
    }

    /// Spends a bucket's full budget against a fixed clock, so tests never
    /// depend on wall time.
    fn spend_budget(limiter: &TestLimiter, key: &str) {
        for _ in 0..RATE_LIMIT {
            assert!(
                limiter.check_key(&key.to_owned()).is_ok(),
                "requests inside the budget must be allowed"
            );
        }
        assert!(
            limiter.check_key(&key.to_owned()).is_err(),
            "the request after the budget must be rejected"
        );
    }

    #[test]
    fn keyed_quota_permits_requests_under_the_limit() {
        let limiter = test_limiter(FakeRelativeClock::default());
        for count in 0..RATE_LIMIT {
            assert!(
                limiter.check_key(&"203.0.113.7".to_owned()).is_ok(),
                "request {count} is inside the budget"
            );
        }
    }

    #[test]
    fn keyed_quota_rejects_requests_over_the_limit() {
        let limiter = test_limiter(FakeRelativeClock::default());
        spend_budget(&limiter, "203.0.113.7");
    }

    #[test]
    fn keyed_quota_refills_after_the_window() {
        let clock = FakeRelativeClock::default();
        let limiter = test_limiter(clock.clone());
        let key = "203.0.113.7";
        spend_budget(&limiter, key);
        clock.advance(RATE_LIMIT_WINDOW + Duration::from_secs(1));
        assert!(
            limiter.check_key(&key.to_owned()).is_ok(),
            "a fresh window must restore the budget"
        );
    }

    #[test]
    fn start_and_callback_budgets_are_independent() {
        let limiters = RateLimiters::default();
        let ip = "203.0.113.7";
        for _ in 0..RATE_LIMIT {
            assert!(limiters.start.check_key(&ip.to_owned()).is_ok());
        }
        assert!(limiters.start.check_key(&ip.to_owned()).is_err());

        // The exhausted start bucket must not consume the callback budget.
        for _ in 0..RATE_LIMIT {
            assert!(
                limiters.callback.check_key(&ip.to_owned()).is_ok(),
                "the callback bucket must still have its full budget"
            );
        }
        assert!(
            limiters.start.check_key(&ip.to_owned()).is_err(),
            "spending the callback budget must not refill the start bucket"
        );
    }

    #[test]
    fn production_limiter_enforces_the_budget_per_client() {
        let peer = Some(SocketAddr::from(([127, 0, 0, 1], 0)));
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("192.0.2.55"));
        for _ in 0..RATE_LIMIT {
            assert!(RateLimit::allow_callback(&headers, peer));
        }
        assert!(!RateLimit::allow_callback(&headers, peer));
        // The other endpoint keeps a separate bucket for the same client.
        assert!(RateLimit::allow_start(&headers, peer));
    }

    #[test]
    fn concurrent_rate_limit_checks_do_not_deadlock() {
        // A loopback peer is what a local tunnel looks like, so the forwarded
        // address is trusted and every request gets its own bucket.
        let peer = Some(SocketAddr::from(([127, 0, 0, 1], 0)));
        let workers: Vec<_> = (0..8)
            .map(|worker| {
                std::thread::spawn(move || {
                    for request in 0..50 {
                        let mut headers = HeaderMap::new();
                        headers.insert(
                            "x-forwarded-for",
                            HeaderValue::from_str(&format!("198.51.100.{worker}:{request}"))
                                .expect("valid test header"),
                        );
                        assert!(RateLimit::allow_callback(&headers, peer));
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("rate limit check panicked");
        }
    }
}
