use chopin_core::models::user::{Model, Role, UserResponse};

#[test]
fn test_role_as_str() {
    assert_eq!(Role::User.as_str(), "user");
    assert_eq!(Role::Admin.as_str(), "admin");
    assert_eq!(Role::Superuser.as_str(), "superuser");
}

#[test]
fn test_role_from_str() {
    use std::str::FromStr;

    assert_eq!(Role::from_str("user").unwrap(), Role::User);
    assert_eq!(Role::from_str("admin").unwrap(), Role::Admin);
    assert_eq!(Role::from_str("superuser").unwrap(), Role::Superuser);

    // Unknown roles default to User
    assert_eq!(Role::from_str("unknown").unwrap(), Role::User);
    assert_eq!(Role::from_str("").unwrap(), Role::User);
    assert_eq!(Role::from_str("moderator").unwrap(), Role::User);
}

#[test]
fn test_role_permissions_user() {
    let user = Role::User;

    assert!(user.has_permission(&Role::User));
    assert!(!user.has_permission(&Role::Admin));
    assert!(!user.has_permission(&Role::Superuser));
}

#[test]
fn test_role_permissions_admin() {
    let admin = Role::Admin;

    assert!(admin.has_permission(&Role::User));
    assert!(admin.has_permission(&Role::Admin));
    assert!(!admin.has_permission(&Role::Superuser));
}

#[test]
fn test_role_permissions_superuser() {
    let superuser = Role::Superuser;

    assert!(superuser.has_permission(&Role::User));
    assert!(superuser.has_permission(&Role::Admin));
    assert!(superuser.has_permission(&Role::Superuser));
}

#[test]
fn test_role_hierarchy() {
    let user = Role::User;
    let admin = Role::Admin;
    let superuser = Role::Superuser;

    // Superuser > Admin > User
    assert!(superuser.has_permission(&admin));
    assert!(superuser.has_permission(&user));
    assert!(admin.has_permission(&user));

    assert!(!admin.has_permission(&superuser));
    assert!(!user.has_permission(&admin));
    assert!(!user.has_permission(&superuser));
}

#[test]
fn test_role_serialization() {
    let roles = vec![
        (Role::User, "\"user\""),
        (Role::Admin, "\"admin\""),
        (Role::Superuser, "\"superuser\""),
    ];

    for (role, expected) in roles {
        let json = serde_json::to_string(&role).expect("Failed to serialize");
        assert_eq!(json, expected);

        let deserialized: Role = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized, role);
    }
}

#[test]
fn test_role_clone_and_equality() {
    let admin1 = Role::Admin;
    let admin2 = admin1.clone();

    assert_eq!(admin1, admin2);
    assert_eq!(admin1, Role::Admin);
    assert_ne!(admin1, Role::User);
}

#[test]
fn test_user_model_creation() {
    let now = chrono::Utc::now().naive_utc();

    let user = Model {
        id: 1,
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$...".to_string(),
        role: "user".to_string(),
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    assert_eq!(user.id, 1);
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.username, "testuser");
    assert!(user.is_active);
}

#[test]
fn test_user_response_conversion() {
    let now = chrono::Utc::now().naive_utc();

    let user = Model {
        id: 42,
        email: "user@example.com".to_string(),
        username: "username".to_string(),
        password_hash: "secret_hash".to_string(),
        role: "admin".to_string(),
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    let response: UserResponse = user.into();

    assert_eq!(response.id, 42);
    assert_eq!(response.email, "user@example.com");
    assert_eq!(response.username, "username");
    assert_eq!(response.role, "admin");
    assert!(response.is_active);
    assert_eq!(response.created_at, now);
}

#[test]
fn test_user_response_excludes_password() {
    let now = chrono::Utc::now().naive_utc();

    let user = Model {
        id: 1,
        email: "test@example.com".to_string(),
        username: "test".to_string(),
        password_hash: "secret_password_hash".to_string(),
        role: "user".to_string(),
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    let response: UserResponse = user.into();
    let json = serde_json::to_string(&response).expect("Failed to serialize");

    // Password should not be in JSON
    assert!(!json.contains("password"));
    assert!(!json.contains("secret_password_hash"));

    // Other fields should be present
    assert!(json.contains("test@example.com"));
    assert!(json.contains("test"));
}

#[test]
fn test_user_model_serialization() {
    let now = chrono::Utc::now().naive_utc();

    let user = Model {
        id: 1,
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
        password_hash: "secret_hash".to_string(),
        role: "user".to_string(),
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    let json = serde_json::to_string(&user).expect("Failed to serialize");

    // password_hash is marked with #[serde(skip_serializing)]
    assert!(!json.contains("password_hash"));
    assert!(!json.contains("secret_hash"));

    // Other fields should be present
    assert!(json.contains("test@example.com"));
    assert!(json.contains("testuser"));
}

#[test]
fn test_user_response_serialization() {
    let now = chrono::Utc::now().naive_utc();

    let response = UserResponse {
        id: 1,
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
        role: "admin".to_string(),
        is_active: false,
        created_at: now,
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");

    assert!(json.contains("\"id\":1"));
    assert!(json.contains("\"email\":\"test@example.com\""));
    assert!(json.contains("\"username\":\"testuser\""));
    assert!(json.contains("\"role\":\"admin\""));
    assert!(json.contains("\"is_active\":false"));
}

#[test]
fn test_different_user_roles() {
    let now = chrono::Utc::now().naive_utc();

    let roles = vec!["user", "admin", "superuser"];

    for role_str in roles {
        let user = Model {
            id: 1,
            email: "test@example.com".to_string(),
            username: "test".to_string(),
            password_hash: "hash".to_string(),
            role: role_str.to_string(),
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        let response: UserResponse = user.into();
        assert_eq!(response.role, role_str);
    }
}

#[test]
fn test_inactive_user() {
    let now = chrono::Utc::now().naive_utc();

    let user = Model {
        id: 1,
        email: "inactive@example.com".to_string(),
        username: "inactive".to_string(),
        password_hash: "hash".to_string(),
        role: "user".to_string(),
        is_active: false,
        created_at: now,
        updated_at: now,
    };

    assert!(!user.is_active);

    let response: UserResponse = user.into();
    assert!(!response.is_active);
}

#[test]
fn test_role_debug_output() {
    let user = Role::User;
    let admin = Role::Admin;
    let superuser = Role::Superuser;

    let user_debug = format!("{:?}", user);
    let admin_debug = format!("{:?}", admin);
    let superuser_debug = format!("{:?}", superuser);

    assert!(user_debug.contains("User"));
    assert!(admin_debug.contains("Admin"));
    assert!(superuser_debug.contains("Superuser"));
}
