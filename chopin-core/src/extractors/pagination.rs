use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Pagination query parameters extractor.
///
/// Supports both offset-based and page-based pagination:
/// - `?limit=20&offset=0` (offset-based)
/// - `?page=1&per_page=20` (page-based)
///
/// Usage in handlers:
/// ```rust,ignore
/// async fn list_posts(pagination: Pagination) -> impl IntoResponse {
///     let p = pagination.clamped();
///     let items = Post::find()
///         .offset(p.offset)
///         .limit(p.limit)
///         .all(&db)
///         .await?;
///     let total = Post::find().count(&db).await?;
///     Ok(ApiResponse::success(PaginatedResponse::new(items, total, &p)))
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

    /// Page number (1-based, alternative to offset)
    #[serde(default)]
    pub page: Option<u64>,

    /// Items per page (alternative to limit)
    #[serde(default)]
    pub per_page: Option<u64>,
}

fn default_limit() -> u64 {
    20
}

impl Default for Pagination {
    fn default() -> Self {
        Pagination {
            limit: 20,
            offset: 0,
            page: None,
            per_page: None,
        }
    }
}

impl Pagination {
    /// Clamp limit to max 100 and resolve page-based to offset-based.
    pub fn clamped(&self) -> Self {
        let limit = self.per_page.unwrap_or(self.limit).min(100).max(1);
        let offset = if let Some(page) = self.page {
            (page.max(1) - 1) * limit
        } else {
            self.offset
        };
        Pagination {
            limit,
            offset,
            page: self.page,
            per_page: self.per_page,
        }
    }

    /// Get the current page number (1-based).
    pub fn current_page(&self) -> u64 {
        if let Some(page) = self.page {
            page.max(1)
        } else {
            (self.offset / self.limit.max(1)) + 1
        }
    }

    /// Calculate total pages.
    pub fn total_pages(&self, total_items: u64) -> u64 {
        let limit = self.per_page.unwrap_or(self.limit).max(1);
        (total_items + limit - 1) / limit
    }
}

impl<S> FromRequestParts<S> for Pagination
where
    S: Send + Sync,
{
    type Rejection = crate::error::ChopinError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        let pagination: Pagination = serde_urlencoded::from_str(query).unwrap_or_default();
        Ok(pagination.clamped())
    }
}

/// Paginated response wrapper with metadata.
///
/// ```json
/// {
///   "items": [...],
///   "total": 100,
///   "page": 1,
///   "per_page": 20,
///   "total_pages": 5
/// }
/// ```
#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl<T: Serialize> PaginatedResponse<T> {
    /// Create a paginated response from items, total count, and pagination params.
    pub fn new(items: Vec<T>, total: u64, pagination: &Pagination) -> Self {
        let per_page = pagination.per_page.unwrap_or(pagination.limit).max(1);
        PaginatedResponse {
            items,
            total,
            page: pagination.current_page(),
            per_page,
            total_pages: (total + per_page - 1) / per_page,
        }
    }
}
