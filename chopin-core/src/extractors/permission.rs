use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

use crate::auth;
use crate::auth::rbac::RbacService;
use crate::config::Config;
use crate::error::ChopinError;
use crate::models::user::{Entity as User, Role};
use sea_orm::EntityTrait;

/// Extractor that validates JWT, loads user role, and provides permission checking.
///
/// This is the primary extractor for the RBAC system. It validates the JWT token,
/// loads the user's role from the database, and pre-loads all permissions for that role.
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_core::prelude::*;
///
/// // With the macro (recommended):
/// #[permission_required("can_edit_posts")]
/// async fn edit_post(guard: PermissionGuard) -> impl IntoResponse {
///     let user_id = guard.user_id();
///     // ...
/// }
///
/// // Manual check in handler:
/// async fn my_handler(guard: PermissionGuard) -> Result<ApiResponse<String>, ChopinError> {
///     guard.require("can_edit_posts")?;
///     ApiResponse::success("ok".to_string())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PermissionGuard {
    user_id: String,
    role: String,
    permissions: Vec<String>,
}

impl PermissionGuard {
    /// Get the authenticated user's ID.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the authenticated user's role name.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Get all permission codenames for this user's role.
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }

    /// Check if the user has a specific permission. Returns `Ok(())` or `Forbidden`.
    ///
    /// Superuser role always passes.
    pub fn require(&self, permission: &str) -> Result<(), ChopinError> {
        if self.role == "superuser" || self.permissions.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(ChopinError::Forbidden(format!(
                "Permission '{}' required",
                permission
            )))
        }
    }

    /// Check if the user has ALL of the specified permissions.
    pub fn require_all(&self, permissions: &[&str]) -> Result<(), ChopinError> {
        if self.role == "superuser" {
            return Ok(());
        }
        for perm in permissions {
            if !self.permissions.iter().any(|p| p == perm) {
                return Err(ChopinError::Forbidden(format!(
                    "Permission '{}' required",
                    perm
                )));
            }
        }
        Ok(())
    }

    /// Check if the user has ANY of the specified permissions.
    pub fn require_any(&self, permissions: &[&str]) -> Result<(), ChopinError> {
        if self.role == "superuser" {
            return Ok(());
        }
        for perm in permissions {
            if self.permissions.iter().any(|p| p == perm) {
                return Ok(());
            }
        }
        Err(ChopinError::Forbidden(format!(
            "One of permissions {:?} required",
            permissions
        )))
    }

    /// Check if the user has a specific permission (returns bool, no error).
    pub fn has_permission(&self, permission: &str) -> bool {
        self.role == "superuser" || self.permissions.iter().any(|p| p == permission)
    }

    /// Check if the user has at least the given role level.
    pub fn has_role(&self, required: &Role) -> bool {
        let user_role = self.role.parse::<Role>().unwrap_or(Role::User);
        user_role.has_permission(required)
    }

    /// Require a minimum role level (e.g., admin).
    pub fn require_role(&self, required: &Role) -> Result<(), ChopinError> {
        if self.has_role(required) {
            Ok(())
        } else {
            Err(ChopinError::Forbidden(format!(
                "{} access required",
                required.as_str()
            )))
        }
    }

    /// Create a PermissionGuard for testing purposes.
    pub fn test_guard(user_id: &str, role: &str, permissions: Vec<String>) -> Self {
        Self {
            user_id: user_id.to_string(),
            role: role.to_string(),
            permissions,
        }
    }
}

impl<S> FromRequestParts<S> for PermissionGuard
where
    S: Send + Sync,
{
    type Rejection = ChopinError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1. Extract and validate JWT token
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ChopinError::Unauthorized("Missing Authorization header".to_string()))?;

        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            ChopinError::Unauthorized("Invalid Authorization header format".to_string())
        })?;

        let config = parts
            .extensions
            .get::<Arc<Config>>()
            .ok_or_else(|| ChopinError::Internal("Config not found in request".to_string()))?;

        let claims = auth::validate_token(token, &config.jwt_secret)?;

        // 2. Get DatabaseConnection and RbacService from extensions
        let db = parts
            .extensions
            .get::<DatabaseConnection>()
            .ok_or_else(|| {
                ChopinError::Internal("Database connection not found in request".to_string())
            })?;

        let rbac = parts.extensions.get::<Arc<RbacService>>().ok_or_else(|| {
            ChopinError::Internal("RBAC service not found in request".to_string())
        })?;

        // 3. Load user and their role from the database
        let user_id_i32 = claims
            .sub
            .parse::<i32>()
            .map_err(|_| ChopinError::Unauthorized("Invalid user ID in token".to_string()))?;

        let user = User::find_by_id(user_id_i32)
            .one(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Database error: {e}")))?
            .ok_or_else(|| ChopinError::Unauthorized("User not found".to_string()))?;

        if !user.is_active {
            return Err(ChopinError::Forbidden("Account is deactivated".to_string()));
        }

        // 4. Load permissions for this role (cached in RbacService)
        let permissions = rbac.get_permissions_for_role(db, &user.role).await?;

        Ok(PermissionGuard {
            user_id: claims.sub,
            role: user.role,
            permissions,
        })
    }
}
