# Chopin Architecture

## Design Philosophy

Chopin is built on three core principles:

1. **Convention over Configuration** - Sensible defaults that work out of the box
2. **Performance without Complexity** - Optimized for speed while maintaining simplicity
3. **Developer Experience First** - Intuitive APIs that feel natural to web developers

## Framework Stack

```
┌─────────────────────────────────────────┐
│         Your Application Code           │
│    (Models, Controllers, Handlers)      │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│           Chopin Core                   │
│  (App, Routing, Auth, Extractors)      │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│           Web Framework                 │
│         (Axum + Tower)                  │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│          ORM & Database                 │
│     (SeaORM + SQLx)                     │
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│         Runtime & Async                 │
│            (Tokio)                      │
└─────────────────────────────────────────┘
```

## Core Components

### 1. App (Application)

The `App` struct is the heart of Chopin. It:
- Loads configuration from environment variables
- Establishes database connection pool
- Runs pending migrations
- Builds the route tree
- Starts the HTTP server

**Location**: `chopin-core/src/app.rs`

```rust
pub struct App {
    pub config: Config,
    pub db: DatabaseConnection,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn Error>>;
    pub fn router(&self) -> Router;
    pub async fn run(self) -> Result<(), Box<dyn Error>>;
}
```

### 2. Config (Configuration)

Configuration is loaded from environment variables via the `.env` file.

**Location**: `chopin-core/src/config.rs`

```rust
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub server_host: String,
    pub server_port: u16,
    pub environment: String,
}
```

### 3. Database

SeaORM provides the ORM layer with:
- Type-safe query builder
- Automatic migrations
- Connection pooling
- Multi-database support (SQLite, PostgreSQL, MySQL)

**Location**: `chopin-core/src/db.rs`

### 4. Routing

Routes are organized hierarchically:

```
/api
├── /auth
│   ├── POST /signup
│   └── POST /login
└── /{resource}
    ├── GET    /          (list)
    ├── POST   /          (create)
    ├── GET    /:id       (get)
    ├── PUT    /:id       (update)
    └── DELETE /:id       (delete)
```

**Location**: `chopin-core/src/routing.rs`

### 5. Controllers

Controllers contain handler functions that process requests:

```rust
async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateRequest>,
) -> Result<ApiResponse<Response>, ChopinError> {
    // Business logic here
}
```

**Location**: `chopin-core/src/controllers/`

### 6. Extractors

Custom Axum extractors provide convenient request handling:

- `Json<T>` - Fast JSON deserialization (sonic-rs)
- `AuthUser` - Extracts authenticated user from JWT
- `Pagination` - Extracts limit/offset from query params

**Location**: `chopin-core/src/extractors/`

### 7. Middleware Stack

Applied to all routes in order:

1. **Request ID** - Adds `x-request-id` header
2. **CORS** - Cross-origin resource sharing
3. **Compression** - gzip/brotli response compression
4. **Tracing** - Request/response logging

**Location**: `chopin-core/src/app.rs` (router method)

## Request Lifecycle

```
1. HTTP Request arrives
   ↓
2. Middleware Stack (in order)
   → Request ID
   → CORS
   → Tracing (start)
   ↓
3. Route Matching (matchit)
   ↓
4. Extractors
   → Parse JSON body
   → Validate JWT token
   → Extract query params
   ↓
5. Handler Function
   → Business logic
   → Database queries
   → Response building
   ↓
6. Response Serialization
   → JSON encoding (sonic-rs)
   → Error handling
   ↓
7. Middleware Stack (reverse order)
   → Tracing (end)
   → Compression
   ↓
8. HTTP Response sent
```

## Data Flow

### Request → Handler

```
HTTP POST /api/posts
{
  "title": "Hello",
  "body": "World"
}
   ↓
Json<CreatePostRequest>  (extractor deserializes)
   ↓
Handler receives typed struct
   ↓
Validation happens automatically via serde
```

### Handler → Response

```
Handler returns Result<ApiResponse<T>, ChopinError>
   ↓
Ok path:
  ApiResponse::success(data)
  → Serialized to standardized JSON
  → HTTP 200 with { success: true, data: ... }

Err path:
  ChopinError::NotFound(..)
  → Converted to ApiResponse
  → HTTP 404 with { success: false, error: ... }
```

## Authentication Architecture

```
┌──────────────┐    signup/login     ┌──────────────┐
│   Client     │ ──────────────────→ │   Server     │
│              │                      │              │
│              │ ←────────────────── │              │
│              │   JWT token          │              │
└──────────────┘                      └──────────────┘
       │                                      ↑
       │  Subsequent requests with            │
       │  Authorization: Bearer <token>       │
       └──────────────────────────────────────┘
```

### JWT Token Structure

```json
{
  "sub": 1,              // User ID
  "exp": 1707782400      // Expiration timestamp
}
```

Token is:
- Signed with HMAC-SHA256
- Uses hardware AES acceleration (ring crate)
- Contains minimal payload for performance

### Password Hashing

- Algorithm: **Argon2id** (memory-hard, resistant to GPU attacks)
- Salt: Automatically generated per-password
- Parameters: Balanced for security and performance

## Error Handling

All errors flow through the `ChopinError` enum:

```rust
pub enum ChopinError {
    Unauthorized(String),
    NotFound(String),
    Conflict(String),
    Validation(String),
    Database(DbErr),
    Internal(String),
}
```

Errors implement `IntoResponse`, automatically converting to JSON:

```json
{
  "success": false,
  "error": {
    "code": "NOT_FOUND",
    "message": "Post with id 42 not found"
  }
}
```

**Location**: `chopin-core/src/error.rs`

## Database Architecture

### Connection Pool

SeaORM manages connection pooling automatically:

```rust
Database::connect(&config.database_url)
    .await?
```

Default settings:
- Max connections: 100
- Min connections: 5
- Connection timeout: 8s
- Idle timeout: 8s

### Migrations

Migrations are embedded in the binary and run on startup:

```rust
pub struct Migrator;

#[async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250211_000001_create_users_table::Migration),
            // ... more migrations
        ]
    }
}
```

**Location**: `chopin-core/src/migrations/mod.rs`

### Entity-First Design

Models are defined as SeaORM entities:

```rust
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub email: String,
    // ...
}
```

Queries are type-safe and composable:

```rust
User::find()
    .filter(user::Column::Email.eq("user@example.com"))
    .one(&db)
    .await?
```

## Performance Optimizations

### 1. Apple Silicon NEON

**sonic-rs** uses ARM NEON SIMD instructions for JSON:
- 10-15% faster than serde_json
- Enabled via `.cargo/config.toml` rustflags

### 2. Hardware AES

**ring** crate uses native AES instructions:
- 5-10% faster JWT token operations
- Reduced CPU usage for crypto

### 3. Compile-Time Optimizations

```toml
[profile.release]
opt-level = 3              # Maximum optimization
lto = "fat"               # Full link-time optimization
codegen-units = 1          # Single codegen unit
strip = true              # Strip debug symbols
```

### 4. Connection Pooling

Reused database connections eliminate connection overhead.

### 5. Zero-Copy Deserialization

sonic-rs minimizes allocations during JSON parsing.

## Testing Architecture

### TestApp

Provides isolated test environment:

```rust
pub struct TestApp {
    pub addr: SocketAddr,
    pub db: DatabaseConnection,
    pub client: TestClient,
}
```

Each test gets:
- Fresh in-memory database
- Random port
- Helper methods for common operations

**Location**: `chopin-core/src/testing.rs`

### Integration Tests

Tests make real HTTP requests:

```rust
#[tokio::test]
async fn test_signup() {
    let app = TestApp::new().await;
    let response = app.client
        .post(&app.url("/api/auth/signup"))
        .json(&signup_payload)
        .send()
        .await;
    
    assert_eq!(response.status, 200);
}
```

## Code Organization

### Workspace Structure

```
chopin/
├── chopin-core/       # Framework library (pub)
├── chopin-cli/        # CLI tool (bin)
└── chopin-examples/   # Example apps
```

### Module Boundaries

- **Public API**: `chopin_core::*` exports
- **Internal**: Types not re-exported from lib.rs
- **CLI**: Completely separate from core library

### Dependency Management

Core dependencies:
- `axum` - Web framework
- `sea-orm` - ORM
- `tokio` - Async runtime
- `serde` - Serialization
- `sonic-rs` - Fast JSON
- `jsonwebtoken` - JWT
- `argon2` - Password hashing
- `utoipa` - OpenAPI

## Extension Points

### Custom Controllers

Add your own routes:

```rust
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/custom", get(handler))
}
```

Register in main router:

```rust
app.router()
    .nest("/api/custom", custom::routes())
```

### Custom Middleware

Use Tower middleware:

```rust
.layer(middleware::from_fn(my_middleware))
```

### Custom Extractors

Implement `FromRequestParts`:

```rust
#[async_trait]
impl<S> FromRequestParts<S> for MyExtractor
where
    S: Send + Sync,
{
    type Rejection = ChopinError;
    
    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Extract logic
    }
}
```

## Design Decisions

### Why Axum?

- Type-safe extractors
- Composable middleware (Tower)
- Excellent performance
- Growing ecosystem
- Good ergonomics

### Why SeaORM?

- Async-first design
- Type-safe queries
- Migration support
- Multi-database
- Active development

### Why sonic-rs?

- ARM NEON SIMD optimization
- Drop-in serde replacement
- Faster than serde_json on Apple Silicon

### Why JWT?

- Stateless (no session storage)
- Horizontally scalable
- Standard format
- Hardware-accelerated crypto

### Why Convention over Configuration?

- Faster onboarding
- Less boilerplate
- Consistent patterns
- Easy to scaffold

## Future Architecture

Planned additions:

- **Permissions system** - Role-based access control (RBAC)
- **Background jobs** - Async task queue
- **Caching layer** - Redis integration
- **GraphQL** - Alternative API style
- **WebSockets** - Real-time communication
- **Rate limiting** - Request throttling

---

## Summary

Chopin's architecture prioritizes:

✅ **Simplicity** - Clear layers, minimal abstractions  
✅ **Performance** - Hardware-optimized, zero-cost patterns  
✅ **Safety** - Type-safe queries, compile-time errors  
✅ **Ergonomics** - Intuitive APIs, sensible defaults  
✅ **Extensibility** - Easy to add custom functionality  

The modular design allows you to use Chopin's components à la carte or embrace the full framework experience.
