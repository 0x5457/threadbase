//! HTTP error mapping. Internal details are logged, never returned to clients.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use threadbase_shared::{ApiError, ApiErrorBody, ErrorCode, ErrorResponse};

#[derive(Debug)]
pub struct ServerError(pub ApiError);

impl ServerError {
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self(ApiError::internal(message))
    }
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ServerError {}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.code.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if status.is_server_error() {
            tracing::error!(code = self.0.code.as_str(), message = %self.0.message, "request failed");
        }
        let body = ErrorResponse {
            error: ApiErrorBody {
                code: self.0.code.as_str().to_owned(),
                message: self.0.message,
                details: None,
            },
        };
        (status, Json(body)).into_response()
    }
}

impl From<ApiError> for ServerError {
    fn from(value: ApiError) -> Self {
        Self(value)
    }
}

impl From<sqlx::Error> for ServerError {
    fn from(value: sqlx::Error) -> Self {
        tracing::error!(error = %value, "database error");
        Self(ApiError::internal("database error"))
    }
}

impl From<crate::events::EventStoreError> for ServerError {
    fn from(value: crate::events::EventStoreError) -> Self {
        match value {
            crate::events::EventStoreError::Api(error) => Self(error),
            other => {
                tracing::error!(error = %other, "event store error");
                Self(ApiError::new(ErrorCode::Internal, "event store error"))
            }
        }
    }
}
