pub mod auth_user;
pub mod json;
pub mod pagination;
pub mod permission;
pub mod role;

pub use auth_user::AuthUser;
pub use json::Json;
pub use pagination::{PaginatedResponse, Pagination};
pub use permission::PermissionGuard;
pub use role::{require_role, AuthUserWithRole};

// ── Axum extractor re-exports ──────────────────────────────────
// Common axum extractors available under `chopin_core::extractors::`.
pub use axum::extract::{ConnectInfo, MatchedPath, OriginalUri, Path, Query, State};
pub use axum::Extension;
