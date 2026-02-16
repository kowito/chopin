use axum::Router;

use crate::controllers::AppState;

// ── Re-exports ─────────────────────────────────────────────────
// So users can write `use chopin_core::routing::get;` etc.
pub use axum::routing::{
    any, delete, get, head, method_routing, on, options, patch, post, put, MethodFilter,
    MethodRouter,
};

/// Build core application routes (excluding module-provided routes).
///
/// Module routes (including built-in auth) are registered via
/// `App::mount_module()` and merged separately.
pub fn build_routes() -> Router<AppState> {
    Router::new()
}
