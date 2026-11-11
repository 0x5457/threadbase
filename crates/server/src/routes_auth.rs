//! Authentication endpoints.
//!
//! Sign-in happens in the [`crate::auth::CurrentUser`] extractor from Tailscale
//! identity headers; there is no password endpoint to call.

use crate::auth::{self, CurrentUser};
use crate::state::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, patch};
use axum::Json;
use axum::Router;
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{AuthMeDto, AuthStatusResponse, UserListResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(status))
        .route("/api/auth/me", get(me))
        .route("/api/auth/users", get(list_users))
        .route("/api/auth/users/{user_id}", patch(update_user))
}

async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthStatusResponse>, crate::error::ServerError> {
    let tailscale_header = auth::has_tailscale_identity(&headers);
    Ok(Json(
        auth::auth_status(
            &state.pool,
            tailscale_header,
            state.config.dev_identity.is_some(),
        )
        .await?,
    ))
}

async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<AuthMeDto>, crate::error::ServerError> {
    Ok(Json(auth::me(&state.pool, &user).await?))
}

async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<UserListResponse>, crate::error::ServerError> {
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
) -> Result<Json<UserListResponse>, crate::error::ServerError> {
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

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::db;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Method, Request, StatusCode};
    use sqlx::SqlitePool;
    use std::net::{IpAddr, SocketAddr};
    use tower::ServiceExt;

    async fn test_app() -> (axum::Router, SqlitePool, tempfile::TempDir) {
        let data = tempfile::TempDir::new().unwrap();
        let pool = db::connect_memory().await.unwrap();
        let state = AppState::new(
            Config {
                data_dir: data.path().to_path_buf(),
                db_path: data.path().join("threadbase.db"),
                bind: "127.0.0.1:7777".parse().unwrap(),
                public_origin: None,
                tailscale_admins: Vec::new(),
                dev_identity: None,
                log_json: false,
            },
            pool.clone(),
        );
        (crate::routes::router(state), pool, data)
    }

    fn request(method: Method, uri: &str, login: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(login) = login {
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

    #[tokio::test]
    async fn password_endpoints_are_gone() {
        let (app, _pool, _data) = test_app().await;
        for path in [
            "/api/auth/login",
            "/api/auth/setup",
            "/api/auth/logout",
            "/api/auth/logout-all",
        ] {
            let response = app
                .clone()
                .oneshot(request(Method::POST, path, Some("ada@example.com")))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
        let response = app
            .oneshot(request(
                Method::POST,
                "/api/auth/users",
                Some("ada@example.com"),
            ))
            .await
            .unwrap();
        // The collection route still exists for listing, but password user
        // creation is gone.
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn tailscale_identity_signs_in_and_bootstraps_admin() {
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
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["authMethod"], "tailscale");
        assert_eq!(body["user"]["username"], "ada");
        assert_eq!(body["user"]["isAdmin"], true);
        assert_eq!(body["needsAdmin"], false);
    }

    #[tokio::test]
    async fn only_the_first_identity_becomes_admin() {
        let (app, _pool, _data) = test_app().await;
        let ada = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/auth/me",
                Some("ada@example.com"),
            ))
            .await
            .unwrap();
        assert_eq!(ada.status(), StatusCode::OK);

        let bob = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/auth/me",
                Some("bob@example.com"),
            ))
            .await
            .unwrap();
        let body = axum::body::to_bytes(bob.into_body(), 1 << 20)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["user"]["username"], "bob");
        assert_eq!(body["user"]["isAdmin"], false);
    }

    #[tokio::test]
    async fn requests_without_identity_are_unauthorized() {
        let (app, _pool, _data) = test_app().await;
        let response = app
            .oneshot(request(Method::GET, "/api/auth/me", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn identity_is_only_trusted_from_loopback() {
        let (app, _pool, _data) = test_app().await;
        let mut remote = request(Method::GET, "/api/auth/me", Some("eve@example.com"));
        remote.extensions_mut().insert(ConnectInfo(SocketAddr::new(
            IpAddr::from([100, 64, 0, 9]),
            4321,
        )));
        let response = app.oneshot(remote).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
