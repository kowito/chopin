use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::auth::rate_limit::RateLimiter;
use crate::cache::CacheService;
use crate::config::Config;

/// Shared application state available in all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub cache: CacheService,
    pub rate_limiter: Arc<RateLimiter>,
}

pub mod auth;
pub mod auth_module;

pub use auth_module::AuthModule;
