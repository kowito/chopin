# Chopin Framework — LLM Learning Guide

> This document is designed for AI assistants (ChatGPT, Claude, Copilot, etc.) to understand the Chopin framework completely. It contains everything needed to help users build applications with Chopin.
>
> **Last Updated:** February 2026

## What is Chopin?

Chopin is a **high-performance Rust web framework** built on Axum, SeaORM, and Tokio. It provides:

- Dual server modes: **Standard** (easy, full middleware) and **Performance** (raw hyper, zero-alloc hot path)
- Built-in JWT authentication with Argon2id password hashing
- Role-based access control (User, Moderator, Admin, SuperAdmin)
- SeaORM database ORM with auto-migrations
- OpenAPI 3.1 documentation with Scalar UI
- In-memory and Redis caching
- Local and S3 file uploads
- Optional GraphQL via async-graphql
- CLI for project scaffolding and code generation
- Integration testing utilities

## Technology Stack

| Component | Library | Version |
|-----------|---------|---------|
| HTTP server | Axum | 0.8 |
| Raw HTTP | Hyper | 1.x |
| Async runtime | Tokio | 1.x |
| ORM | SeaORM | 1.x |
| JSON | sonic-rs / serde_json | 0.5 / 1.x (via crate::json) |
| Auth | jsonwebtoken + argon2 | 9 / 0.5 |
| API docs | utoipa + utoipa-scalar | 5 / 0.3 |
| Caching | DashMap / Redis | — / 0.27 |
| Storage | Local / AWS S3 | — / 1.x |
| GraphQL | async-graphql | 7.x (optional) |
| Allocator | mimalloc | 0.1 (perf feature) |
| Socket mult. | socket2 | 0.5 (SO_REUSEPORT) |

## Project Structure

When a user creates a Chopin project, it follows this structure:

```
my-app/
├── Cargo.toml
├── .env
├── .cargo/
│   └── config.toml          # CPU-specific rustflags
├── src/
│   ├── main.rs              # Entry point
│   ├── controllers/
│   │   ├── mod.rs           # Controller module exports
│   │   └── posts.rs         # Example controller
│   ├── models/
│   │   ├── mod.rs           # Model module exports
│   │   └── post.rs          # SeaORM entity
│   └── migrations/
│       ├── mod.rs           # Migrator
│       └── m20250211_*.rs   # Migration files
└── tests/
    └── integration_tests.rs
```

## Core Concepts

### 1. App Initialization

```rust
use chopin_core::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

`App::new()` does:
1. Loads config from `.env` + environment variables
2. Connects to the database
3. Runs pending migrations
4. Initializes cache (in-memory or Redis)

`App::run()` does:
1. Builds the Axum router
2. Checks `config.server_mode`:
   - **Standard** → `axum::serve(listener, router)`
   - **Performance** → `server::run_reuseport(addr, router, shutdown)` with SO_REUSEPORT × N cores

### 2. Server Modes

**Standard Mode** (default, `SERVER_MODE=standard`):
- Full Axum pipeline
- CORS middleware
- Tracing + request-id middleware (dev only)
- `axum::serve` with graceful shutdown
- Best for: development, typical production

**Performance Mode** (`SERVER_MODE=performance`):
- Raw hyper HTTP/1.1 service
- SO_REUSEPORT — N TCP listeners (one per CPU core)
- `/json` and `/plaintext` bypass Axum entirely → zero allocation
- Pre-computed static response bodies and headers
- Cached Date header (500ms refresh)
- TCP_NODELAY, backlog 8192
- HTTP/1.1 keep-alive + pipeline_flush
- Best for: benchmarks, extreme throughput

### 3. Configuration

All config comes from environment variables:

```rust
pub struct Config {
    pub server_mode: ServerMode,       // standard / performance
    pub database_url: String,          // sqlite://app.db?mode=rwc
    pub jwt_secret: String,            // HMAC-SHA256 signing key
    pub jwt_expiry_hours: u64,         // 24
    pub server_host: String,           // 127.0.0.1
    pub server_port: u16,              // 3000
    pub environment: String,           // development / production / test
    pub redis_url: Option<String>,
    pub upload_dir: String,            // ./uploads
    pub max_upload_size: u64,          // 10485760 (10MB)
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_public_url: Option<String>,
    pub s3_prefix: Option<String>,
}
```

### 4. AppState

Shared state available in all handlers:

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,     // SeaORM connection pool
    pub config: Arc<Config>,        // Shared config (Arc, not cloned)
    pub cache: CacheService,        // In-memory or Redis
}
```

### 5. Routing

Chopin auto-registers:
- `GET /` — Welcome JSON
- `POST /api/auth/signup` — User registration
- `POST /api/auth/login` — User login
- `GET /api-docs` — Scalar OpenAPI UI
- `GET /api-docs/openapi.json` — Raw OpenAPI spec

In performance mode, also:
- `GET /json` — `{"message":"Hello, World!"}` (zero-alloc)
- `GET /plaintext` — `Hello, World!` (zero-alloc)

User routes are added by merging into the router:

```rust
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts", get(list).post(create))
        .route("/api/posts/:id", get(show).put(update).delete(destroy))
}
```

### 6. Response Format

**ApiResponse\<T\>** — wraps any serializable data:

```rust
// Success (200)
ApiResponse::success(data)         // { "success": true, "data": {...} }
// Created (201)
ApiResponse::created(data)         // { "success": true, "data": {...} }
// Message only
ApiResponse::success_message("ok") // { "success": true, "message": "ok" }
```

**ChopinError** — maps to HTTP error codes:

```rust
ChopinError::NotFound(msg)        // 404
ChopinError::BadRequest(msg)      // 400
ChopinError::Unauthorized(msg)    // 401
ChopinError::Forbidden(msg)       // 403
ChopinError::Validation(errors)   // 422
ChopinError::Conflict(msg)        // 409
ChopinError::Internal(msg)        // 500
ChopinError::Database(err)        // 500 (from SeaORM DbErr)
```

### 7. Authentication

**Signup flow:**
1. Client POSTs `{email, username, password}` to `/api/auth/signup`
2. Password hashed with Argon2id
3. User row inserted (role = User)
4. JWT token created (HMAC-SHA256, includes user_id + role)
5. Returns `{access_token, user}`

**Login flow:**
1. Client POSTs `{email, password}` to `/api/auth/login`
2. User looked up by email
3. Password verified against Argon2id hash
4. JWT token created
5. Returns `{access_token, user}`

**Protecting endpoints:**
```rust
use chopin_core::extractors::AuthUser;

async fn protected(user: AuthUser) -> ApiResponse<String> {
    // user.user_id and user.role are available
    ApiResponse::success(format!("Hello user {}", user.user_id))
}
```

The `AuthUser` extractor:
1. Reads `Authorization: Bearer <token>` header
2. Validates JWT signature
3. Extracts `user_id` and `role` from claims
4. Returns 401 if missing/invalid

### 8. Roles

```rust
pub enum Role {
    User       = 0,
    Moderator  = 1,
    Admin      = 2,
    SuperAdmin = 3,
}
```

**AuthUserWithRole** — restrict to minimum role:
```rust
async fn admin_only(user: AuthUserWithRole<{ Role::Admin as u8 }>) -> ApiResponse<String> { ... }
```

**require_role middleware** — protect route groups:
```rust
router.layer(middleware::from_fn(require_role(Role::Admin)))
```

### 9. Models (SeaORM)

Entity definition:
```rust
#[derive(DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub title: String,
    pub body: String,
    pub created_at: chrono::NaiveDateTime,
}
```

CRUD operations:
```rust
// Create
let model = posts::ActiveModel { title: Set("Hello".into()), ..Default::default() };
let result = model.insert(&db).await?;

// Read
let post = posts::Entity::find_by_id(1).one(&db).await?;
let all = posts::Entity::find().all(&db).await?;

// Update
let mut active: posts::ActiveModel = post.into_active_model();
active.title = Set("Updated".into());
active.update(&db).await?;

// Delete
post.delete(&db).await?;
```

### 10. Extractors

| Extractor | Purpose | Usage |
|-----------|---------|-------|
| `Json<T>` | Deserialize JSON body (crate::json) | `Json(body): Json<CreateReq>` |
| `AuthUser` | JWT auth → user_id + role | `user: AuthUser` |
| `AuthUserWithRole<N>` | Auth with minimum role | `user: AuthUserWithRole<{Role::Admin as u8}>` |
| `Pagination` | Parse `?page=1&per_page=20` | `pagination: Pagination` |
| `State<AppState>` | Shared application state | `State(state): State<AppState>` |
| `Path<T>` | URL path parameters | `Path(id): Path<i32>` |
| `Query<T>` | Query string parameters | `Query(params): Query<SearchParams>` |

### 11. Caching

```rust
// Get
let val: Option<String> = state.cache.get("key").await;

// Set with 5 minute TTL
state.cache.set("key", "value", Some(300)).await;

// Delete
state.cache.delete("key").await;
```

Backends:
- **In-memory** (default) — `DashMap` with lazy TTL expiration
- **Redis** (feature: `redis`) — uses `SETEX` for TTL, auto-reconnect

### 12. File Uploads

```rust
use chopin_core::storage::FileUploadService;

let service = FileUploadService::new(&config);
let result = service.upload("photo.jpg", &bytes, "image/jpeg").await?;
// result.path, result.url
```

Backends:
- **Local** (default) — saves to `UPLOAD_DIR` with UUID filenames
- **S3** (feature: `s3`) — uploads to configured S3 bucket

### 13. Testing

```rust
use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_signup() {
    let app = TestApp::new().await;  // In-memory SQLite, random port
    let res = app.client.post(&app.url("/api/auth/signup"), r#"{"email":"a@b.com","username":"alice","password":"secret123"}"#).await;
    assert_eq!(res.status, 201);
}
```

Helpers:
```rust
let (token, user) = app.create_user("email", "username", "password").await;
let token = app.login("email", "password").await;
let res = app.client.get_with_token(&app.url("/api/me"), &token).await;
```

### 14. OpenAPI

Chopin auto-generates OpenAPI 3.1 specs. Add annotations:

```rust
#[utoipa::path(get, path = "/api/posts", tag = "posts",
    responses((status = 200, body = ApiResponse<Vec<PostResponse>>)))]
async fn list_posts(...) { ... }
```

Register in an OpenApi struct:
```rust
#[derive(OpenApi)]
#[openapi(paths(list_posts, create_post), components(schemas(PostResponse)))]
pub struct ApiDoc;
```

### 15. Feature Flags

| Feature | Cargo Flag | Effect |
|---------|-----------|--------|
| Redis | `--features redis` | Redis-backed CacheService |
| GraphQL | `--features graphql` | async-graphql routes |
| S3 | `--features s3` | AWS S3 file storage |
| Performance | `--features perf` | mimalloc global allocator |

### 16. Performance Architecture

**FastRoute API** — Users register zero-allocation static response endpoints:

```rust
use chopin_core::{App, FastRoute};

let app = App::new().await?
    .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));
app.run().await?;
```

The performance mode stack:

```
SO_REUSEPORT × N CPU cores  (kernel distributes connections)
  → per-core TcpListener (backlog 8192, SO_REUSEADDR)
    → TCP_NODELAY on accept
      → hyper::http1::Builder (keep_alive, pipeline_flush, max_buf_size=8192)
        → ChopinService::call(req)
          → FastRoute match → ChopinFuture::Ready (ZERO alloc, no Box::pin)
          → no match        → Axum Router with full middleware
```

JSON serialization uses **crate::json** abstraction:
- **With `perf` feature**: sonic-rs (SIMD-accelerated, ~40% faster)
- **Without `perf`**: serde_json (stable fallback)

All serialization via `to_writer` into pre-allocated buffers.

Release profile: `opt-level=3`, `lto="fat"`, `codegen-units=1`, `strip=true`, `panic="abort"`, `overflow-checks=false`.

> **📚 For a complete guide on building high-performance applications**, see [building-high-performance-apps.md](./building-high-performance-apps.md) which covers setup, optimization patterns, caching strategies, and production deployment.

## CLI Commands

```bash
chopin new <name>                          # Create project
chopin generate model <name> [fields...]   # Generate model + migration
chopin generate controller <name>          # Generate CRUD controller
chopin db migrate                          # Run migrations
chopin db rollback                         # Rollback last migration
chopin db reset                            # Drop & recreate
chopin db seed                             # Seed data
chopin db status                           # Migration status
chopin run                                 # Start dev server
chopin createsuperuser                     # Create admin user
chopin docs export <file>                  # Export OpenAPI spec
chopin info                                # Framework info
```

## Common Patterns

### Full CRUD Controller

```rust
use axum::{Router, routing::{get, post, put, delete}, extract::{State, Path}};
use chopin_core::{ApiResponse, ChopinError, controllers::AppState, extractors::{Json, AuthUser, Pagination, PaginatedResponse}};
use sea_orm::{EntityTrait, ActiveModelTrait, Set, PaginatorTrait, QueryOrder};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts", get(list).post(create))
        .route("/api/posts/:id", get(show).put(update).delete(destroy))
}

async fn list(State(state): State<AppState>, pagination: Pagination) -> Result<ApiResponse<PaginatedResponse<Vec<PostResponse>>>, ChopinError> {
    let total = posts::Entity::find().count(&state.db).await? as u64;
    let items = posts::Entity::find()
        .order_by_desc(posts::Column::Id)
        .offset(Some(pagination.offset()))
        .limit(Some(pagination.limit()))
        .all(&state.db)
        .await?
        .into_iter()
        .map(PostResponse::from)
        .collect();
    Ok(ApiResponse::success(pagination.response(items, total)))
}

async fn create(State(state): State<AppState>, user: AuthUser, Json(body): Json<CreatePost>) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let model = posts::ActiveModel {
        title: Set(body.title),
        body: Set(body.body),
        author_id: Set(user.user_id),
        ..Default::default()
    };
    let result = model.insert(&state.db).await?;
    Ok(ApiResponse::created(PostResponse::from(result)))
}

async fn show(State(state): State<AppState>, Path(id): Path<i32>) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let post = posts::Entity::find_by_id(id).one(&state.db).await?
        .ok_or(ChopinError::NotFound("Post not found".into()))?;
    Ok(ApiResponse::success(PostResponse::from(post)))
}

async fn update(State(state): State<AppState>, user: AuthUser, Path(id): Path<i32>, Json(body): Json<UpdatePost>) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let post = posts::Entity::find_by_id(id).one(&state.db).await?
        .ok_or(ChopinError::NotFound("Post not found".into()))?;
    let mut active: posts::ActiveModel = post.into();
    if let Some(title) = body.title { active.title = Set(title); }
    if let Some(body_text) = body.body { active.body = Set(body_text); }
    let updated = active.update(&state.db).await?;
    Ok(ApiResponse::success(PostResponse::from(updated)))
}

async fn destroy(State(state): State<AppState>, user: AuthUser, Path(id): Path<i32>) -> Result<ApiResponse<()>, ChopinError> {
    let post = posts::Entity::find_by_id(id).one(&state.db).await?
        .ok_or(ChopinError::NotFound("Post not found".into()))?;
    post.delete(&state.db).await?;
    Ok(ApiResponse::success_message("Deleted"))
}
```

### Custom Example App (not using App::new)

```rust
use std::sync::Arc;
use axum::Router;
use chopin_core::{config::Config, db, controllers::AppState, cache::CacheService};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let db = db::connect(&config).await?;
    let cache = CacheService::in_memory();

    let state = AppState {
        db,
        config: Arc::new(config.clone()),
        cache,
    };

    let app = Router::new()
        .merge(my_controllers::routes())
        .with_state(state)
        .layer(axum::Extension(Arc::new(config.clone())))
        .layer(tower_http::cors::CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(&config.server_addr()).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

## Key Implementation Details for LLMs

1. **Config is `Arc<Config>`** in AppState — never clone the full Config, always Arc.
2. **JSON uses `crate::json::`** — dispatches to sonic-rs (with `perf`) or serde_json (fallback). All `ApiResponse` and `ChopinError` serialize with `crate::json::to_writer()` into pre-allocated buffers.
3. **The `Json` extractor** in `chopin_core::extractors` uses `crate::json::from_slice()` — it's NOT `axum::Json`.
4. **FastRoute bypasses Axum** — `ChopinService` checks user-registered `FastRoute`s before the Router. No hardcoded paths.
5. **`ChopinFuture` avoids `Box::pin`** — fast routes return `ChopinFuture::Ready` (stack-allocated), only Router path boxes.
6. **SO_REUSEPORT uses `socket2`** — creates N `socket2::Socket` instances, converts to `tokio::TcpListener`.
7. **mimalloc is optional** — only active with `--features perf`. Set via `#[global_allocator]` in `lib.rs`.
8. **Date header is cached** — `perf::cached_date_header()` uses `std::sync::RwLock` (not tokio), updated every 500ms. Readers never block.
9. **Migrations auto-run** — both `App::new()` and `App::with_config()` call `Migrator::up()` on startup.
10. **Role is an integer in the DB** — `Role::User = 0`, `Role::Admin = 2`, etc. Stored in the `role` column.
11. **AuthUser reads from request Extensions** — the JWT middleware injects claims via `axum::Extension<Arc<Config>>`.
