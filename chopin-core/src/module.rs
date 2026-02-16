//! The `ChopinModule` trait — the foundation of Chopin's modular architecture.
//!
//! Every feature (Blog, Auth, Billing) is a self-contained module that
//! implements this trait. Modules declare their own routes, migrations, and
//! health checks, and are composed at startup via `App::mount_module()`.
//!
//! # Architecture
//!
//! Chopin uses a **hub-and-spoke** model:
//! - `chopin-core` is the hub (shared types, traits, services)
//! - Modules are spokes (they depend on core, never on each other)
//!
//! # Example
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//!
//! pub struct BlogModule;
//!
//! #[async_trait]
//! impl ChopinModule for BlogModule {
//!     fn name(&self) -> &str { "blog" }
//!
//!     fn routes(&self) -> Router<AppState> {
//!         Router::new()
//!             .route("/posts", get(list_posts).post(create_post))
//!     }
//! }
//! ```

use async_trait::async_trait;
use axum::Router;
use sea_orm::DatabaseConnection;

use crate::controllers::AppState;
use crate::error::ChopinError;

/// A composable feature module for Chopin applications.
///
/// Implement this trait to create a self-contained module that registers
/// its own routes, runs its own migrations, and exposes a health check.
///
/// Modules follow the **MVSR pattern** (Model-View-Service-Router):
/// - **Model**: SeaORM entities and migrations
/// - **View/Handler**: HTTP handlers (thin adapters)
/// - **Service**: Pure business logic (100% unit-testable)
/// - **Router**: Route definitions mapping paths to handlers
#[async_trait]
pub trait ChopinModule: Send + Sync {
    /// A unique name identifying this module (e.g., "blog", "auth", "billing").
    ///
    /// Used for logging and diagnostics.
    fn name(&self) -> &str;

    /// Return the Axum router containing this module's routes.
    ///
    /// Routes are automatically merged into the main application
    /// when the module is mounted via `App::mount_module()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn routes(&self) -> Router<AppState> {
    ///     Router::new()
    ///         .route("/posts", get(handlers::list_posts).post(handlers::create_post))
    ///         .route("/posts/:id", get(handlers::get_post))
    /// }
    /// ```
    fn routes(&self) -> Router<AppState>;

    /// Run module-specific database migrations.
    ///
    /// Called during application startup after the core migrations.
    /// Default implementation does nothing.
    async fn migrate(&self, _db: &DatabaseConnection) -> Result<(), ChopinError> {
        Ok(())
    }

    /// Optional health check for this module.
    ///
    /// Called when the application health endpoint is hit.
    /// Default implementation always returns Ok.
    async fn health_check(&self) -> Result<(), ChopinError> {
        Ok(())
    }

    /// Optional OpenAPI spec for this module.
    ///
    /// If provided, the spec is merged into the application's OpenAPI documentation.
    /// Default implementation returns None.
    fn openapi_spec(&self) -> Option<utoipa::openapi::OpenApi> {
        None
    }
}
