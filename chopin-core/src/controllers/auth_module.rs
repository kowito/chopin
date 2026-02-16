//! Built-in Auth module — wraps Chopin's authentication endpoints as a `ChopinModule`.
//!
//! This module provides signup, login, logout, token refresh, TOTP, password
//! reset, and email verification endpoints. It is mounted by default at
//! `/api/auth` when using `App::new()`, or can be mounted manually:
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//! use chopin_core::controllers::AuthModule;
//!
//! let app = App::new().await?
//!     .mount_module(AuthModule::new());
//! ```

use async_trait::async_trait;
use axum::Router;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;

use crate::controllers::AppState;
use crate::error::ChopinError;
use crate::migrations::Migrator;
use crate::module::ChopinModule;
use crate::openapi::ApiDoc;
use utoipa::OpenApi;

/// Built-in authentication module.
///
/// Provides all auth-related endpoints (signup, login, refresh, TOTP, etc.)
/// following the MVSR pattern:
///
/// - **Model**: `user`, `refresh_token`, `session`, `security_token`, `login_event`
/// - **View/Handler**: `controllers::auth` handlers
/// - **Service**: `auth::*` (JWT, password hashing, TOTP, rate limiting, etc.)
/// - **Router**: Routes nested at `/api/auth`
pub struct AuthModule {
    /// Base path for auth routes (default: `/api/auth`).
    prefix: String,
}

impl AuthModule {
    /// Create a new AuthModule with the default `/api/auth` prefix.
    pub fn new() -> Self {
        Self {
            prefix: "/api/auth".to_string(),
        }
    }

    /// Create an AuthModule with a custom route prefix.
    ///
    /// ```rust,ignore
    /// let auth = AuthModule::with_prefix("/v2/auth");
    /// ```
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl Default for AuthModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChopinModule for AuthModule {
    fn name(&self) -> &str {
        "auth"
    }

    fn routes(&self) -> Router<AppState> {
        Router::new().nest(&self.prefix, super::auth::routes())
    }

    async fn migrate(&self, db: &DatabaseConnection) -> Result<(), ChopinError> {
        Migrator::up(db, None)
            .await
            .map_err(|e| ChopinError::Internal(format!("Auth migration failed: {e}")))?;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), ChopinError> {
        Ok(())
    }

    fn openapi_spec(&self) -> Option<utoipa::openapi::OpenApi> {
        Some(ApiDoc::openapi())
    }
}
