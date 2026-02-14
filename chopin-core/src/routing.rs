use axum::Router;

use crate::controllers;
use crate::controllers::AppState;

// ── Re-exports ─────────────────────────────────────────────────
// So users can write `use chopin_core::routing::get;` etc.
pub use axum::routing::{
    any, delete, get, head, method_routing, on, options, patch, post, put, MethodFilter,
    MethodRouter,
};

/// Build all application routes.
pub fn build_routes() -> Router<AppState> {
    Router::new().nest("/api/auth", controllers::auth::routes())
}
