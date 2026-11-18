//! HTTP router. Routes are grouped by domain as the implementation grows.

use crate::app::AppState;
use crate::assets;
use crate::auth::needs_admin;
use crate::error::ServerError;
use crate::middleware::{csrf_guard, host_guard, security_headers};
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Json;
use axum::Router;
use std::time::Duration;
use threadbase_shared::HealthResponse;
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

pub fn router(state: AppState) -> Router {
    let csrf_state = state.clone();
    let host_state = state.clone();
    Router::new()
        .merge(crate::routes_auth::router())
        .merge(crate::routes_workspace::router())
        .merge(crate::routes_actions::router())
        .merge(crate::routes_agent::router())
        .merge(crate::routes_meta::router())
        .merge(crate::routes_search::router())
        .merge(crate::routes_push::router())
        .merge(crate::routes_comments::router())
        .merge(crate::routes_files::router())
        .merge(crate::routes_fs::router())
        .merge(crate::routes_skills::router())
        .merge(crate::routes_terminal::router())
        .merge(crate::routes_screen::router())
        .merge(crate::sessions::router())
        .route("/api/health", get(health))
        .route("/ws", get(crate::ws::handler))
        .route("/ws/drafts/{thread_id}", get(crate::drafts::handler))
        .fallback(static_asset)
        .with_state(state)
        .layer(from_fn_with_state(csrf_state, csrf_guard))
        .layer(from_fn_with_state(host_state, host_guard))
        .layer(from_fn(security_headers))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60),
        ))
        .layer(TraceLayer::new_for_http())
}

async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ServerError> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        needs_admin: needs_admin(&state.pool).await?,
        bind: state.config.bind.to_string(),
        deploy_staged: state.config.auto_deploy
            && crate::deploy::is_staged(&crate::deploy::staged_path(&state.config)).await,
    }))
}

async fn static_asset(uri: Uri) -> impl IntoResponse {
    // Unknown API routes must not fall through to the SPA shell: callers expect
    // JSON, and a 200 HTML body hides broken clients.
    if uri.path().starts_with("/api/") || uri.path() == "/ws" {
        return crate::error::ServerError(threadbase_shared::ApiError::not_found(
            "unknown endpoint",
        ))
        .into_response();
    }
    assets::serve(&uri)
}
