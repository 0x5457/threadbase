//! Browser-facing GitHub OAuth routes: start the redirect and handle the
//! callback. The protocol details live in `auth_github`; this module only
//! wires them to HTTP, cookies and the user table.

use crate::app::AppState;
use crate::auth::{self, client_ip_for_headers, Identity, METHOD_GITHUB};
use crate::auth_github;
use crate::auth_session;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use url::form_urlencoded;

/// Handshake cookie is only presented to the callback.
const HANDSHAKE_PATH: &str = "/api/auth/github";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/github/start", get(github_start))
        .route("/api/auth/github/callback", get(github_callback))
}

async fn github_start(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !RateLimit::allow(&headers) {
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
    let url = auth_github::authorize_url(oauth, &callback, &state_value, &verifier);

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
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
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

    let ip = client_ip_for_headers(&headers);
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

/// Coarse bucket for the two unauthenticated auth endpoints: they are reachable
/// from a public tunnel, and each one can trigger an outbound GitHub request.
struct RateLimit;

impl RateLimit {
    fn allow(headers: &HeaderMap) -> bool {
        const LIMIT: u32 = 30;
        const WINDOW: Duration = Duration::from_secs(60);
        static BUCKETS: OnceLock<Mutex<HashMap<String, (u32, Instant)>>> = OnceLock::new();
        let key = client_ip_for_headers(headers);
        let buckets = BUCKETS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut buckets) = buckets.lock() else {
            return true;
        };
        if buckets.len() > 4096 {
            buckets.retain(|_, (_, started)| started.elapsed() < WINDOW);
        }
        let entry = buckets.entry(key).or_insert((0, Instant::now()));
        if entry.1.elapsed() > WINDOW {
            *entry = (1, Instant::now());
            return true;
        }
        entry.0 = entry.0.saturating_add(1);
        if entry.0 > LIMIT {
            tracing::warn!("auth endpoint rate limit reached");
            return false;
        }
        true
    }
}
