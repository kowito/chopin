# Quick Start Guide

Get started with Chopin in 60 seconds.

## Installation

```bash
# Install the CLI
cargo install chopin-cli

# Create a new modular project
chopin new my-api
cd my-api

# Run in development mode
cargo run

# Run with maximum performance (SO_REUSEPORT multi-core)
REUSEPORT=true cargo run --release --features perf
```

**Your API is now serving 650K+ req/s 🚀**

## Your First Modular App (2 minutes)

### Step 1: Create a Blog Module

```bash
mkdir -p apps/blog
```

**apps/blog/mod.rs:**
```rust
use chopin_core::prelude::*;

mod handlers;
mod services;
mod models;

pub struct BlogModule;

impl ChopinModule for BlogModule {
    fn name(&self) -> &str { "blog" }
    
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/posts", get(handlers::list_posts).post(handlers::create_post))
            .route("/posts/:id", get(handlers::get_post))
    }
}

impl BlogModule {
    pub fn new() -> Self { Self }
}
```

**apps/blog/services.rs** (Pure business logic - 100% unit-testable):
```rust
use chopin_core::prelude::*;
use super::models::Post;

pub async fn get_tenant_posts(
    db: &DatabaseConnection,
    tenant_id: i32,
    page: u64,
) -> Result<Vec<Post>, ChopinError> {
    Post::find()
        .filter(post::Column::TenantId.eq(tenant_id))
        .paginate(db, 20)
        .fetch_page(page)
        .await
        .map_err(Into::into)
}
```

**apps/blog/handlers.rs** (HTTP layer - thin adapter):
```rust
use chopin_core::prelude::*;
use super::services;

pub async fn list_posts(
    State(state): State<AppState>,
    Pagination { page, per_page }: Pagination,
) -> Result<ApiResponse<Vec<PostDto>>, ChopinError> {
    let posts = services::get_posts(&state.db, page, per_page).await?;
    Ok(ApiResponse::success(posts))
}
```

### Step 2: Compose Your Application

**src/main.rs:**
```rust
use chopin_core::prelude::*;
mod apps;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    App::new().await?
        .mount_module(apps::blog::BlogModule::new())
        .mount_module(AuthModule::new())  // vendor/chopin_auth
        .run().await?;
    
    Ok(())
}
```

**That's it!** You now have:
- ✅ **Compile-time verification** - Missing routes or modules = compiler error
- ✅ **100% unit-testable** - Services are pure Rust functions
- ✅ **Zero circular dependencies** - Hub-and-spoke architecture
- ✅ **Feature-first folders** - Everything "Blog" lives in `apps/blog/`
- ✅ **Auto-generated OpenAPI docs** at `/api-docs`
- ✅ **Built-in auth endpoints** at `/api/auth/signup` and `/api/auth/login`
- ✅ **Database migrations** run automatically on startup
- ✅ **Request logging** with structured traces

## Authentication & RBAC

### Simple Login Required

```rust
use chopin_core::prelude::*;

#[login_required]
async fn get_profile() -> Result<Json<ApiResponse<UserProfile>>, ChopinError> {
    // __chopin_auth is auto-injected
    Ok(Json(ApiResponse::success(UserProfile {
        id: __chopin_auth.user_id.clone(),
        email: __chopin_auth.email.clone(),
    })))
}
```

### Permission-Based Access Control

```rust
#[permission_required("can_publish_post")]
async fn publish_post(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
) -> Result<Json<ApiResponse<PostResponse>>, ChopinError> {
    // Only users with "can_publish_post" permission can access
    let post = services::publish(&state.db, post_id, &__chopin_auth.user_id).await?;
    Ok(Json(ApiResponse::success(post)))
}
```

### Fine-Grained Permission Checks

```rust
#[login_required]
async fn get_report(
    guard: PermissionGuard,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ReportData>>, ChopinError> {
    guard.require("can_view_reports")?;
    
    let mut report = services::build_report(&state.db).await?;
    
    // Include sensitive data only if user has the permission
    if guard.has_permission("can_view_financials") {
        report.financial_data = Some(services::get_financials(&state.db).await?);
    }
    
    Ok(Json(ApiResponse::success(report)))
}
```

## Built-In Auth Endpoints

No code required:

```bash
# Sign up
curl -X POST http://localhost:3000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secret123","email":"alice@example.com"}'

# Login
curl -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secret123"}'

# Returns: {"access_token":"eyJ0eXAi...", "refresh_token":"abc123...", "csrf_token":"def456..."}

# Access protected endpoint
curl -X GET http://localhost:3000/api/profile \
  -H "Authorization: Bearer eyJ0eXAi..."

# Access permission-protected endpoint (with permission)
curl -X PUT http://localhost:3000/api/posts/42/publish \
  -H "Authorization: Bearer eyJ0eXAi..."
# Returns: {"data":{"id":42,"published":true},"success":true}

# Access permission-protected endpoint (without permission)
curl -X PUT http://localhost:3000/api/posts/42/publish \
  -H "Authorization: Bearer eyJ0eXAi..."
# Returns: {"data":null,"error":"Permission denied: can_publish_post","success":false}
```

## FastRoute: Zero-Allocation Endpoints

`FastRoute` bypasses Axum middleware entirely for maximum throughput on predictable endpoints. Registered at startup, all headers are pre-computed — **zero per-request cost**.

### Static routes (~35ns/req)

```rust
use chopin_core::{App, FastRoute};

App::new().await?
    // Static plaintext: clone body pointer + memcpy headers
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!").get_only())
    
    // Static JSON: pre-cached response
    .fast_route(FastRoute::json("/health", br#"{"status":"ok"}"#).get_only())

    // With CORS and Cache-Control decorators (pre-computed, zero per-request cost)
    .fast_route(
        FastRoute::json("/api/status", br#"{"status":"ok","version":"0.3.5"}"#)
            .cors()
            .cache_control("public, max-age=60")
            .get_only()
    )
    .run().await?;
```

### Dynamic routes (~100-150ns/req)

For endpoints that must serialize fresh JSON on every request (e.g., TechEmpower benchmark):

```rust
use chopin_core::{App, FastRoute};
use serde::Serialize;

#[derive(Serialize)]
struct Message { message: &'static str }

App::new().await?
    // Per-request serialization — thread-local buffer reuse + sonic-rs SIMD
    // TechEmpower benchmark compliant (no body caching)
    .fast_route(FastRoute::json_serialize("/json", || Message {
        message: "Hello, World!",
    }).get_only())
    .run().await?;
```

### Performance comparison

| Route Type | Latency | Best For |
|-----------|---------|----------|
| `FastRoute::json()` / `text()` | ~35ns | Health, version, static API |
| `FastRoute::json_serialize()` | ~100-150ns | TFB-compliant JSON, metrics |
| Axum Router | ~1-5µs | Auth, DB queries, business logic |

> FastRoute does **not** run middleware — no auth, no logging. Use Axum routes for those.

## Working with Databases

```rust
use chopin_core::{App, ApiResponse, get, database::DatabaseConnection};
use sea_orm::{EntityTrait, QueryFilter, ColumnTrait};

async fn list_posts(db: DatabaseConnection) -> ApiResponse<Vec<Post>> {
    let posts = Post::find()
        .filter(post::Column::Published.eq(true))
        .all(&db)
        .await?;
    
    ApiResponse::success(posts)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?
        .route("/posts", get(list_posts));
    
    app.run().await?;
    Ok(())
}
```

Configure via `.env`:
```bash
DATABASE_URL=sqlite://database.db?mode=rwc
# Or PostgreSQL: postgresql://user:pass@localhost/dbname
# Or MySQL: mysql://user:pass@localhost/dbname
```

## Building & Testing

### Build

```bash
cargo build                              # Debug build
cargo build --release                    # Release build
cargo build --release --features perf    # With mimalloc + SIMD JSON
```

### Test

```bash
cargo test                               # All tests
cargo test -p chopin-core                # Core library only
cargo test -p chopin-basic-api           # Example tests
cargo test --test auth_tests             # Specific test file
```

## Next Steps

- **[Read the Documentation](README.md)** — Comprehensive guides and tutorials
- **[View Examples](../chopin-examples/)** — Real-world project templates
- **[Check Performance Benchmarks](BENCHMARKS.md)** — See how Chopin compares
- **[Learn the Architecture](modular-architecture.md)** — Understand the design patterns
- **[Security Features](FEATURES.md)** — Explore production security options
