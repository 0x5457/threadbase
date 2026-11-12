//! Authentication endpoints.
//!
//! Sign-in is a GitHub OAuth redirect that ends in an `HttpOnly` session
//! cookie; there is no password endpoint and no identity header to spoof.

use crate::app::AppState;
use crate::auth::{self, CurrentUser};
use crate::auth_github;
use crate::auth_session;
use crate::error::ServerError;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{AuthMeDto, AuthStatusResponse, UserListResponse};
use url::form_urlencoded;

/// Handshake cookie is only presented to the callback.
const HANDSHAKE_PATH: &str = "/api/auth/github";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(status))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/github/start", get(github_start))
        .route("/api/auth/github/callback", get(github_callback))
        .route("/api/auth/users", get(list_users))
        .route("/api/auth/users/{user_id}", patch(update_user))
}

async fn status(State(state): State<AppState>) -> Result<Json<AuthStatusResponse>, ServerError> {
    Ok(Json(
        auth::auth_status(
            &state.pool,
            state.config.github_ready(),
            state.config.dev_identity.is_some(),
        )
        .await?,
    ))
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<AuthMeDto>, ServerError> {
    Ok(Json(auth::me(&state.pool, &user).await?))
}

/// Clears the session even when it already expired, so a stale cookie cannot
/// trap the browser in a sign-in loop.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = auth_session::read_cookie(&headers, auth_session::COOKIE) {
        if let Err(error) = auth_session::revoke(&state.pool, &token).await {
            tracing::warn!(%error, "failed to revoke session");
        }
    }
    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        cookie(auth_session::clear_cookie(
            auth_session::COOKIE,
            state.config.cookie_secure,
        )),
    );
    (StatusCode::NO_CONTENT, headers).into_response()
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

    let identity = auth::Identity::github(
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

    let ip = crate::auth::client_ip_for_headers(&headers);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let token = match auth_session::create(
        &state.pool,
        user.id.as_str(),
        auth::METHOD_GITHUB,
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
        Some(auth::METHOD_GITHUB),
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
        let key = crate::auth::client_ip_for_headers(headers);
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

async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<UserListResponse>, ServerError> {
    user.require_admin()?;
    Ok(Json(UserListResponse {
        users: auth::list_users(&state.pool).await?,
    }))
}

async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(target_id): axum::extract::Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserListResponse>, ServerError> {
    user.require_admin()?;
    let updated =
        auth::update_user(&state.pool, user.user.id.as_str(), &target_id, &request).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "user.access_changed",
        Some("user"),
        Some(updated.id.as_str()),
        Some(&format!(
            "admin={} access={}",
            request.is_admin,
            request.access.as_str()
        )),
        None,
    )
    .await;
    Ok(Json(UserListResponse {
        users: auth::list_users(&state.pool).await?,
    }))
}

#[cfg(test)]
mod tests {
    use crate::app::AppState;
    use crate::config::{Config, GitHubOAuth};
    use crate::db;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Method, Request, StatusCode};
    use sqlx::SqlitePool;
    use std::net::{IpAddr, SocketAddr};
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, SqlitePool, tempfile::TempDir) {
        test_app_with(Config::default()).await
    }

    async fn test_app_with(config: Config) -> (axum::Router, SqlitePool, tempfile::TempDir) {
        let data = tempfile::TempDir::new().unwrap();
        let pool = db::connect_memory().await.unwrap();
        let config = Config {
            data_dir: data.path().to_path_buf(),
            db_path: data.path().join("threadbase.db"),
            ..config
        };
        let state = AppState::new(config, pool.clone());
        (crate::routes::router(state), pool, data)
    }

    fn request(method: Method, uri: &str, login: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(login) = login {
            // Legacy Tailscale headers must be inert.
            builder = builder.header("tailscale-user-login", login);
        }
        let mut request = builder
            .header("host", "127.0.0.1:7777")
            .header("origin", "http://127.0.0.1:7777")
            .header("x-requested-with", "threadbase")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
            IpAddr::from([127, 0, 0, 1]),
            1234,
        )));
        request
    }

    fn dev_app() -> impl std::future::Future<Output = (axum::Router, SqlitePool, tempfile::TempDir)>
    {
        test_app_with(Config {
            dev_identity: Some("ada".to_owned()),
            ..Config::default()
        })
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn status_reports_github_and_dev_options() {
        let (app, _pool, _data) = test_app_with(Config {
            public_origin: Some("https://machine.example.ts.net".to_owned()),
            github: Some(GitHubOAuth {
                client_id: "id".to_owned(),
                client_secret: "secret".to_owned(),
            }),
            ..Config::default()
        })
        .await;
        let response = app
            .clone()
            .oneshot(request(Method::GET, "/api/auth/status", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["status"]["github"], true);
        assert_eq!(body["status"]["loginUrl"], "/api/auth/github/start");
        assert_eq!(body["status"]["needsAdmin"], true);

        let response = app
            .oneshot(request(Method::GET, "/api/auth/status", None))
            .await
            .unwrap();
        let body = body_json(response).await;
        assert_eq!(body["status"]["devIdentity"], false);
    }

    #[tokio::test]
    async fn status_without_github_configuration_offers_no_login_url() {
        let (app, _pool, _data) = test_app().await;
        let response = app
            .oneshot(request(Method::GET, "/api/auth/status", None))
            .await
            .unwrap();
        let body = body_json(response).await;
        assert_eq!(body["status"]["github"], false);
        assert!(body["status"]["loginUrl"].is_null());
    }

    #[tokio::test]
    async fn tailscale_headers_no_longer_sign_anyone_in() {
        let (app, _pool, _data) = test_app().await;
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/auth/me",
                Some("ada@example.com"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // An older deployment's identity column must not be usable either.
        let response = app
            .oneshot(request(Method::GET, "/api/auth/me", Some("root")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dev_identity_bootstraps_admin_and_sets_no_cookie() {
        let (app, _pool, _data) = dev_app().await;
        let response = app
            .clone()
            .oneshot(request(Method::GET, "/api/auth/me", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["authMethod"], "dev");
        assert_eq!(body["user"]["username"], "ada");
        assert_eq!(body["user"]["isAdmin"], true);
        assert_eq!(body["needsAdmin"], false);
    }

    #[tokio::test]
    async fn only_the_first_identity_becomes_admin() {
        let (app, pool, _data) = dev_app().await;
        let response = app
            .clone()
            .oneshot(request(Method::GET, "/api/auth/me", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // A GitHub sign-in afterwards must not inherit admin.
        let other = crate::auth::ensure_identity_user(
            &pool,
            &crate::auth::Identity::github("bob", None, None),
            &[],
        )
        .await
        .unwrap();
        assert!(!other.is_admin);
    }

    #[tokio::test]
    async fn sessions_authenticate_and_logout_revokes_them() {
        let (app, pool, _data) = test_app().await;
        let user = crate::auth::ensure_identity_user(
            &pool,
            &crate::auth::Identity::github("ada", Some("Ada"), None),
            &[],
        )
        .await
        .unwrap();
        let token =
            crate::auth_session::create(&pool, user.id.as_str(), "github", None, None, 3600)
                .await
                .unwrap();

        let mut authed = request(Method::GET, "/api/auth/me", None);
        authed.headers_mut().insert(
            "cookie",
            format!("threadbase_session={token}").parse().unwrap(),
        );
        let response = app.clone().oneshot(authed).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["authMethod"], "github");
        assert_eq!(body["user"]["username"], "ada");

        let mut logout = request(Method::POST, "/api/auth/logout", None);
        logout.headers_mut().insert(
            "cookie",
            format!("threadbase_session={token}").parse().unwrap(),
        );
        let response = app.clone().oneshot(logout).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let cleared = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(cleared.contains("Max-Age=0"), "{cleared}");

        let mut stale = request(Method::GET, "/api/auth/me", None);
        stale.headers_mut().insert(
            "cookie",
            format!("threadbase_session={token}").parse().unwrap(),
        );
        let response = app.oneshot(stale).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn callback_without_a_handshake_cookie_is_rejected() {
        let (app, _pool, _data) = test_app_with(Config {
            public_origin: Some("https://machine.example.ts.net".to_owned()),
            github: Some(GitHubOAuth {
                client_id: "id".to_owned(),
                client_secret: "secret".to_owned(),
            }),
            ..Config::default()
        })
        .await;
        let response = app
            .oneshot(request(
                Method::GET,
                "/api/auth/github/callback?code=abc&state=xyz",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(location.starts_with("/?auth_error="), "{location}");
    }

    #[tokio::test]
    async fn callback_state_must_match_the_handshake_cookie() {
        let (app, _pool, _data) = test_app_with(Config {
            public_origin: Some("https://machine.example.ts.net".to_owned()),
            github: Some(GitHubOAuth {
                client_id: "id".to_owned(),
                client_secret: "secret".to_owned(),
            }),
            ..Config::default()
        })
        .await;
        let mut request = request(
            Method::GET,
            "/api/auth/github/callback?code=abc&state=attacker",
            None,
        );
        request.headers_mut().insert(
            "cookie",
            "threadbase_oauth=expected:verifier".parse().unwrap(),
        );
        let response = app.oneshot(request).await.unwrap();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(location.contains("state+did+not+match"), "{location}");
    }

    #[tokio::test]
    async fn start_redirects_to_github_with_pkce_and_sets_the_handshake_cookie() {
        let (app, _pool, _data) = test_app_with(Config {
            public_origin: Some("https://machine.example.ts.net".to_owned()),
            github: Some(GitHubOAuth {
                client_id: "the-client".to_owned(),
                client_secret: "secret".to_owned(),
            }),
            ..Config::default()
        })
        .await;
        let response = app
            .oneshot(request(Method::GET, "/api/auth/github/start", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(
            location.starts_with("https://github.com/login/oauth/authorize?"),
            "{location}"
        );
        assert!(location.contains("code_challenge_method=S256"));
        assert!(location.contains("client_id=the-client"));
        let cookie = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(cookie.starts_with("threadbase_oauth="), "{cookie}");
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Path=/api/auth/github"), "{cookie}");
    }

    #[tokio::test]
    async fn start_without_configuration_explains_itself() {
        let (app, _pool, _data) = test_app().await;
        let response = app
            .oneshot(request(Method::GET, "/api/auth/github/start", None))
            .await
            .unwrap();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(location.contains("not+configured"), "{location}");
    }

    #[tokio::test]
    async fn sign_in_policy_covers_allowlists_and_existing_users() {
        let (_app, pool, _data) = test_app().await;
        let empty: Vec<String> = Vec::new();

        // An unowned instance accepts the first login.
        assert!(crate::auth::may_sign_in(&pool, "ada", &empty, &empty)
            .await
            .unwrap());

        let _admin = crate::auth::ensure_identity_user(
            &pool,
            &crate::auth::Identity::github("ada", None, None),
            &[],
        )
        .await
        .unwrap();
        // Once owned, strangers need an allowlist entry.
        assert!(!crate::auth::may_sign_in(&pool, "stranger", &empty, &empty)
            .await
            .unwrap());
        // Existing users keep access.
        assert!(crate::auth::may_sign_in(&pool, "ada", &empty, &empty)
            .await
            .unwrap());
        let allow = vec!["invited".to_owned()];
        assert!(crate::auth::may_sign_in(&pool, "invited", &empty, &allow)
            .await
            .unwrap());
        let admins = vec!["Owner".to_owned()];
        assert!(crate::auth::may_sign_in(&pool, "owner", &admins, &empty)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn password_endpoints_are_gone() {
        let (app, _pool, _data) = test_app().await;
        for path in ["/api/auth/setup", "/api/auth/logout-all"] {
            let response = app
                .clone()
                .oneshot(request(Method::POST, path, None))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
        let response = app
            .oneshot(request(Method::POST, "/api/auth/users", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
