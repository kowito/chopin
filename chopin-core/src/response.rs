use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ErrorDetail;

/// Standard API response wrapper.
///
/// All Chopin endpoints return this format:
/// ```json
/// {
///   "success": true,
///   "data": { ... },
///   "error": null
/// }
/// ```
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Create a successful response with data.
    pub fn success(data: T) -> Self {
        ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> ApiResponse<T> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(ErrorDetail {
                code: code.into(),
                message: message.into(),
                fields: None,
            }),
        }
    }
}

impl<T: Serialize> axum::response::IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        let status = if self.success {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::BAD_REQUEST
        };
        // Pre-allocate 256 bytes to avoid reallocs for typical small-medium responses.
        // serde_json::to_writer writes directly into the buffer without intermediate copies.
        let mut buf = Vec::with_capacity(256);
        match crate::json::to_writer(&mut buf, &self) {
            Ok(()) => (
                status,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                buf,
            )
                .into_response(),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
            )
                .into_response(),
        }
    }
}
