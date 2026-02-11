use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Pagination query parameters extractor.
///
/// Usage in handlers:
/// ```rust,ignore
/// async fn list_posts(pagination: Pagination) -> impl IntoResponse {
///     // pagination.limit, pagination.offset
/// }
/// ```
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct Pagination {
    /// Number of items to return (default: 20, max: 100)
    #[serde(default = "default_limit")]
    pub limit: u64,

    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    20
}

impl Default for Pagination {
    fn default() -> Self {
        Pagination {
            limit: 20,
            offset: 0,
        }
    }
}

impl Pagination {
    /// Clamp limit to max 100.
    pub fn clamped(&self) -> Self {
        Pagination {
            limit: self.limit.min(100),
            offset: self.offset,
        }
    }
}

impl<S> FromRequestParts<S> for Pagination
where
    S: Send + Sync,
{
    type Rejection = crate::error::ChopinError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        let pagination: Pagination =
            serde_urlencoded::from_str(query).unwrap_or_default();
        Ok(pagination.clamped())
    }
}
