# Modular Architecture Guide

> **Django's feature-first philosophy meets Rust's type safety**

## Overview

Chopin uses a **Static Modular Composition** model inspired by Django's app architecture. Every feature (Blog, Auth, Billing) is a self-contained `ChopinModule` that explicitly declares its routes, services, and dependencies.

## Core Concepts

### 1. The ChopinModule Trait

Every module implements the `ChopinModule` trait:

```rust
pub trait ChopinModule: Send + Sync {
    /// Module name (e.g., "blog", "auth")
    fn name(&self) -> &str;
    
    /// Register routes with the application
    fn routes(&self) -> Router<AppState>;
    
    /// Optional: Register services/state
    fn services(&self) -> Option<Box<dyn Any + Send + Sync>> {
        None
    }
    
    /// Optional: Run migrations on startup
    async fn migrate(&self, db: &DatabaseConnection) -> Result<(), ChopinError> {
        Ok(())
    }
    
    /// Optional: Health check for this module
    async fn health_check(&self) -> Result<(), ChopinError> {
        Ok(())
    }
}
```

### 2. Hub-and-Spoke Architecture

```
┌─────────────────────────────────────────┐
│         Your Application                 │
│  ┌────────┐  ┌────────┐  ┌────────┐    │
│  │ Blog   │  │ Auth   │  │ Billing│    │
│  │ Module │  │ Module │  │ Module │    │
│  └────┬───┘  └────┬───┘  └────┬───┘    │
│       │           │           │         │
│       └───────────┴───────────┘         │
│                   │                      │
│                   ▼                      │
│        ┌──────────────────┐             │
│        │   chopin-core    │             │
│        │   (The Hub)      │             │
│        └──────────────────┘             │
└─────────────────────────────────────────┘
```

**Key Benefits:**
- **No Circular Dependencies** - Modules only depend on `chopin-core`, never on each other
- **Compile-Time Safety** - Rust's type system enforces module contracts
- **Clear Boundaries** - Each module owns its domain completely

### 3. MVSR Pattern

**Model-View-Service-Router** separates concerns:

```
blog/
├── mod.rs        # Module entry point (implements ChopinModule)
├── models.rs     # M: Database entities (SeaORM)
├── services.rs   # S: Business logic (pure Rust functions)
├── handlers.rs   # V: HTTP handlers (Axum extractors/responses)
├── routes.rs     # R: Route configuration (paths → handlers)
└── dto.rs        # Data Transfer Objects (API contracts)
```

**Why MVSR?**
- **100% Unit-Testable** - Services are pure functions, no HTTP mocking needed
- **Clear Separation** - HTTP concerns stay in handlers, logic in services
- **Reusability** - Services can be called from CLI, jobs, tests, etc.

## Creating a Module

### Step 1: Define the Module Structure

```
apps/blog/
├── mod.rs           # Implements ChopinModule
├── models.rs        # Post, Comment, Category entities
├── services.rs      # get_posts(), create_post(), etc.
├── handlers.rs      # list_posts, create_post HTTP handlers
├── routes.rs        # Route definitions
└── dto.rs           # PostDto, CreatePostRequest
```

### Step 2: Implement Services (Pure Business Logic)

**apps/blog/services.rs:**

```rust
use chopin_core::prelude::*;
use super::models::Post;

/// Get all posts for a specific tenant with pagination.
/// 
/// This is pure business logic - no HTTP concerns!
pub async fn get_tenant_posts(
    db: &DatabaseConnection,
    tenant_id: i32,
    page: u64,
    per_page: u64,
) -> Result<Vec<Post>, ChopinError> {
    Post::find()
        .filter(post::Column::TenantId.eq(tenant_id))
        .filter(post::Column::Published.eq(true))
        .order_by_desc(post::Column::CreatedAt)
        .paginate(db, per_page)
        .fetch_page(page)
        .await
        .map_err(Into::into)
}

/// Create a new post for a tenant.
pub async fn create_post(
    db: &DatabaseConnection,
    tenant_id: i32,
    user_id: i32,
    title: String,
    content: String,
) -> Result<Post, ChopinError> {
    let post = post::ActiveModel {
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        title: Set(title),
        content: Set(content),
        published: Set(false),
        ..Default::default()
    };
    
    post.insert(db)
        .await
        .map_err(Into::into)
}
```

**Testing Services:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_get_tenant_posts() {
        let db = test_db().await;
        
        // Create test data
        seed_posts(&db, 1, 10).await;
        seed_posts(&db, 2, 5).await;
        
        // Tenant 1 should see 10 posts
        let posts = get_tenant_posts(&db, 1, 0, 20).await.unwrap();
        assert_eq!(posts.len(), 10);
        
        // Tenant 2 should only see their 5 posts
        let posts = get_tenant_posts(&db, 2, 0, 20).await.unwrap();
        assert_eq!(posts.len(), 5);
    }
}
```

### Step 3: Create HTTP Handlers (Thin HTTP Layer)

**apps/blog/handlers.rs:**

```rust
use chopin_core::prelude::*;
use super::{services, dto::*};

/// List all posts with pagination.
pub async fn list_posts(
    State(state): State<AppState>,
    Pagination { page, per_page }: Pagination,
) -> Result<ApiResponse<Vec<PostDto>>, ChopinError> {
    // Call the service - no HTTP concerns here
    let posts = services::get_posts(
        &state.db,
        page,
        per_page,
    ).await?;
    
    // Convert to DTOs
    let dtos = posts.into_iter().map(PostDto::from).collect();
    
    Ok(ApiResponse::success(dtos))
}

/// Create a new post.
pub async fn create_post(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,  // Requires authentication
    Json(req): Json<CreatePostRequest>,
) -> Result<ApiResponse<PostDto>, ChopinError> {
    // Validate input
    req.validate()?;
    
    // Call service
    let post = services::create_post(
        &state.db,
        tenant.id,
        user.id,
        req.title,
        req.content,
    ).await?;
    
    Ok(ApiResponse::created(PostDto::from(post)))
}
```

### Step 4: Define Routes

**apps/blog/routes.rs:**

```rust
use chopin_core::prelude::*;
use super::handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/posts", 
            get(handlers::list_posts)
                .post(handlers::create_post)
        )
        .route("/posts/:id", 
            get(handlers::get_post)
                .put(handlers::update_post)
                .delete(handlers::delete_post)
        )
        .route("/posts/:id/publish", 
            post(handlers::publish_post)
        )
}
```

### Step 5: Implement the Module

**apps/blog/mod.rs:**

```rust
use chopin_core::prelude::*;

mod models;
mod services;
mod handlers;
mod routes;
mod dto;

pub struct BlogModule;

impl ChopinModule for BlogModule {
    fn name(&self) -> &str {
        "blog"
    }
    
    fn routes(&self) -> Router<AppState> {
        routes::routes()
    }
    
    async fn migrate(&self, db: &DatabaseConnection) -> Result<(), ChopinError> {
        // Run blog-specific migrations
        migrations::Migrator::up(db, None).await?;
        Ok(())
    }
    
    async fn health_check(&self) -> Result<(), ChopinError> {
        // Optional: Check blog-specific dependencies
        Ok(())
    }
}

impl BlogModule {
    pub fn new() -> Self {
        Self
    }
}
```

### Step 6: Mount the Module

**src/main.rs:**

```rust
use chopin_core::prelude::*;

mod apps {
    pub mod blog;
    pub mod billing;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    App::new().await?
        .mount_module(apps::blog::BlogModule::new())
        .mount_module(apps::billing::BillingModule::new())
        .mount_module(AuthModule::new())  // vendor/chopin_auth
        .run().await?;
    
    Ok(())
}
```

## Module Communication

### Anti-Pattern: Direct Module Dependencies

```rust
// ❌ DON'T DO THIS
// blog/services.rs
use crate::billing::services::check_subscription;  // Circular dependency risk!
```

### Pattern 1: Shared Types via Hub

```rust
// chopin-core/src/shared/subscription.rs
pub trait SubscriptionChecker: Send + Sync {
    async fn has_feature(&self, tenant_id: i32, feature: &str) -> Result<bool, ChopinError>;
}

// billing/mod.rs
impl SubscriptionChecker for BillingModule {
    async fn has_feature(&self, tenant_id: i32, feature: &str) -> Result<bool, ChopinError> {
        // Implementation
    }
}

// Register in AppState
pub struct AppState {
    pub db: DatabaseConnection,
    pub subscription_checker: Arc<dyn SubscriptionChecker>,
}
```

### Pattern 2: Events/Messaging

```rust
// chopin-core/src/events.rs
pub enum AppEvent {
    PostPublished { tenant_id: i32, post_id: i32 },
    UserSignedUp { tenant_id: i32, user_id: i32 },
}

// blog/handlers.rs
pub async fn publish_post(
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, ChopinError> {
    // ... publish post logic ...
    
    // Emit event
    state.events.emit(AppEvent::PostPublished {
        tenant_id: post.tenant_id,
        post_id: post.id,
    });
    
    Ok(ApiResponse::success(()))
}
```

## Testing Modules

### Unit Tests (Services)

```rust
// apps/blog/services.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_post() {
        let db = setup_test_db().await;
        
        let post = create_post(
            &db,
            1,  // tenant_id
            1,  // user_id
            "Test Post".into(),
            "Content".into(),
        ).await.unwrap();
        
        assert_eq!(post.title, "Test Post");
        assert_eq!(post.tenant_id, 1);
    }
}
```

### Integration Tests (Full Module)

```rust
// apps/blog/tests/integration.rs
use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_list_posts_endpoint() {
    let app = TestApp::new().await.unwrap();
    
    // Seed data
    seed_posts(&app.db, 1, 10).await;
    
    // Test the endpoint
    let response = app
        .get("/blog/posts")
        .header("X-Tenant-Id", "1")
        .send()
        .await;
    
    assert_eq!(response.status(), 200);
    let posts: Vec<PostDto> = response.json().await.unwrap();
    assert_eq!(posts.len(), 10);
}
```

## Best Practices

### 1. Keep Modules Focused

Each module should have a single, well-defined purpose:

- ✅ **Blog Module** - Posts, comments, categories
- ✅ **Auth Module** - Authentication, authorization
- ✅ **Billing Module** - Subscriptions, payments
- ❌ **Core Module** - Everything (too broad)

### 2. Services First, Handlers Second

Write business logic as services first:

1. Design service signatures
2. Implement pure business logic
3. Write comprehensive unit tests
4. Create thin HTTP handlers that call services

### 3. Use DTOs for API Contracts

```rust
// dto.rs
#[derive(Serialize, Deserialize, ToSchema)]
pub struct PostDto {
    pub id: i32,
    pub title: String,
    pub excerpt: String,
    pub author_name: String,
    pub published_at: Option<DateTime<Utc>>,
}

impl From<Post> for PostDto {
    fn from(post: Post) -> Self {
        Self {
            id: post.id,
            title: post.title,
            excerpt: truncate(&post.content, 200),
            // ... map fields, hide sensitive data
        }
    }
}
```

### 4. Leverage Type-Safe Extractors

```rust
// Always use typed extractors
pub async fn handler(
    AuthUser(user): AuthUser,              // Type-safe auth
    Pagination { page, .. }: Pagination,   // Type-safe pagination
    Json(data): Json<CreateRequest>,       // Type-safe body
) -> Result<ApiResponse<Response>, ChopinError> {
    // All inputs are validated and typed!
}
```

### 5. Document Module Dependencies

```rust
/// Blog module for managing posts and comments.
/// 
/// # Dependencies
/// - `chopin-core` for framework primitives
/// - SeaORM for database access
/// - Optional: `BillingModule` for feature gating (via AppState)
/// 
/// # Routes
/// - `GET /posts` - List posts
/// - `POST /posts` - Create post (requires auth)
/// - `GET /posts/:id` - Get single post
/// - `PUT /posts/:id` - Update post (requires auth)
/// - `DELETE /posts/:id` - Delete post (requires auth)
pub struct BlogModule;
```

## Comparison with Django

| Aspect | Django | Chopin |
|--------|--------|--------|
| **Structure** | apps/ with models, views, urls | apps/ with models, services, handlers, routes |
| **Registration** | `INSTALLED_APPS` (runtime list) | `mount_module()` (compile-time trait) |
| **Verification** | Runtime errors if misconfigured | Compile-time errors |
| **URL Resolution** | String-based with runtime checks | Type-safe Router |
| **Dependencies** | Import directly (circular risk) | Hub-and-spoke (no circular deps) |
| **Testing** | Django TestCase (heavy) | Pure Rust unit tests (lightweight) |
| **Reusability** | Via PyPI packages | Via Cargo crates |

## Next Steps

- See [ARCHITECTURE.md](../ARCHITECTURE.md) for system design details
- Check `chopin-examples/basic-api/` for a complete MVSR example
- Read [JSON Performance](json-performance.md) for optimization techniques
- Read [PERFORMANCE_OPTIMIZATION.md](PERFORMANCE_OPTIMIZATION.md) for `FastRoute` — zero-alloc endpoints that complement your modules for ultra-high-throughput paths like `/health`, `/json`, and `/plaintext`
