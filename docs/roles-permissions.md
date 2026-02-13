# Roles & Permissions

**Last Updated:** February 2026

## Role Enum

Chopin defines four built-in roles with increasing privilege levels:

```rust
pub enum Role {
    User       = 0,   // Default role for new signups
    Moderator  = 1,
    Admin      = 2,
    SuperAdmin = 3,
}
```

Roles are stored as integers in the database and auto-converted.

## Creating Users with Roles

New users created via `/api/auth/signup` default to `Role::User`.

To create an admin user:

```bash
chopin createsuperuser
# Interactive prompts for email, username, password
```

## Protecting Endpoints

### AuthUserWithRole Extractor

Restrict a handler to a minimum role level:

```rust
use chopin_core::extractors::AuthUserWithRole;
use chopin_core::models::user::Role;

// Only Admin (2) and SuperAdmin (3) can access
async fn admin_dashboard(
    user: AuthUserWithRole<{ Role::Admin as u8 }>,
) -> ApiResponse<String> {
    ApiResponse::success(format!("Welcome admin, user_id={}", user.user_id))
}
```

### require_role Middleware

Apply role checking to a group of routes:

```rust
use axum::{Router, routing::get, middleware};
use chopin_core::extractors::require_role;
use chopin_core::models::user::Role;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/users", get(list_all_users))
        .route("/api/admin/stats", get(admin_stats))
        .layer(middleware::from_fn(require_role(Role::Admin)))
}
```

### Manual Role Checking

```rust
use chopin_core::extractors::AuthUser;
use chopin_core::models::user::Role;
use chopin_core::ChopinError;

async fn flexible_handler(user: AuthUser) -> Result<ApiResponse<String>, ChopinError> {
    match user.role {
        Role::SuperAdmin => {
            // Full access
            Ok(ApiResponse::success("Super admin view".into()))
        }
        Role::Admin => {
            // Admin access
            Ok(ApiResponse::success("Admin view".into()))
        }
        _ => {
            Err(ChopinError::Forbidden("Admin access required".into()))
        }
    }
}
```

## Role Hierarchy

The `require_role` middleware uses `>=` comparison, so:

- `require_role(Role::User)` → allows User, Moderator, Admin, SuperAdmin
- `require_role(Role::Moderator)` → allows Moderator, Admin, SuperAdmin
- `require_role(Role::Admin)` → allows Admin, SuperAdmin
- `require_role(Role::SuperAdmin)` → allows only SuperAdmin

## Updating User Roles

```rust
use sea_orm::{ActiveModelTrait, Set, IntoActiveModel};
use chopin_core::models::user;

// Find and update
let mut user_model: user::ActiveModel = existing_user.into_active_model();
user_model.role = Set(user::Role::Admin);
user_model.update(&state.db).await?;
```

## JWT Claims

The role is included in the JWT token:

```json
{
  "sub": "1",
  "role": "admin",
  "exp": 1707696000,
  "iat": 1707609600
}
```

The `AuthUser` extractor parses this automatically:

```rust
pub struct AuthUser {
    pub user_id: i32,
    pub role: Role,
}
```
