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

    #[error("Validation errors")]
    ValidationErrors(Vec<FieldError>),

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
            ChopinError::ValidationErrors(_) => StatusCode::UNPROCESSABLE_ENTITY,
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
            ChopinError::ValidationErrors(_) => "VALIDATION_ERROR",
            ChopinError::Internal(_) => "INTERNAL_ERROR",
            ChopinError::Database(_) => "DATABASE_ERROR",
        }
    }

    /// Create a validation error with field-level details.
    pub fn validation_fields(errors: Vec<FieldError>) -> Self {
        ChopinError::ValidationErrors(errors)
    }
}

/// Error detail for API responses.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldError>>,
}

/// Field-level validation error.
///
/// ```json
/// {
///   "field": "email",
///   "message": "must be a valid email address",
///   "code": "invalid_format"
/// }
/// ```
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FieldError {
    pub field: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl FieldError {
    /// Create a new field error.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        FieldError {
            field: field.into(),
            message: message.into(),
            code: None,
        }
    }

    /// Create a new field error with a code.
    pub fn with_code(
        field: impl Into<String>,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        FieldError {
            field: field.into(),
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

impl axum::response::IntoResponse for ChopinError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let fields = match &self {
            ChopinError::ValidationErrors(errs) => Some(errs.clone()),
            _ => None,
        };
        let message = match &self {
            ChopinError::ValidationErrors(errs) => errs
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; "),
            _ => self.to_string(),
        };
        let body: ApiResponse<()> = ApiResponse {
            success: false,
            data: None,
            error: Some(ErrorDetail {
                code: self.error_code().to_string(),
                message,
                fields,
            }),
        };

        // Use sonic-rs for ARM NEON accelerated serialization
        match sonic_rs::to_vec(&body) {
            Ok(bytes) => (
                status,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            Err(_) => (status, "Internal Server Error").into_response(),
        }
    }
}
