# Roles & Permissions Guide

Chopin includes a built-in role-based access control (RBAC) system with three default roles: User, Admin, and Superuser.

## Quick Start

```rust
use chopin_core::extractors::{AuthUserWithRole, require_role};
use chopin_core::models::user::Role;

// Protect a route with role requirement
#[utoipa::path(
    get,
    path = "/api/admin/users",
    responses(
        (status = 200, description = "List all users"),
        (status = 403, description = "Insufficient permissions"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_users(
    AuthUserWithRole(user_id, role): AuthUserWithRole,
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<UserResponse>>, ChopinError> {
    // Check role manually
    if !role.has_permission(&Role::Admin) {
        return Err(ChopinError::Forbidden("Admin access required".into()));
    }
    
    // Your handler logic
    let users = User::find().all(&state.db).await?;
    Ok(ApiResponse::success(users.into_iter().map(UserResponse::from).collect()))
}
```

## Role Hierarchy

Chopin uses a hierarchical role system where higher roles inherit permissions from lower roles:

| Role | Level | Description | Use Case |
|------|-------|-------------|----------|
| **User** | 0 | Default role | Regular users, basic access |
| **Admin** | 1 | Administrative access | Content moderators, managers |
| **Superuser** | 2 | Full system access | System administrators |

Permission checks use `>=` comparison, so:
- **Superuser** can access Admin and User endpoints
- **Admin** can access User endpoints
- **User** can only access User endpoints

## Creating Users with Roles

### Via Signup (Default: User)

The signup endpoint automatically assigns the "user" role:

```rust
// In controllers/auth.rs
let new_user = user::ActiveModel {
    email: Set(payload.email),
    username: Set(payload.username),
    password_hash: Set(password_hash),
    role: Set("user".to_string()),  // Default role
    is_active: Set(true),
    created_at: Set(now),
    updated_at: Set(now),
    ..Default::default()
};
```

### Via CLI (Superuser)

Create admin accounts via CLI:

```bash
chopin createsuperuser
```

Interactive prompts:

```
🎹 Creating superuser account...

  Email: admin@example.com
  Username: admin
  Password: ********
  Confirm password: ********

  ✓ Superuser 'admin' created successfully!
```

### Programmatically

```rust
use chopin_core::auth::hash_password;
use chopin_core::models::user;

async fn create_admin(
    db: &DatabaseConnection,
    email: &str,
    username: &str,
    password: &str,
) -> Result<user::Model, ChopinError> {
    let password_hash = hash_password(password)?;
    let now = chrono::Utc::now().naive_utc();
    
    let admin = user::ActiveModel {
        email: Set(email.to_string()),
        username: Set(username.to_string()),
        password_hash: Set(password_hash),
        role: Set("admin".to_string()),  // or "superuser"
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    
    Ok(admin.insert(db).await?)
}
```

## Protecting Routes

### Method 1: Extractor

Use `AuthUserWithRole` extractor in your handler:

```rust
use chopin_core::extractors::AuthUserWithRole;
use chopin_core::models::user::Role;

async fn admin_dashboard(
    AuthUserWithRole(user_id, role): AuthUserWithRole,
) -> Result<ApiResponse<DashboardData>, ChopinError> {
    if !role.has_permission(&Role::Admin) {
        return Err(ChopinError::Forbidden("Admin access required".into()));
    }
    
    // Admin logic here
    Ok(ApiResponse::success(DashboardData { ... }))
}
```

### Method 2: Middleware

Apply role requirement to entire route groups:

```rust
use axum::middleware;
use chopin_core::extractors::require_role;
use chopin_core::models::user::Role;

pub fn routes() -> Router<AppState> {
    // Public routes
    let public = Router::new()
        .route("/posts", get(list_posts));
    
    // Admin-only routes
    let admin_routes = Router::new()
        .route("/users", get(list_users))
        .route("/users/:id", delete(delete_user))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            require_role(Role::Admin),
        ));
    
    // Combine routes
    public.merge(admin_routes)
}
```

### Method 3: Manual Check

Check role within your handler:

```rust
async fn update_user(
    AuthUser(current_user_id): AuthUser,
    Path(target_user_id): Path<i32>,
    State(state): State<AppState>,
) -> Result<ApiResponse<UserResponse>, ChopinError> {
    // Get current user's role
    let current_user = User::find_by_id(current_user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::Unauthorized("User not found".into()))?;
    
    let role = Role::from_str(&current_user.role);
    
    // Users can only edit themselves, admins can edit anyone
    if target_user_id != current_user_id && !role.has_permission(&Role::Admin) {
        return Err(ChopinError::Forbidden("Cannot edit other users".into()));
    }
    
    // Update logic...
}
```

## Role API

### `Role` Enum

```rust
pub enum Role {
    User,       // "user"
    Admin,      // "admin"
    Superuser,  // "superuser"
}
```

### `Role::from_str`

Convert string to role:

```rust
let role = Role::from_str("admin");  // Role::Admin
let role = Role::from_str("user");   // Role::User
let role = Role::from_str("invalid"); // Role::User (default)
```

### `Role::as_str`

Convert role to string:

```rust
let role_str = Role::Admin.as_str();  // "admin"
```

### `Role::has_permission`

Check if role has required permission level:

```rust
let user_role = Role::User;
let admin_role = Role::Admin;
let super_role = Role::Superuser;

// User cannot access admin features
assert!(!user_role.has_permission(&Role::Admin));

// Admin can access admin features
assert!(admin_role.has_permission(&Role::Admin));

// Superuser can access everything
assert!(super_role.has_permission(&Role::User));
assert!(super_role.has_permission(&Role::Admin));
assert!(super_role.has_permission(&Role::Superuser));
```

## Common Patterns

### Resource Ownership

Users can edit their own resources, admins can edit any:

```rust
async fn update_profile(
    AuthUserWithRole(current_user_id, role): AuthUserWithRole,
    Path(profile_id): Path<i32>,
    State(state): State<AppState>,
    Json(data): Json<UpdateProfileRequest>,
) -> Result<ApiResponse<Profile>, ChopinError> {
    let profile = Profile::find_by_id(profile_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Profile not found".into()))?;
    
    // Check ownership or admin status
    if profile.user_id != current_user_id && !role.has_permission(&Role::Admin) {
        return Err(ChopinError::Forbidden("Access denied".into()));
    }
    
    // Update profile...
}
```

### Soft Permissions

Grant specific permissions within role boundaries:

```rust
#[derive(Clone)]
pub struct UserPermissions {
    pub can_publish: bool,
    pub can_moderate: bool,
    pub can_delete: bool,
}

impl UserPermissions {
    pub fn from_role(role: &Role) -> Self {
        match role {
            Role::User => UserPermissions {
                can_publish: false,
                can_moderate: false,
                can_delete: false,
            },
            Role::Admin => UserPermissions {
                can_publish: true,
                can_moderate: true,
                can_delete: true,
            },
            Role::Superuser => UserPermissions {
                can_publish: true,
                can_moderate: true,
                can_delete: true,
            },
        }
    }
}
```

### Role Upgrade

Admins can promote users:

```rust
async fn promote_user(
    AuthUserWithRole(_, role): AuthUserWithRole,
    Path(user_id): Path<i32>,
    State(state): State<AppState>,
    Json(req): Json<PromoteRequest>,
) -> Result<ApiResponse<UserResponse>, ChopinError> {
    // Only superusers can promote
    if !role.has_permission(&Role::Superuser) {
        return Err(ChopinError::Forbidden("Superuser access required".into()));
    }
    
    // Validate target role
    let target_role = Role::from_str(&req.role);
    
    // Update user
    let user = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".into()))?;
    
    let mut active: user::ActiveModel = user.into();
    active.role = Set(target_role.as_str().to_string());
    let updated = active.update(&state.db).await?;
    
    Ok(ApiResponse::success(UserResponse::from(updated)))
}
```

## OpenAPI Documentation

Document role requirements in API docs:

```rust
#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}",
    params(
        ("id" = i32, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User deleted"),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
async fn delete_user(
    AuthUserWithRole(_, role): AuthUserWithRole,
    Path(id): Path<i32>,
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, ChopinError> {
    if !role.has_permission(&Role::Admin) {
        return Err(ChopinError::Forbidden("Admin access required".into()));
    }
    
    User::delete_by_id(id).exec(&state.db).await?;
    Ok(ApiResponse::success(()))
}
```

## Testing Role-Based Access

```rust
#[tokio::test]
async fn test_admin_access() {
    let app = TestApp::new().await;
    
    // Create regular user
    let (user_token, _) = app.create_user("user@test.com", "user", "password123").await;
    
    // Create admin (manually)
    let admin = create_admin(&app.db, "admin@test.com", "admin", "admin123").await.unwrap();
    let admin_token = create_token(admin.id, &app.config.jwt_secret, 24).unwrap();
    
    // Regular user cannot access admin endpoint
    let res = app.client.get_with_auth(
        &app.url("/api/admin/users"),
        &user_token
    ).await;
    assert_eq!(res.status, 403);
    
    // Admin can access admin endpoint
    let res = app.client.get_with_auth(
        &app.url("/api/admin/users"),
        &admin_token
    ).await;
    assert_eq!(res.status, 200);
}
```

## Best Practices

### 1. Use Middleware for Route Groups

Apply role checks at the router level for consistency:

```rust
Router::new()
    .nest("/admin", admin_routes())
    .layer(middleware::from_fn_with_state(state, require_role(Role::Admin)))
```

### 2. Fail Securely

Default to denying access:

```rust
// ✓ Good - explicit check
if !role.has_permission(&Role::Admin) {
    return Err(ChopinError::Forbidden("Access denied".into()));
}

// ✗ Bad - implicit allow
if role.has_permission(&Role::Admin) {
    // ... admin logic
}
// Falls through without error!
```

### 3. Log Access Attempts

Track privileged operations:

```rust
tracing::warn!(
    "Unauthorized admin access attempt by user {} to {}",
    user_id,
    request_path
);
```

### 4. Document Role Requirements

Always document which roles can access endpoints:

```rust
/// Delete a user (Admin only)
#[utoipa::path(security(("bearer_auth" = [])))]
async fn delete_user(...) { }
```

### 5. Consider Granular Permissions

For complex applications, extend beyond three roles:

```rust
pub enum Permission {
    ReadPosts,
    WritePosts,
    DeletePosts,
    ManageUsers,
    ViewAnalytics,
}

impl Role {
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            Role::User => vec![Permission::ReadPosts, Permission::WritePosts],
            Role::Admin => vec![/* all permissions */],
            Role::Superuser => vec![/* all permissions */],
        }
    }
}
```

## Security Considerations

- **Always validate on the server** - Never trust client-side role checks
- **Use HTTPS in production** - Protect JWT tokens in transit
- **Rotate JWT secrets** - Periodically change JWT_SECRET
- **Audit privileged actions** - Log admin/superuser operations
- **Principle of least privilege** - Grant minimum required role
- **Test authorization** - Write tests for each protected endpoint
