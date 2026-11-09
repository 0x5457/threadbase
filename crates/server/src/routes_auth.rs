//! Authentication, setup and user administration routes.

use crate::auth::{
    self, clear_cookie, cookie_value, cookies_secure, issue_session, revoke_all_sessions,
    revoke_session, session_cookie, CurrentUser, SESSION_COOKIE,
};
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::header::{SET_COOKIE, USER_AGENT};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::Json;
use axum::Router;
use serde::Serialize;
use std::net::SocketAddr;
use threadbase_shared::api::{LoginRequest, SetupRequest, UpdateUserRequest};
use threadbase_shared::{ApiError, AuthMeDto, AuthStatusResponse, LoginResponse, UserListResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(status))
        .route("/api/auth/setup", post(setup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/logout-all", post(logout_all))
        .route("/api/auth/me", get(me))
        .route("/api/auth/users", get(list_users))
        .route("/api/auth/users/{user_id}", patch(update_user))
}

async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthStatusResponse>, ServerError> {
    let tailscale_header =
        headers.contains_key("tailscale-user-login") || headers.contains_key("tailscale-user");
    Ok(Json(
        auth::auth_status(&state.pool, tailscale_header).await?,
    ))
}

async fn setup(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> Result<Response, ServerError> {
    if auth::has_tailscale_identity(&headers) {
        return Err(ServerError(ApiError::forbidden(
            "complete first-run setup from the local machine",
        )));
    }
    let user = auth::setup_admin(&state.pool, &request).await?;
    let token = issue_session(
        &state.pool,
        user.id.as_str(),
        &peer.ip().to_string(),
        headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok()),
    )
    .await?;
    auth::audit(
        &state.pool,
        Some(user.id.as_str()),
        "auth.setup",
        Some("user"),
        Some(user.id.as_str()),
        None,
        Some(&peer.ip().to_string()),
    )
    .await;
    let secure = cookies_secure(&headers, state.config.public_origin.as_deref());
    Ok(json_with_cookie(
        LoginResponse {
            user,
            needs_setup: false,
        },
        session_cookie(&token, secure),
    ))
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ServerError> {
    let key = format!(
        "login:{}:{}",
        peer.ip(),
        request.username.trim().to_lowercase()
    );
    if !state
        .login_limiter
        .check(&key, 5, std::time::Duration::from_secs(60))
    {
        return Err(ServerError(ApiError::new(
            threadbase_shared::ErrorCode::RateLimited,
            "too many login attempts; try again in a minute",
        )));
    }
    let user = auth::login(&state.pool, &request).await?;
    let token = issue_session(
        &state.pool,
        user.id.as_str(),
        &peer.ip().to_string(),
        headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok()),
    )
    .await?;
    auth::audit(
        &state.pool,
        Some(user.id.as_str()),
        "auth.login",
        Some("user"),
        Some(user.id.as_str()),
        None,
        Some(&peer.ip().to_string()),
    )
    .await;
    let secure = cookies_secure(&headers, state.config.public_origin.as_deref());
    Ok(json_with_cookie(
        LoginResponse {
            user,
            needs_setup: false,
        },
        session_cookie(&token, secure),
    ))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        revoke_session(&state.pool, &token).await?;
    }
    let secure = cookies_secure(&headers, state.config.public_origin.as_deref());
    Ok(json_with_cookie(
        serde_json::json!({ "ok": true }),
        clear_cookie(secure),
    ))
}

async fn logout_all(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    revoke_all_sessions(&state.pool, user.user.id.as_str()).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "auth.logout_all",
        Some("user"),
        Some(user.user.id.as_str()),
        None,
        None,
    )
    .await;
    let secure = cookies_secure(&headers, state.config.public_origin.as_deref());
    Ok(json_with_cookie(
        serde_json::json!({ "ok": true }),
        clear_cookie(secure),
    ))
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<AuthMeDto>, ServerError> {
    Ok(Json(auth::me(&state.pool, &user).await?))
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
    Path(target_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserListResponse>, ServerError> {
    user.require_admin()?;
    let updated = auth::update_user_admin(&state.pool, &target_id, &request).await?;
    auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "user.admin_changed",
        Some("user"),
        Some(updated.id.as_str()),
        Some(if request.is_admin {
            "granted"
        } else {
            "revoked"
        }),
        None,
    )
    .await;
    Ok(Json(UserListResponse {
        users: auth::list_users(&state.pool).await?,
    }))
}

fn json_with_cookie<T: Serialize>(body: T, cookie: String) -> Response {
    ([(SET_COOKIE, cookie)], Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Method, Request, StatusCode};
    use serde_json::{json, Value};
    use sqlx::SqlitePool;
    use std::net::{IpAddr, SocketAddr};
    use tower::ServiceExt;

    pub(super) async fn test_app() -> (axum::Router, SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            db_path: dir.path().join("threadbase.db"),
            bind: "127.0.0.1:7777".parse().unwrap(),
            public_origin: None,
            log_json: false,
        };
        let pool = db::connect_memory().await.unwrap();
        let state = AppState::new(config, pool.clone());
        (crate::routes::router(state), pool, dir)
    }

    pub(super) fn request(
        method: Method,
        uri: &str,
        body: Option<Value>,
        ip: IpAddr,
        custom_header: bool,
        origin: bool,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost")
            .header("content-type", "application/json");
        if custom_header {
            builder = builder.header("x-requested-with", "threadbase");
        }
        if origin {
            builder = builder.header("origin", "http://localhost");
        }
        let body = body
            .map(|value| Body::from(value.to_string()))
            .unwrap_or_else(Body::empty);
        let mut request = builder.body(body).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(ip, 50000)));
        request
    }

    pub(super) async fn send(
        app: &axum::Router,
        request: Request<Body>,
    ) -> (StatusCode, HeaderMap, Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, headers, json)
    }

    fn header(headers: &HeaderMap, name: &str) -> String {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }

    pub(super) fn cookie_token(headers: &HeaderMap) -> String {
        header(headers, "set-cookie")
            .split(';')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    pub(super) fn setup_body() -> Value {
        json!({
            "username": "ada",
            "password": "correct horse battery",
            "displayName": "Ada Lovelace"
        })
    }

    #[tokio::test]
    async fn setup_login_logout_and_me() {
        let (app, _pool, _dir) = test_app().await;
        let (status, _, body) = send(
            &app,
            request(
                Method::GET,
                "/api/auth/status",
                None,
                IpAddr::from([127, 0, 0, 1]),
                true,
                false,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"]["needsSetup"], true);

        let (status, headers, body) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["user"]["isAdmin"], true);
        let cookie = cookie_token(&headers);
        assert!(cookie.starts_with("threadbase_session="));

        let (status, _, body) = send(
            &app,
            request_with_cookie(Method::GET, "/api/auth/me", &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["username"], "ada");
        assert_eq!(body["authMethod"], "session");

        let (status, _headers, _) = send(
            &app,
            request_with_cookie(Method::POST, "/api/auth/logout", &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = send(
            &app,
            request_with_cookie(Method::GET, "/api/auth/me", &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, headers, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/login",
                Some(json!({"username": "ada", "password": "correct horse battery"})),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(cookie_token(&headers).starts_with("threadbase_session="));
    }

    pub(super) fn request_with_cookie(method: Method, uri: &str, cookie: &str) -> Request<Body> {
        let mut request = request(method, uri, None, IpAddr::from([127, 0, 0, 1]), true, true);
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        request
    }

    #[tokio::test]
    async fn csrf_requires_custom_header_and_origin() {
        let (app, _pool, _dir) = test_app().await;
        let (status, _, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                false,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                false,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn tailscale_identity_is_only_trusted_from_loopback() {
        let (app, _pool, _dir) = test_app().await;
        let mut loopback = request(
            Method::GET,
            "/api/auth/me",
            None,
            IpAddr::from([127, 0, 0, 1]),
            true,
            false,
        );
        loopback
            .headers_mut()
            .insert("tailscale-user-login", "ada@example.com".parse().unwrap());
        loopback
            .headers_mut()
            .insert("tailscale-user-name", "Ada".parse().unwrap());
        let (status, _, body) = send(&app, loopback).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["username"], "ada");
        assert_eq!(body["authMethod"], "tailscale");

        let mut remote = request(
            Method::GET,
            "/api/auth/me",
            None,
            IpAddr::from([10, 0, 0, 1]),
            true,
            false,
        );
        remote
            .headers_mut()
            .insert("tailscale-user-login", "eve@example.com".parse().unwrap());
        let (status, _, _) = send(&app, remote).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn passwords_use_argon2id() {
        let (app, pool, _dir) = test_app().await;
        let (status, _, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let hash: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE username = 'ada'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(hash.starts_with("$argon2id$"));
    }

    #[tokio::test]
    async fn login_is_rate_limited() {
        let (app, _pool, _dir) = test_app().await;
        send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        let mut last = StatusCode::OK;
        for _ in 0..6 {
            let (status, _, _) = send(
                &app,
                request(
                    Method::POST,
                    "/api/auth/login",
                    Some(json!({"username": "ada", "password": "wrong"})),
                    IpAddr::from([127, 0, 0, 1]),
                    true,
                    true,
                ),
            )
            .await;
            last = status;
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn logout_all_revokes_existing_sessions() {
        let (app, _pool, _dir) = test_app().await;
        let (_, headers, _) = send(
            &app,
            request(
                Method::POST,
                "/api/auth/setup",
                Some(setup_body()),
                IpAddr::from([127, 0, 0, 1]),
                true,
                true,
            ),
        )
        .await;
        let cookie = cookie_token(&headers);
        let (status, _, _) = send(
            &app,
            request_with_cookie(Method::POST, "/api/auth/logout-all", &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = send(
            &app,
            request_with_cookie(Method::GET, "/api/auth/me", &cookie),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn thread_events_require_authentication() {
        let (app, _pool, _dir) = test_app().await;
        let (status, _, _) = send(
            &app,
            request(
                Method::GET,
                "/api/threads/t1/events",
                None,
                IpAddr::from([127, 0, 0, 1]),
                false,
                false,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[cfg(test)]
mod setup_guard_tests {
    use super::tests::{request, send, setup_body, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::Value;
    use std::net::IpAddr;

    #[tokio::test]
    async fn setup_is_rejected_through_tailscale() {
        let (app, _pool, _dir) = test_app().await;
        let mut req = request(
            Method::POST,
            "/api/auth/setup",
            Some(setup_body()),
            IpAddr::from([127, 0, 0, 1]),
            true,
            true,
        );
        req.headers_mut()
            .insert("tailscale-user-login", "eve@example.com".parse().unwrap());
        let (status, _, body): (StatusCode, _, Value) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
}
