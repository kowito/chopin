use sea_orm::DatabaseConnection;

use crate::config::Config;

/// Shared application state available in all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
}

pub mod auth;
