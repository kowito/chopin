use axum::http::StatusCode;
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;

use crate::response::ApiResponse;

/// Standard error type for the Chopin framework.
#[derive(Debug, Error)]
pub enum ChopinError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

impl ChopinError {
    /// Get the HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            ChopinError::NotFound(_) => StatusCode::NOT_FOUND,
            ChopinError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ChopinError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ChopinError::Forbidden(_) => StatusCode::FORBIDDEN,
            ChopinError::Conflict(_) => StatusCode::CONFLICT,
            ChopinError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ChopinError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ChopinError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get the error code string for this error.
    pub fn error_code(&self) -> &'static str {
        match self {
            ChopinError::NotFound(_) => "NOT_FOUND",
            ChopinError::BadRequest(_) => "BAD_REQUEST",
            ChopinError::Unauthorized(_) => "UNAUTHORIZED",
            ChopinError::Forbidden(_) => "FORBIDDEN",
            ChopinError::Conflict(_) => "CONFLICT",
            ChopinError::Validation(_) => "VALIDATION_ERROR",
            ChopinError::Internal(_) => "INTERNAL_ERROR",
            ChopinError::Database(_) => "DATABASE_ERROR",
        }
    }
}

/// Error detail for API responses.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

impl axum::response::IntoResponse for ChopinError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let body: ApiResponse<()> = ApiResponse {
            success: false,
            data: None,
            error: Some(ErrorDetail {
                code: self.error_code().to_string(),
                message: self.to_string(),
            }),
        };

        (status, axum::Json(body)).into_response()
    }
}
