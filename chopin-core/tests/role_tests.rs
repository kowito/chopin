use chopin_core::models::user::Role;

// ═══ Role::has_permission ═══

#[test]
fn test_superuser_has_permission_for_superuser() {
    assert!(Role::Superuser.has_permission(&Role::Superuser));
}

#[test]
fn test_superuser_has_permission_for_admin() {
    assert!(Role::Superuser.has_permission(&Role::Admin));
}

#[test]
fn test_superuser_has_permission_for_user() {
    assert!(Role::Superuser.has_permission(&Role::User));
}

#[test]
fn test_admin_has_permission_for_admin() {
    assert!(Role::Admin.has_permission(&Role::Admin));
}

#[test]
fn test_admin_has_permission_for_user() {
    assert!(Role::Admin.has_permission(&Role::User));
}

#[test]
fn test_admin_no_permission_for_superuser() {
    assert!(!Role::Admin.has_permission(&Role::Superuser));
}

#[test]
fn test_user_has_permission_for_user() {
    assert!(Role::User.has_permission(&Role::User));
}

#[test]
fn test_user_no_permission_for_admin() {
    assert!(!Role::User.has_permission(&Role::Admin));
}

#[test]
fn test_user_no_permission_for_superuser() {
    assert!(!Role::User.has_permission(&Role::Superuser));
}

// ═══ Role::as_str ═══

#[test]
fn test_role_as_str_admin() {
    assert_eq!(Role::Admin.as_str(), "admin");
}

#[test]
fn test_role_as_str_superuser() {
    assert_eq!(Role::Superuser.as_str(), "superuser");
}

#[test]
fn test_role_as_str_user() {
    assert_eq!(Role::User.as_str(), "user");
}

// ═══ Role parsing ═══

#[test]
fn test_role_parse_admin() {
    let role: Role = "admin".parse().expect("parse admin");
    assert_eq!(role.as_str(), "admin");
}

#[test]
fn test_role_parse_superuser() {
    let role: Role = "superuser".parse().expect("parse superuser");
    assert_eq!(role.as_str(), "superuser");
}

#[test]
fn test_role_parse_user() {
    let role: Role = "user".parse().expect("parse user");
    assert_eq!(role.as_str(), "user");
}

#[test]
fn test_role_parse_unknown_defaults_to_user() {
    let role: Role = "unknown".parse().expect("parse unknown");
    assert_eq!(role.as_str(), "user");
}

#[test]
fn test_role_parse_empty_defaults_to_user() {
    let role: Role = "".parse().expect("parse empty");
    assert_eq!(role.as_str(), "user");
}

// ═══ Role debug ═══

#[test]
fn test_role_debug_admin() {
    let s = format!("{:?}", Role::Admin);
    assert!(s.contains("Admin"));
}

#[test]
fn test_role_debug_superuser() {
    let s = format!("{:?}", Role::Superuser);
    assert!(s.contains("Superuser"));
}

// ═══ Role clone / eq ═══

#[test]
fn test_role_clone() {
    let role = Role::Admin;
    let cloned = role.clone();
    assert_eq!(role.as_str(), cloned.as_str());
}

#[test]
fn test_role_eq() {
    assert_eq!(Role::Admin, Role::Admin);
    assert_ne!(Role::Admin, Role::User);
}

// ═══ Role serialization ═══

#[test]
fn test_role_serialize_user() {
    let json = serde_json::to_string(&Role::User).expect("serialize");
    assert_eq!(json, "\"user\"");
}

#[test]
fn test_role_serialize_admin() {
    let json = serde_json::to_string(&Role::Admin).expect("serialize");
    assert_eq!(json, "\"admin\"");
}

#[test]
fn test_role_serialize_superuser() {
    let json = serde_json::to_string(&Role::Superuser).expect("serialize");
    assert_eq!(json, "\"superuser\"");
}

#[test]
fn test_role_deserialize() {
    let role: Role = serde_json::from_str("\"admin\"").expect("deserialize");
    assert_eq!(role, Role::Admin);
}
