//! Host, CSRF and response hardening middleware.

use crate::app::AppState;
use crate::config::Config;
use crate::error::ServerError;
use axum::extract::{Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use threadbase_shared::{ApiError, ErrorCode};

const CUSTOM_HEADER: &str = "x-requested-with";
const CUSTOM_VALUE: &str = "threadbase";
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; \
     font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; \
     base-uri 'none'; form-action 'self'";

pub async fn host_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok());
    if !host.is_some_and(|host| state.config.is_allowed_host(host)) {
        return Err(ServerError(ApiError::validation("host is not allowed")));
    }
    Ok(next.run(request).await)
}

/// True when `origin` is an http(s) origin whose host is allowed for this
/// server. Comparing against the configured hosts, rather than the request's
/// own Host header, closes the DNS-rebinding bypass.
pub fn origin_allowed(config: &Config, origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    url.host_str()
        .is_some_and(|host| config.is_allowed_host(host))
}

pub async fn csrf_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let method = request.method().clone();
    if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(next.run(request).await);
    }
    // The MCP endpoint authenticates with a bearer token and never reads a cookie,
    // so a cross-site request cannot borrow anyone's authority there — and MCP
    // clients cannot send headers of our choosing. It is the one write path that
    // does not need the browser rule.
    if state.config.mcp && request.uri().path() == "/mcp" {
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
    if !origin_allowed(&state.config, origin) {
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
    headers.insert(
        "content-security-policy",
        axum::http::HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_must_match_an_allowed_host() {
        let mut config = Config::default();
        assert!(origin_allowed(&config, "http://127.0.0.1:7777"));
        assert!(origin_allowed(&config, "https://localhost"));
        assert!(!origin_allowed(&config, "http://evil.example"));
        assert!(!origin_allowed(&config, "ftp://localhost"));
        assert!(!origin_allowed(&config, "null"));
        config.public_origin = Some("https://machine.example.ts.net".to_owned());
        assert!(origin_allowed(&config, "https://machine.example.ts.net"));
        assert!(!origin_allowed(&config, "https://evil.example"));
    }
}
