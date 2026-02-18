//! Authentication and permission middleware for route-level access control.
//!
//! Provides middleware layers that can be applied to routes or route groups
//! for declarative access control.
//!
//! # Usage
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//! use chopin_core::auth::middleware::{login_required_layer, permission_required_layer};
//!
//! Router::new()
//!     // Login required for all routes in this group
//!     .route("/profile", get(profile))
//!     .route_layer(login_required_layer())
//!
//!     // Permission required for specific routes
//!     .route("/admin/users", get(list_users))
//!     .route_layer(permission_required_layer("manage_users"))
//! ```

use std::sync::Arc;

use axum::{extract::Request, middleware::Next, response::Response};
use sea_orm::DatabaseConnection;

use crate::auth;
use crate::auth::rbac::RbacService;
use crate::config::Config;
use crate::error::ChopinError;
use crate::models::user::Entity as User;
use sea_orm::EntityTrait;

/// Create a middleware layer that requires a valid JWT token (login required).
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_core::auth::middleware::login_required_layer;
///
/// Router::new()
///     .route("/profile", get(profile))
///     .route_layer(axum::middleware::from_fn(login_required_layer))
/// ```
pub async fn login_required_layer(req: Request, next: Next) -> Result<Response, ChopinError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ChopinError::Unauthorized("Missing Authorization header".to_string()))?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        ChopinError::Unauthorized("Invalid Authorization header format".to_string())
    })?;

    let config = req
        .extensions()
        .get::<Arc<Config>>()
        .ok_or_else(|| ChopinError::Internal("Config not found in request".to_string()))?;

    let _claims = auth::validate_token(token, &config.jwt_secret)?;

    Ok(next.run(req).await)
}

/// Create a middleware that requires a specific permission.
///
/// Returns a closure suitable for use with `axum::middleware::from_fn`.
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_core::auth::middleware::permission_required_layer;
///
/// Router::new()
///     .route("/admin/users", get(list_users))
///     .route_layer(axum::middleware::from_fn(
///         permission_required_layer("manage_users"),
///     ))
/// ```
pub fn permission_required_layer(
    permission: &'static str,
) -> impl Fn(
    Request,
    Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Response, ChopinError>> + Send>,
> + Clone
       + Send {
    move |req: Request, next: Next| {
        Box::pin(async move {
            // 1. Validate JWT
            let auth_header = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    ChopinError::Unauthorized("Missing Authorization header".to_string())
                })?;

            let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
                ChopinError::Unauthorized("Invalid Authorization header format".to_string())
            })?;

            let config = req
                .extensions()
                .get::<Arc<Config>>()
                .ok_or_else(|| ChopinError::Internal("Config not found".to_string()))?
                .clone();

            let claims = auth::validate_token(token, &config.jwt_secret)?;

            // 2. Get DB and RBAC service
            let db = req
                .extensions()
                .get::<DatabaseConnection>()
                .ok_or_else(|| ChopinError::Internal("Database not found".to_string()))?
                .clone();

            let rbac = req
                .extensions()
                .get::<Arc<RbacService>>()
                .ok_or_else(|| ChopinError::Internal("RBAC service not found".to_string()))?
                .clone();

            // 3. Load user and check permission
            let user_id_i32 = claims
                .sub
                .parse::<i32>()
                .map_err(|_| ChopinError::Unauthorized("Invalid user ID".to_string()))?;

            let user = User::find_by_id(user_id_i32)
                .one(&db)
                .await
                .map_err(|e| ChopinError::Internal(e.to_string()))?
                .ok_or_else(|| ChopinError::Unauthorized("User not found".to_string()))?;

            if !user.is_active {
                return Err(ChopinError::Forbidden("Account is deactivated".to_string()));
            }

            // Superuser bypasses all permission checks
            if user.role != "superuser" {
                rbac.check_permission(&db, &user.role, permission).await?;
            }

            Ok(next.run(req).await)
        })
    }
}
