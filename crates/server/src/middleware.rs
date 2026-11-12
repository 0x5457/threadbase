//! CSRF and response hardening middleware.

use crate::app::AppState;
use crate::error::ServerError;
use axum::extract::{Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use threadbase_shared::{ApiError, ErrorCode};

const CUSTOM_HEADER: &str = "x-requested-with";
const CUSTOM_VALUE: &str = "threadbase";

pub async fn csrf_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let method = request.method().clone();
    if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(next.run(request).await);
    }
    let headers = request.headers();
    if headers
        .get(CUSTOM_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some(CUSTOM_VALUE)
    {
        return Err(ServerError(ApiError::new(
            ErrorCode::Csrf,
            "missing X-Requested-With header",
        )));
    }
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return Err(ServerError(ApiError::new(
            ErrorCode::Csrf,
            "missing Origin header",
        )));
    };
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let mut allowed = vec![format!("http://{host}"), format!("https://{host}")];
    if let Some(public_origin) = state.config.public_origin.as_deref() {
        allowed.push(public_origin.trim_end_matches('/').to_owned());
    }
    if !allowed.iter().any(|candidate| candidate == origin) {
        return Err(ServerError(ApiError::new(
            ErrorCode::Csrf,
            "origin is not allowed",
        )));
    }
    Ok(next.run(request).await)
}

pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-frame-options",
        axum::http::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        "referrer-policy",
        axum::http::HeaderValue::from_static("same-origin"),
    );
    response
}
