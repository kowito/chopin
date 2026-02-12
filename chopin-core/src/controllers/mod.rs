use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::cache::CacheService;
use crate::config::Config;

/// Shared application state available in all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub cache: CacheService,
}

pub mod auth;
