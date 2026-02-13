pub mod controllers;
pub mod migrations;
pub mod models;

use chopin_core::config::Config;
use sea_orm::DatabaseConnection;

/// Shared application state for all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
}
