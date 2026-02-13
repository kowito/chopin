use axum::Router;

use crate::controllers;
use crate::controllers::AppState;

/// Build all application routes.
pub fn build_routes() -> Router<AppState> {
    Router::new().nest("/api/auth", controllers::auth::routes())
}
