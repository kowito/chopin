use chopin_core::auth::rbac::RbacService;
use chopin_core::extractors::PermissionGuard;
use chopin_core::models::user::Role;
use std::time::Duration;

// ═══ RbacService Unit Tests ═══

#[test]
fn test_rbac_service_creation() {
    let svc = RbacService::new();
    let _ = svc.clone(); // cloneable
}

#[test]
fn test_rbac_service_with_custom_ttl() {
    let svc = RbacService::with_cache_ttl(Duration::from_secs(60));
    let _ = svc.clone();
}

#[test]
fn test_rbac_service_default() {
    let svc = RbacService::default();
    let _ = svc.clone();
}

// ═══ PermissionGuard Unit Tests ═══

#[test]
fn test_permission_guard_require_success() {
    let guard = PermissionGuard::test_guard(
        "1",
        "editor",
        vec!["can_edit_posts".to_string(), "can_view_posts".to_string()],
    );
    assert!(guard.require("can_edit_posts").is_ok());
    assert!(guard.require("can_view_posts").is_ok());
}

#[test]
fn test_permission_guard_require_failure() {
    let guard = PermissionGuard::test_guard("1", "editor", vec!["can_edit_posts".to_string()]);
    let result = guard.require("can_delete_posts");
    assert!(result.is_err());
}

#[test]
fn test_permission_guard_superuser_bypasses_all() {
    let guard = PermissionGuard::test_guard("1", "superuser", vec![]);
    assert!(guard.require("can_edit_posts").is_ok());
    assert!(guard.require("can_delete_users").is_ok());
    assert!(guard.require("any_random_permission").is_ok());
}

#[test]
fn test_permission_guard_require_all_success() {
    let guard = PermissionGuard::test_guard(
        "1",
        "admin",
        vec![
            "can_edit_posts".to_string(),
            "can_delete_posts".to_string(),
            "can_publish".to_string(),
        ],
    );
    assert!(guard
        .require_all(&["can_edit_posts", "can_delete_posts"])
        .is_ok());
}

#[test]
fn test_permission_guard_require_all_partial_failure() {
    let guard = PermissionGuard::test_guard("1", "editor", vec!["can_edit_posts".to_string()]);
    let result = guard.require_all(&["can_edit_posts", "can_delete_posts"]);
    assert!(result.is_err());
}

#[test]
fn test_permission_guard_require_all_superuser_bypass() {
    let guard = PermissionGuard::test_guard("1", "superuser", vec![]);
    assert!(guard
        .require_all(&["can_edit_posts", "can_delete_users", "manage_system"])
        .is_ok());
}

#[test]
fn test_permission_guard_require_any_success() {
    let guard = PermissionGuard::test_guard("1", "editor", vec!["can_edit_posts".to_string()]);
    assert!(guard
        .require_any(&["can_edit_posts", "can_delete_posts"])
        .is_ok());
}

#[test]
fn test_permission_guard_require_any_failure() {
    let guard = PermissionGuard::test_guard("1", "user", vec!["can_view_posts".to_string()]);
    let result = guard.require_any(&["can_edit_posts", "can_delete_posts"]);
    assert!(result.is_err());
}

#[test]
fn test_permission_guard_require_any_superuser_bypass() {
    let guard = PermissionGuard::test_guard("1", "superuser", vec![]);
    assert!(guard
        .require_any(&["can_edit_posts", "can_delete_posts"])
        .is_ok());
}

#[test]
fn test_permission_guard_has_permission() {
    let guard = PermissionGuard::test_guard("1", "editor", vec!["can_edit_posts".to_string()]);
    assert!(guard.has_permission("can_edit_posts"));
    assert!(!guard.has_permission("can_delete_posts"));
}

#[test]
fn test_permission_guard_has_permission_superuser() {
    let guard = PermissionGuard::test_guard("1", "superuser", vec![]);
    assert!(guard.has_permission("literally_anything"));
}

#[test]
fn test_permission_guard_has_role() {
    let guard = PermissionGuard::test_guard("1", "admin", vec![]);
    assert!(guard.has_role(&Role::User));
    assert!(guard.has_role(&Role::Admin));
    assert!(!guard.has_role(&Role::Superuser));
}

#[test]
fn test_permission_guard_require_role_success() {
    let guard = PermissionGuard::test_guard("1", "admin", vec![]);
    assert!(guard.require_role(&Role::User).is_ok());
    assert!(guard.require_role(&Role::Admin).is_ok());
}

#[test]
fn test_permission_guard_require_role_failure() {
    let guard = PermissionGuard::test_guard("1", "user", vec![]);
    let result = guard.require_role(&Role::Admin);
    assert!(result.is_err());
}

#[test]
fn test_permission_guard_getters() {
    let guard = PermissionGuard::test_guard(
        "42",
        "editor",
        vec!["can_edit_posts".to_string(), "can_publish".to_string()],
    );
    assert_eq!(guard.user_id(), "42");
    assert_eq!(guard.role(), "editor");
    assert_eq!(guard.permissions().len(), 2);
    assert!(guard.permissions().contains(&"can_edit_posts".to_string()));
    assert!(guard.permissions().contains(&"can_publish".to_string()));
}

#[test]
fn test_permission_guard_empty_permissions() {
    let guard = PermissionGuard::test_guard("1", "user", vec![]);
    assert!(!guard.has_permission("anything"));
    assert!(guard.require("something").is_err());
    assert!(guard.require_all(&["a", "b"]).is_err());
    assert!(guard.require_any(&["a", "b"]).is_err());
}

// ═══ Error message quality ═══

#[test]
fn test_permission_error_message_includes_codename() {
    let guard = PermissionGuard::test_guard("1", "user", vec![]);
    let err = guard.require("can_edit_posts").unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("can_edit_posts"),
        "Error message should include permission codename, got: {}",
        msg
    );
}

#[test]
fn test_permission_require_any_error_includes_options() {
    let guard = PermissionGuard::test_guard("1", "user", vec![]);
    let err = guard
        .require_any(&["can_edit_posts", "can_delete_posts"])
        .unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("can_edit_posts") || msg.contains("can_delete_posts"),
        "Error message should include attempted permissions, got: {}",
        msg
    );
}

#[test]
fn test_permission_guard_require_role_error_message() {
    let guard = PermissionGuard::test_guard("1", "user", vec![]);
    let err = guard.require_role(&Role::Admin).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("admin"),
        "Error message should include required role, got: {}",
        msg
    );
}

// ═══ Rbac Cache Invalidation ═══

#[tokio::test]
async fn test_rbac_invalidate_role() {
    let svc = RbacService::new();
    svc.invalidate_role("admin").await;
    // Should not panic or error
}

#[tokio::test]
async fn test_rbac_invalidate_all() {
    let svc = RbacService::new();
    svc.invalidate_all().await;
    // Should not panic or error
}
