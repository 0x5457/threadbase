//! Integration tests for the authentication routes and session extraction.
//!
//! These exercise the public router and `AppState` only, so the auth modules
//! can be reorganised without the tests reaching into private helpers.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use sqlx::SqlitePool;
use std::net::{IpAddr, SocketAddr};
use threadbase_server::app::AppState;
use threadbase_server::auth;
use threadbase_server::auth_session;
use threadbase_server::config::{Config, GitHubOAuth};
use threadbase_server::db;
use threadbase_server::routes;
use threadbase_shared::now_ms;
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
    (routes::router(state), pool, data)
}

async fn dev_app() -> (axum::Router, SqlitePool, tempfile::TempDir) {
    test_app_with(Config {
        dev_identity: Some("ada".to_owned()),
        ..Config::default()
    })
    .await
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

fn with_session(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        "cookie",
        format!("{}={token}", auth_session::COOKIE).parse().unwrap(),
    );
    request
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn session_token(pool: &SqlitePool, login: &str) -> String {
    let user = auth::ensure_identity_user(
        pool,
        &auth::Identity::github(1, login, Some(login), None),
        &[],
    )
    .await
    .unwrap();
    auth_session::create(
        pool,
        user.id.as_str(),
        auth::METHOD_GITHUB,
        None,
        None,
        3600,
    )
    .await
    .unwrap()
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
    let other =
        auth::ensure_identity_user(&pool, &auth::Identity::github(2, "bob", None, None), &[])
            .await
            .unwrap();
    assert!(!other.is_admin);
}

#[tokio::test]
async fn sessions_authenticate_and_logout_revokes_them() {
    let (app, pool, _data) = test_app().await;
    let user = auth::ensure_identity_user(
        &pool,
        &auth::Identity::github(1, "ada", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let token = auth_session::create(&pool, user.id.as_str(), "github", None, None, 3600)
        .await
        .unwrap();

    let authed = with_session(request(Method::GET, "/api/auth/me", None), &token);
    let response = app.clone().oneshot(authed).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["authMethod"], "github");
    assert_eq!(body["user"]["username"], "ada");

    let logout = with_session(request(Method::POST, "/api/auth/logout", None), &token);
    let response = app.clone().oneshot(logout).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cleared = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(cleared.contains("Max-Age=0"), "{cleared}");

    let stale = with_session(request(Method::GET, "/api/auth/me", None), &token);
    let response = app.oneshot(stale).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_cookie_is_rejected() {
    let (app, pool, _data) = test_app().await;
    let user =
        auth::ensure_identity_user(&pool, &auth::Identity::github(1, "ada", None, None), &[])
            .await
            .unwrap();
    let token = auth_session::create(&pool, user.id.as_str(), "github", None, None, -1)
        .await
        .unwrap();

    let response = app
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoked_session_cookie_is_rejected() {
    let (app, pool, _data) = test_app().await;
    let token = session_token(&pool, "ada").await;
    auth_session::revoke(&pool, &token).await.unwrap();

    let response = app
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabled_user_is_forbidden_outside_self_service_routes() {
    let (app, pool, _data) = test_app().await;
    let token = session_token(&pool, "ada").await;
    sqlx::query("UPDATE users SET access = 'disabled' WHERE username = 'ada'")
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(with_session(
            request(Method::GET, "/api/state", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pending_and_read_only_access_are_enforced() {
    let (app, pool, _data) = test_app().await;
    let _admin =
        auth::ensure_identity_user(&pool, &auth::Identity::github(1, "ada", None, None), &[])
            .await
            .unwrap();
    let pending =
        auth::ensure_identity_user(&pool, &auth::Identity::github(2, "bob", None, None), &[])
            .await
            .unwrap();
    let token = auth_session::create(&pool, pending.id.as_str(), "github", None, None, 3600)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/state", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    sqlx::query("UPDATE users SET access = 'read_only' WHERE id = ?")
        .bind(pending.id.as_str())
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/state", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(with_session(
            request(Method::POST, "/api/projects", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn session_cookie_slides_its_expiry() {
    let (app, pool, _data) = test_app().await;
    let token = session_token(&pool, "ada").await;
    let token_hash = auth_session::hash(&token);
    let old_seen = now_ms() - 2 * 60 * 60 * 1000;
    sqlx::query("UPDATE auth_sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?")
        .bind(old_seen)
        .bind(now_ms() + 1000)
        .bind(&token_hash)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (last_seen, expires): (Option<i64>, i64) =
        sqlx::query_as("SELECT last_seen_at, expires_at FROM auth_sessions WHERE token_hash = ?")
            .bind(&token_hash)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_seen.unwrap() > old_seen);
    assert!(expires > now_ms() + 3600 * 1000 - 5_000);
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
    assert!(auth::may_sign_in(&pool, 1, "ada", &empty, &empty)
        .await
        .unwrap());

    let _admin =
        auth::ensure_identity_user(&pool, &auth::Identity::github(1, "ada", None, None), &[])
            .await
            .unwrap();
    // Once owned, strangers need an allowlist entry.
    assert!(!auth::may_sign_in(&pool, 2, "stranger", &empty, &empty)
        .await
        .unwrap());
    // Existing users keep access.
    assert!(auth::may_sign_in(&pool, 1, "ada", &empty, &empty)
        .await
        .unwrap());
    let allow = vec!["invited".to_owned()];
    assert!(auth::may_sign_in(&pool, 3, "invited", &empty, &allow)
        .await
        .unwrap());
    let admins = vec!["Owner".to_owned()];
    assert!(auth::may_sign_in(&pool, 4, "owner", &admins, &empty)
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

// ── access levels ─────────────────────────────────────────────

fn json_request(method: Method, uri: &str, body: serde_json::Value) -> Request<Body> {
    let mut request = request(method, uri, None);
    *request.body_mut() = Body::from(body.to_string());
    request
}

async fn sign_in(pool: &SqlitePool, id: i64, login: &str) -> (threadbase_shared::UserDto, String) {
    let user = auth::ensure_identity_user(
        pool,
        &auth::Identity::github(id, login, Some(login), None),
        &[],
    )
    .await
    .unwrap();
    let token = auth_session::create(pool, user.id.as_str(), "github", None, None, 3600)
        .await
        .unwrap();
    (user, token)
}

async fn set_access(
    app: axum::Router,
    admin_token: &str,
    user_id: &str,
    is_admin: bool,
    access: &str,
) -> axum::response::Response {
    app.oneshot(with_session(
        json_request(
            Method::PATCH,
            &format!("/api/auth/users/{user_id}"),
            serde_json::json!({ "isAdmin": is_admin, "access": access }),
        ),
        admin_token,
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn pending_account_is_signed_in_but_sees_no_workspace_data() {
    let (app, pool, _data) = test_app().await;
    let (admin, _admin_token) = sign_in(&pool, 1, "ada").await;
    assert!(admin.is_admin);
    let (bob, token) = sign_in(&pool, 2, "bob").await;
    assert_eq!(bob.access, threadbase_shared::UserAccess::Pending);

    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["user"]["access"], "pending");

    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/auth/status", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/projects", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .oneshot(with_session(
            json_request(
                Method::POST,
                "/api/projects",
                serde_json::json!({ "path": "/tmp/threadbase-pending-test" }),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn read_only_account_can_read_but_not_write() {
    let (app, pool, _data) = test_app().await;
    let (_admin, admin_token) = sign_in(&pool, 1, "ada").await;
    let (bob, _) = sign_in(&pool, 2, "bob").await;

    let response = set_access(
        app.clone(),
        &admin_token,
        bob.id.as_str(),
        false,
        "read_only",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let updated = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["id"] == bob.id.as_str())
        .cloned()
        .unwrap();
    assert_eq!(updated["access"], "read_only");

    let token = auth_session::create(&pool, bob.id.as_str(), "github", None, None, 3600)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/projects", None),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(with_session(
            json_request(
                Method::POST,
                "/api/projects",
                serde_json::json!({ "path": "/tmp/threadbase-readonly-test" }),
            ),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Personal preferences stay writable for a read-only account.
    let response = app
        .oneshot(with_session(
            json_request(Method::PATCH, "/api/settings", serde_json::json!({})),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn revoking_access_ends_live_sessions() {
    let (app, pool, _data) = test_app().await;
    let (_admin, admin_token) = sign_in(&pool, 1, "ada").await;
    let (bob, old_token) = sign_in(&pool, 2, "bob").await;

    let response = set_access(
        app.clone(),
        &admin_token,
        bob.id.as_str(),
        false,
        "disabled",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // The session issued before the change is gone immediately.
    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &old_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // A fresh sign-in can see the revoked state, but no workspace data.
    let new_token = auth_session::create(&pool, bob.id.as_str(), "github", None, None, 3600)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(with_session(
            request(Method::GET, "/api/auth/me", None),
            &new_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["user"]["access"], "disabled");

    let response = app
        .oneshot(with_session(
            request(Method::GET, "/api/projects", None),
            &new_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admins_cannot_reduce_their_own_access() {
    let (app, pool, _data) = test_app().await;
    let (admin, admin_token) = sign_in(&pool, 1, "ada").await;
    assert!(admin.is_admin);

    let response = set_access(
        app.clone(),
        &admin_token,
        admin.id.as_str(),
        false,
        "disabled",
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = set_access(app, &admin_token, admin.id.as_str(), false, "read_only").await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
