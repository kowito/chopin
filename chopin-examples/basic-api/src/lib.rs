pub mod controllers;
pub mod models;
pub mod migrations;

use sea_orm::DatabaseConnection;
use chopin_core::config::Config;

/// Shared application state for all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
}
