use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use crate::error::ChopinError;

/// JSON extractor using serde_json for deserialization.
///
/// Usage in handlers:
/// ```rust,ignore
/// async fn create_post(Json(payload): Json<CreatePost>) -> impl IntoResponse {
///     // payload is deserialized from request body
/// }
/// ```
pub struct Json<T>(pub T);

impl<S, T> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ChopinError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .map_err(|e| ChopinError::BadRequest(format!("Failed to read body: {}", e)))?;

        let value: T = crate::json::from_slice(&bytes)
            .map_err(|e| ChopinError::Validation(format!("Invalid JSON: {}", e)))?;

        Ok(Json(value))
    }
}

impl<T: serde::Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        // Serialize into thread-local BytesMut → zero-copy Bytes.
        // Avoids per-request Vec allocation (see json::to_bytes).
        match crate::json::to_bytes(&self.0) {
            Ok(bytes) => (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response(),
        }
    }
}
