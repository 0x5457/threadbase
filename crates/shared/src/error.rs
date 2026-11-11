//! Stable API error codes.

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    Validation,
    Conflict,
    AlreadySent,
    RateLimited,
    Csrf,
    PathOutsideWorkspace,
    AgentUnavailable,
    AgentError,
    Internal,
}

impl ErrorCode {
    #[must_use]
    pub fn http_status(self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::Forbidden | Self::Csrf => 403,
            Self::NotFound => 404,
            Self::Conflict | Self::AlreadySent => 409,
            Self::Validation | Self::PathOutsideWorkspace | Self::AgentUnavailable => 422,
            Self::RateLimited => 429,
            Self::AgentError | Self::Internal => 500,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Validation => "validation",
            Self::Conflict => "conflict",
            Self::AlreadySent => "already_sent",
            Self::RateLimited => "rate_limited",
            Self::Csrf => "csrf",
            Self::PathOutsideWorkspace => "path_outside_workspace",
            Self::AgentUnavailable => "agent_unavailable",
            Self::AgentError => "agent_error",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message)
    }

    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }

    #[must_use]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ApiError {}
