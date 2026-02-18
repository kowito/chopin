//! Role-Based Access Control (RBAC) service.
//!
//! Provides database-configurable permission management with in-memory caching.
//! Permissions are assigned to roles in the database, and the framework
//! automatically enforces them via extractors and middleware.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────┐    ┌──────────────────┐    ┌────────────────┐
//! │  Request  │───→│ PermissionGuard  │───→│  RbacService   │
//! │ (JWT)     │    │ (extractor)      │    │ (cached check) │
//! └──────────┘    └──────────────────┘    └────────┬───────┘
//!                                                  │ cache miss
//!                                         ┌────────▼───────┐
//!                                         │   Database      │
//!                                         │ (permissions,   │
//!                                         │  role_perms)    │
//!                                         └────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//!
//! // Using the macro:
//! #[permission_required("can_edit_posts")]
//! async fn edit_post(...) -> ... { }
//!
//! // Using middleware:
//! Router::new()
//!     .route("/admin", get(admin_panel))
//!     .route_layer(permission_required_layer("manage_users"))
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use tokio::sync::RwLock;

use crate::error::ChopinError;
use crate::models::permission::{self, Entity as Permission};
use crate::models::role_permission::{self, Entity as RolePermission};

/// Default cache TTL: 5 minutes.
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Cached permission set for a role.
#[derive(Clone, Debug)]
struct CachedPermissions {
    permissions: Vec<String>,
    cached_at: Instant,
}

/// RBAC service with in-memory permission caching.
///
/// The service loads role-permission mappings from the database and caches
/// them in memory. Cache entries expire after a configurable TTL (default: 5 min).
#[derive(Clone)]
pub struct RbacService {
    inner: Arc<RbacInner>,
}

struct RbacInner {
    /// Cache: role name → list of permission codenames
    cache: RwLock<HashMap<String, CachedPermissions>>,
    /// Cache time-to-live
    cache_ttl: Duration,
}

impl RbacService {
    /// Create a new RBAC service with default cache TTL (5 minutes).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RbacInner {
                cache: RwLock::new(HashMap::new()),
                cache_ttl: DEFAULT_CACHE_TTL,
            }),
        }
    }

    /// Create a new RBAC service with a custom cache TTL.
    pub fn with_cache_ttl(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RbacInner {
                cache: RwLock::new(HashMap::new()),
                cache_ttl: ttl,
            }),
        }
    }

    /// Check if a role has a specific permission.
    ///
    /// Returns `true` if the role has the permission (from cache or DB).
    /// Superuser role always returns `true` (bypass all permission checks).
    pub async fn has_permission(
        &self,
        db: &DatabaseConnection,
        role: &str,
        permission: &str,
    ) -> Result<bool, ChopinError> {
        // Superuser bypasses all permission checks
        if role == "superuser" {
            return Ok(true);
        }

        let permissions = self.get_permissions_for_role(db, role).await?;
        Ok(permissions.contains(&permission.to_string()))
    }

    /// Check permission or return Forbidden error.
    ///
    /// This is the main entry point used by extractors and middleware.
    pub async fn check_permission(
        &self,
        db: &DatabaseConnection,
        role: &str,
        permission: &str,
    ) -> Result<(), ChopinError> {
        if self.has_permission(db, role, permission).await? {
            Ok(())
        } else {
            Err(ChopinError::Forbidden(format!(
                "Permission '{}' required",
                permission
            )))
        }
    }

    /// Get all permission codenames for a role (cached).
    pub async fn get_permissions_for_role(
        &self,
        db: &DatabaseConnection,
        role: &str,
    ) -> Result<Vec<String>, ChopinError> {
        // Check cache first
        {
            let cache = self.inner.cache.read().await;
            if let Some(cached) = cache.get(role) {
                if cached.cached_at.elapsed() < self.inner.cache_ttl {
                    return Ok(cached.permissions.clone());
                }
            }
        }

        // Cache miss or expired — load from database
        let permissions = self.load_permissions_from_db(db, role).await?;

        // Update cache
        {
            let mut cache = self.inner.cache.write().await;
            cache.insert(
                role.to_string(),
                CachedPermissions {
                    permissions: permissions.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        Ok(permissions)
    }

    /// Load permissions for a role from the database.
    async fn load_permissions_from_db(
        &self,
        db: &DatabaseConnection,
        role: &str,
    ) -> Result<Vec<String>, ChopinError> {
        let role_perms = RolePermission::find()
            .filter(role_permission::Column::Role.eq(role))
            .find_also_related(Permission)
            .all(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to load permissions: {e}")))?;

        let codenames: Vec<String> = role_perms
            .into_iter()
            .filter_map(|(_, perm)| perm.map(|p| p.codename))
            .collect();

        Ok(codenames)
    }

    /// Invalidate cached permissions for a specific role.
    ///
    /// Call this after modifying role-permission mappings in the database.
    pub async fn invalidate_role(&self, role: &str) {
        let mut cache = self.inner.cache.write().await;
        cache.remove(role);
    }

    /// Invalidate all cached permissions.
    pub async fn invalidate_all(&self) {
        let mut cache = self.inner.cache.write().await;
        cache.clear();
    }

    // ═══ Admin API: Permission CRUD ═══

    /// Create a new permission in the database.
    pub async fn create_permission(
        db: &DatabaseConnection,
        codename: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<permission::Model, ChopinError> {
        let now = Utc::now().naive_utc();
        let model = permission::ActiveModel {
            codename: Set(codename.to_string()),
            name: Set(name.to_string()),
            description: Set(description.map(|d| d.to_string())),
            created_at: Set(now),
            ..Default::default()
        };
        model
            .insert(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to create permission: {e}")))
    }

    /// List all permissions in the database.
    pub async fn list_permissions(
        db: &DatabaseConnection,
    ) -> Result<Vec<permission::Model>, ChopinError> {
        Permission::find()
            .all(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to list permissions: {e}")))
    }

    /// Assign a permission to a role.
    pub async fn assign_permission_to_role(
        &self,
        db: &DatabaseConnection,
        role: &str,
        permission_codename: &str,
    ) -> Result<(), ChopinError> {
        // Find the permission by codename
        let perm = Permission::find()
            .filter(permission::Column::Codename.eq(permission_codename))
            .one(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to find permission: {e}")))?
            .ok_or_else(|| {
                ChopinError::NotFound(format!("Permission '{}' not found", permission_codename))
            })?;

        // Check if already assigned
        let existing = RolePermission::find()
            .filter(role_permission::Column::Role.eq(role))
            .filter(role_permission::Column::PermissionId.eq(perm.id))
            .one(db)
            .await
            .map_err(|e| ChopinError::Internal(e.to_string()))?;

        if existing.is_some() {
            return Ok(()); // Already assigned, idempotent
        }

        let now = Utc::now().naive_utc();
        let model = role_permission::ActiveModel {
            role: Set(role.to_string()),
            permission_id: Set(perm.id),
            created_at: Set(now),
            ..Default::default()
        };
        model.insert(db).await.map_err(|e| {
            ChopinError::Internal(format!("Failed to assign permission to role: {e}"))
        })?;

        // Invalidate cache for this role
        self.invalidate_role(role).await;

        Ok(())
    }

    /// Remove a permission from a role.
    pub async fn remove_permission_from_role(
        &self,
        db: &DatabaseConnection,
        role: &str,
        permission_codename: &str,
    ) -> Result<(), ChopinError> {
        let perm = Permission::find()
            .filter(permission::Column::Codename.eq(permission_codename))
            .one(db)
            .await
            .map_err(|e| ChopinError::Internal(e.to_string()))?
            .ok_or_else(|| {
                ChopinError::NotFound(format!("Permission '{}' not found", permission_codename))
            })?;

        RolePermission::delete_many()
            .filter(role_permission::Column::Role.eq(role))
            .filter(role_permission::Column::PermissionId.eq(perm.id))
            .exec(db)
            .await
            .map_err(|e| {
                ChopinError::Internal(format!("Failed to remove permission from role: {e}"))
            })?;

        self.invalidate_role(role).await;
        Ok(())
    }

    /// Get all permissions assigned to a role (from database, not cache).
    pub async fn get_role_permissions(
        db: &DatabaseConnection,
        role: &str,
    ) -> Result<Vec<permission::Model>, ChopinError> {
        let role_perms = RolePermission::find()
            .filter(role_permission::Column::Role.eq(role))
            .find_also_related(Permission)
            .all(db)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to get role permissions: {e}")))?;

        Ok(role_perms
            .into_iter()
            .filter_map(|(_, perm)| perm)
            .collect())
    }
}

impl Default for RbacService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_service_creation() {
        let svc = RbacService::new();
        assert!(Arc::strong_count(&svc.inner) == 1);
    }

    #[test]
    fn test_rbac_service_clone_shares_cache() {
        let svc = RbacService::new();
        let svc2 = svc.clone();
        assert!(Arc::strong_count(&svc.inner) == 2);
        assert!(Arc::strong_count(&svc2.inner) == 2);
    }

    #[test]
    fn test_rbac_service_with_custom_ttl() {
        let svc = RbacService::with_cache_ttl(Duration::from_secs(60));
        assert_eq!(svc.inner.cache_ttl, Duration::from_secs(60));
    }
}
