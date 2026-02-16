# 🎹 Chopin Framework - Architecture & Design

> **High-fidelity engineering for the modern virtuoso.**

## Table of Contents

- [Overview](#overview)
- [Core Architecture](#core-architecture)
- [Component Architecture](#component-architecture)
- [Performance Architecture](#performance-architecture)
- [Security Architecture](#security-architecture)
- [Design Principles](#design-principles)
- [Technology Stack](#technology-stack)
- [Directory Structure](#directory-structure)
- [Extension Points](#extension-points)

---

## Overview

Chopin is a **Rust-native, Type-safe Web Framework** that brings Django's "batteries-included" philosophy to the world of high-performance systems programming. It achieves **650K+ req/s** throughput with **sub-millisecond latency** while providing a modular, compile-time-verified architecture for building production-ready APIs.

### Design Philosophy: Django Meets Rust

Chopin evolves the Django philosophy into a **Static Modular Composition** model:

1. **Trait-Based Modules** - Every feature (Auth, Blog, Billing) is a self-contained `ChopinModule` with explicit registration
2. **Hub-and-Spoke Architecture** - Thin `chopin-core` hub prevents circular dependencies while enabling composition
3. **MVSR Pattern** - Model-View-Service-Router structure separates HTTP concerns from business logic
4. **Type-Safe by Default** - Route conflicts, missing configurations, and module errors caught at compile-time
5. **Performance First** - Extreme optimization through FastRoute, SO_REUSEPORT, and zero-allocation design

### Key Differentiators

- **Django's Comfort, Rust's Safety** - Feature-first folders with compile-time verification (no `KeyError` at 3 AM)
- **Modular Composition** - Apps are self-contained units that explicitly declare dependencies
- **Built-in, Not Hard-coded** - Auth, Admin, etc. are official modules you opt into, not core framework bloat
- **Dual-path routing** - FastRoute for static responses, Axum Router for dynamic content
- **SO_REUSEPORT** - Multi-core kernel-level load balancing with per-core runtimes
- **Zero-allocation hot path** - Pre-computed headers, lock-free caching

---

## Core Architecture

### High-Level Architecture: Hub-and-Spoke Modular Composition

```
┌─────────────────────────────────────────────────────────────┐
│                   Chopin Application                         │
│                  (The "Composer")                            │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  main.rs: Explicit Module Registration                       │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ App::new()                                           │    │
│  │   .mount_module(BlogModule::new())                  │    │
│  │   .mount_module(AuthModule::new())                  │    │
│  │   .mount_module(TenantManagerModule::new())         │    │
│  │   .run()                                             │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                    Module Layer                              │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ BlogModule   │  │ AuthModule   │  │ TenantModule │      │
│  │              │  │ (vendor/)    │  │              │      │
│  │ ChopinModule │  │ ChopinModule │  │ ChopinModule │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                  │                  │              │
│         └──────────────────┴──────────────────┘              │
│                            │                                 │
│                            ▼                                 │
│  ┌─────────────────────────────────────────────────────┐    │
│  │         chopin-core (The Hub)                       │    │
│  │  - ChopinModule trait                               │    │
│  │  - Shared types (User, Config, etc.)               │    │
│  │  - Core services (DB, Cache, Storage)              │    │
│  │  - FastRoute & ChopinService                        │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                      ChopinService                           │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Request Processing Pipeline               │  │
│  │                                                         │  │
│  │  1. FastRoute match?                                   │  │
│  │     ├─ Hit: Pre-computed response (~35ns)             │  │
│  │     └─ Miss: continue to step 2                       │  │
│  │                                                         │  │
│  │  2. OPTIONS preflight?                                 │  │
│  │     ├─ Yes: Pre-computed 204 CORS response            │  │
│  │     └─ No: continue to step 3                         │  │
│  │                                                         │  │
│  │  3. Axum Router                                        │  │
│  │     ├─ Tower middleware stack                          │  │
│  │     ├─ Request ID propagation                          │  │
│  │     ├─ Tracing / logging                               │  │
│  │     ├─ CORS layer                                      │  │
│  │     ├─ Compression                                     │  │
│  │     ├─ Rate limiting                                   │  │
│  │     └─ Handler execution                               │  │
│  │                                                         │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                    Hyper HTTP Server                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ SO_REUSEPORT (when REUSEPORT=true)                   │   │
│  │                                                        │   │
│  │  Core 0: Accept loop → tokio current_thread          │   │
│  │  Core 1: Accept loop → tokio current_thread          │   │
│  │  Core 2: Accept loop → tokio current_thread          │   │
│  │  Core N: Accept loop → tokio current_thread          │   │
│  │                                                        │   │
│  │  (Kernel-level load balancing across cores)          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ TCP Optimizations:                                    │   │
│  │  - TCP_NODELAY (disable Nagle's algorithm)           │   │
│  │  - HTTP/1.1 keep-alive                                │   │
│  │  - Pipeline flush                                     │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Request Flow Diagram

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ HTTP Request
       ▼
┌─────────────────────────────────────────────────────┐
│            SO_REUSEPORT Kernel LB                    │
│  (distributes connections across CPU cores)          │
└──────────────────┬──────────────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
┌─────────────┐         ┌─────────────┐
│  Core 0     │   ...   │  Core N     │
│  Accept Loop│         │  Accept Loop│
└──────┬──────┘         └──────┬──────┘
       │                       │
       ▼                       ▼
┌─────────────────────────────────────────────────────┐
│           Hyper HTTP/1.1 Protocol Handler           │
└──────────────────┬──────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────┐
│              ChopinService::call()                   │
└──────────────────┬──────────────────────────────────┘
                   │
                   ▼
         ┌────────────────┐
         │ FastRoute      │◄──── Path match + Method check
         │ match?         │
         └────┬───────┬───┘
              │       │
       [HIT]  │       │  [MISS]
              ▼       │
      ╔═══════════════╗    │
      ║ Pre-computed  ║    │
      ║ Response      ║    │
      ║ (~35ns)       ║    │
      ╚═══════════════╝    │
              │            │
              │            ▼
              │    ┌────────────────┐
              │    │ OPTIONS        │
              │    │ Preflight?     │
              │    └────┬───────┬───┘
              │         │       │
              │  [YES]  │       │  [NO]
              │         ▼       │
              │  ╔═══════════════╗  │
              │  ║ CORS 204      ║  │
              │  ║ Response      ║  │
              │  ╚═══════════════╝  │
              │         │           │
              │         │           ▼
              │         │   ┌──────────────────┐
              │         │   │  Axum Router     │
              │         │   └────────┬─────────┘
              │         │            │
              │         │            ▼
              │         │   ┌──────────────────┐
              │         │   │ Tower Middleware  │
              │         │   │ - Request ID      │
              │         │   │ - Tracing         │
              │         │   │ - CORS            │
              │         │   │ - Compression     │
              │         │   │ - Rate Limit      │
              │         │   └────────┬─────────┘
              │         │            │
              │         │            ▼
              │         │   ┌──────────────────┐
              │         │   │ Route Handler    │
              │         │   │ - Extractors     │
              │         │   │ - Auth checks    │
              │         │   │ - Business logic │
              │         │   │ - DB queries     │
              │         │   └────────┬─────────┘
              │         │            │
              └─────────┴────────────┘
                        │
                        ▼
              ┌──────────────────┐
              │ HTTP Response    │
              └────────┬─────────┘
                       │
                       ▼
              ┌──────────────────┐
              │     Client       │
              └──────────────────┘
```

---

## Component Architecture

### 1. Application Layer: The Composer (`app.rs`)

**Responsibilities:**
- Module registration and composition
- Application initialization and lifecycle management
- Database connection and migration handling
- Cache service initialization
- Service composition and dependency injection
- Configuration management

**Key Components:**

```rust
pub struct App {
    pub config: Config,              // Environment configuration
    pub db: DatabaseConnection,      // SeaORM database connection
    pub cache: CacheService,         // Cache backend (memory/Redis)
    modules: Vec<Box<dyn ChopinModule>>, // Registered modules
    fast_routes: Vec<FastRoute>,     // Zero-alloc static routes
    custom_openapi: Option<OpenApi>, // Custom OpenAPI spec
    api_docs_path: String,           // OpenAPI docs endpoint
}

/// The core trait that all modules must implement
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

**Initialization Flow (Modular Composition):**
1. Load configuration from environment variables
2. Connect to database with connection pool
3. **Mount modules** via `.mount_module()` - each module registers its routes
4. Run module-specific migrations in dependency order
5. Initialize shared services (cache, storage)
6. Compose final router from all module routes
7. Build OpenAPI documentation from all modules
8. Spawn server with optional SO_REUSEPORT

**Example Usage:**

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    App::new().await?
        .mount_module(TenantManagerModule::new())
        .mount_module(AuthModule::new()) // from vendor/chopin_auth
        .mount_module(BlogModule::new())  // from apps/blog
        .run().await?;
    
    Ok(())
}
```

### 2. Configuration Layer (`config.rs`)

**Responsibilities:**
- Environment-based configuration
- Security settings management
- Database connection parameters
- Server tuning parameters

**Key Types:**

```rust
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub host: String,
    pub port: u16,
    pub environment: String,
    pub redis_url: Option<String>,
    pub upload_dir: String,
    pub security: SecurityConfig,
}

pub struct SecurityConfig {
    // 2FA/TOTP
    pub enable_2fa: bool,
    
    // Rate limiting
    pub enable_rate_limit: bool,
    pub rate_limit_max_attempts: u32,
    pub rate_limit_window_secs: u64,
    
    // Account lockout
    pub enable_account_lockout: bool,
    pub lockout_max_attempts: u32,
    pub lockout_duration_secs: u64,
    
    // Refresh tokens
    pub enable_refresh_tokens: bool,
    pub refresh_token_expiry_days: u64,
    
    // Session management
    pub enable_session_management: bool,
    
    // Password reset
    pub enable_password_reset: bool,
    pub password_reset_expiry_secs: u64,
    
    // Email verification
    pub enable_email_verification: bool,
    pub email_verification_expiry_secs: u64,
    
    // CSRF protection
    pub enable_csrf: bool,
    
    // Device tracking
    pub enable_device_tracking: bool,
    
    // Password policy
    pub min_password_length: usize,
}
```

### 3. Server Layer (`server.rs`)

**Responsibilities:**
- HTTP server management (SO_REUSEPORT support)
- FastRoute static response handling
- Zero-allocation response generation
- Per-route optimization decorators

**Key Components:**

```rust
pub struct FastRoute {
    path: Box<str>,                      // Exact path match
    body: Bytes,                         // Pre-computed body
    base_headers: HeaderMap,             // Pre-built headers
    preflight_headers: Option<HeaderMap>,// CORS preflight headers
    allowed_methods: Option<Box<[Method]>>, // Method filter
}

impl FastRoute {
    // Constructors
    pub fn json(path: &str, body: &[u8]) -> Self
    pub fn text(path: &str, body: &[u8]) -> Self
    pub fn html(path: &str, body: &[u8]) -> Self
    
    // Decorators (zero per-request cost)
    pub fn cors(self) -> Self
    pub fn cache_control(self, value: &str) -> Self
    pub fn header(self, name: HeaderName, value: &str) -> Self
    pub fn methods(self, methods: &[Method]) -> Self
    pub fn get_only(self) -> Self
    pub fn post_only(self) -> Self
}
```

**ChopinService Architecture:**

```rust
pub struct ChopinService {
    fast_routes: Arc<Vec<FastRoute>>,
    axum_service: AxumService,
}

// Service::call() implementation:
// 1. Fast path: O(n) linear scan for FastRoute match
//    - Method check
//    - Pre-computed response clone (~35ns)
// 2. CORS preflight: Pre-computed 204 response
// 3. Fallback: Axum Router with full middleware stack
```

### 4. Database Layer (`db.rs`, `models/`, `migrations/`)

**Responsibilities:**
- Database connection pooling
- ORM entity definitions
- Migration management
- Query optimization

**Architecture:**

```
┌─────────────────────────────────────────┐
│          SeaORM Entity Models            │
│  - User, Role, Session                   │
│  - SecurityToken, RefreshToken           │
│  - DeviceInfo                            │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│      SeaORM Query Builder                │
│  - Type-safe query construction          │
│  - Relation handling                     │
│  - Eager & lazy loading                  │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│        Connection Pool                   │
│  - Max: 100 connections                  │
│  - Min: 5 connections                    │
│  - Timeout: 8 seconds                    │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│    Database Backend (SQLx)               │
│  - SQLite                                │
│  - PostgreSQL                            │
│  - MySQL                                 │
└─────────────────────────────────────────┘
```

### 5. Authentication & Security Layer (`auth/`)

**Module Structure:**

```
auth/
├── mod.rs              # Public API exports
├── jwt.rs              # JWT token creation/validation
├── password.rs         # Argon2id password hashing
├── totp.rs             # 2FA/TOTP implementation
├── csrf.rs             # CSRF token generation/validation
├── rate_limit.rs       # Login rate limiting
├── lockout.rs          # Account lockout tracking
├── refresh.rs          # Refresh token rotation
├── session.rs          # Session management
├── security_token.rs   # Password reset/email verification tokens
└── device_tracking.rs  # Device fingerprinting
```

**Security Features:**

1. **Password Security**
   - Argon2id hashing (OWASP recommended)
   - Configurable minimum length
   - Timing-safe comparison

2. **JWT Tokens**
   - HS256 signing
   - Configurable expiry
   - Claims-based authorization
   - Role extraction

3. **2FA/TOTP**
   - RFC 6238 compliant
   - QR code generation
   - Backup codes support

4. **Rate Limiting**
   - Per-IP tracking
   - Configurable windows and thresholds
   - Automatic cleanup

5. **Account Lockout**
   - Failed attempt tracking
   - Time-based lockout
   - Admin override capability

6. **Refresh Tokens**
   - Rotation on use
   - Secure random generation
   - Family tracking
   - Revocation support

7. **CSRF Protection**
   - Double-submit cookies
   - Token validation
   - SameSite cookie support

8. **Session Management**
   - Server-side session tracking
   - Token blacklist
   - Concurrent session management

### 6. Cache Layer (`cache.rs`)

**Architecture:**

```
┌────────────────────────────────────────┐
│         CacheService API                │
│  - get_json<T>()                        │
│  - set_json<T>()                        │
│  - get_raw()                            │
│  - set_raw()                            │
│  - del()                                │
│  - exists()                             │
│  - flush()                              │
└──────────────┬─────────────────────────┘
               │
               ▼
┌────────────────────────────────────────┐
│       CacheBackend Trait                │
└──────┬──────────────────────┬──────────┘
       │                      │
       ▼                      ▼
┌─────────────────┐   ┌─────────────────┐
│ InMemoryCache   │   │  RedisCache     │
│ - HashMap       │   │  - Redis client │
│ - RwLock        │   │  - Connection   │
│ - TTL tracking  │   │    pool         │
└─────────────────┘   └─────────────────┘
```

**Features:**
- Pluggable backend architecture
- JSON serialization/deserialization
- TTL support
- Type-safe API
- Async/await interface

### 7. Storage Layer (`storage.rs`)

**Architecture:**

```
┌────────────────────────────────────────┐
│       StorageBackend Trait              │
│  - store(filename, content_type, data)  │
│  - delete(stored_name)                  │
│  - exists(stored_name)                  │
│  - url(stored_name)                     │
└──────┬─────────────────────┬───────────┘
       │                     │
       ▼                     ▼
┌─────────────────┐   ┌─────────────────┐
│ LocalStorage    │   │   S3Storage     │
│ - upload_dir    │   │   - AWS SDK     │
│ - UUID names    │   │   - R2/MinIO    │
└─────────────────┘   └─────────────────┘
```

**Features:**
- Multipart file upload handling
- UUID-based file naming (collision prevention)
- Content-type detection
- File metadata tracking
- S3-compatible storage support (R2, MinIO)

### 8. API Documentation Layer (`openapi.rs`)

**Responsibilities:**
- Auto-generate OpenAPI 3.0 specification
- Serve interactive Scalar UI
- Schema generation from Rust types
- Endpoint documentation

**Integration:**

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        controllers::auth::signup,
        controllers::auth::login,
        // ...
    ),
    components(
        schemas(
            SignupRequest,
            LoginRequest,
            AuthResponse,
            // ...
        )
    ),
    tags(
        (name = "auth", description = "Authentication endpoints")
    )
)]
struct ApiDoc;
```

### 9. Testing Layer (`testing.rs`)

**Architecture:**

```rust
pub struct TestApp {
    pub app: App,              // Chopin application instance
    pub client: TestClient,    // HTTP test client
    pub db: DatabaseConnection,// Direct DB access
    pub cache: CacheService,   // Direct cache access
}

impl TestApp {
    // Initialization
    pub async fn new() -> Result<Self>
    pub async fn with_config(config: Config) -> Result<Self>
    
    // Request builders
    pub fn get(&self, uri: &str) -> RequestBuilder
    pub fn post(&self, uri: &str) -> RequestBuilder
    pub fn put(&self, uri: &str) -> RequestBuilder
    pub fn delete(&self, uri: &str) -> RequestBuilder
    
    // Convenience methods
    pub async fn signup_user(&self, ...) -> TestResponse
    pub async fn login_user(&self, ...) -> TestResponse
}
```

**Features:**
- In-memory SQLite database
- Request/response builders
- Authentication helpers
- Database fixture support
- Isolated test environments

### 10. Extractors Layer (`extractors/`)

**Available Extractors:**

```rust
// Authentication
pub struct AuthUser(pub User);

// Role-based access control
pub struct UserRole(pub User);
pub struct ModeratorRole(pub User);
pub struct AdminRole(pub User);

// JSON with performance optimization
pub struct Json<T>(pub T);

// Pagination
pub struct Pagination {
    pub page: u64,
    pub per_page: u64,
}
```

**Usage Example:**

```rust
async fn admin_only(
    AdminRole(user): AdminRole,
) -> Result<ApiResponse<AdminData>, ChopinError> {
    // User is guaranteed to have Admin role
    Ok(ApiResponse::success(AdminData { user }))
}
```

---

## Performance Architecture

### 1. FastRoute Zero-Allocation Design

**Problem:** Traditional web frameworks allocate multiple heap objects per request (headers, body, metadata).

**Solution:** Pre-compute everything at route registration time.

```
Registration time (once):
├─ Parse & validate path
├─ Build HeaderMap with Content-Type, Content-Length, Server
├─ Add decorator headers (CORS, Cache-Control, custom)
├─ Convert body to Bytes (may be &'static from binary)
└─ Store in Vec<FastRoute>

Request time (per request):
├─ Linear scan for path match (~10-50ns for typical API)
├─ Method check
├─ Clone pre-built HeaderMap (~10ns memcpy)
├─ Clone pre-built Bytes (~5ns Arc increment)
└─ Total: ~35ns
```

**Performance Characteristics:**
- **Throughput:** ~28M req/s (single-core)
- **Latency:** ~35ns median
- **Allocations:** 0 heap allocations on hot path
- **CPU:** ~3% CPU at 650K req/s (8-core)

### 2. SO_REUSEPORT Multi-Core Architecture

**Problem:** Single accept loop becomes bottleneck at high connection rates.

**Solution:** Kernel-level load balancing with per-core runtimes.

```
┌─────────────────────────────────────────────────────────┐
│                  Linux Kernel                            │
│                                                           │
│  SO_REUSEPORT socket distribution:                       │
│  - Hash (src_ip, src_port, dst_ip, dst_port)           │
│  - Distribute connections across bind() calls            │
│  - Maintains connection affinity                         │
└──────┬─────────┬─────────┬─────────┬────────────────────┘
       │         │         │         │
       ▼         ▼         ▼         ▼
   ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐
   │ Core 0│ │ Core 1│ │ Core 2│ │ Core N│
   └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘
       │         │         │         │
   ┌───────────────────────────────────────┐
   │ tokio::runtime::Builder               │
   │   .new_current_thread()               │
   │   .enable_all()                       │
   │   .build()                            │
   └───────────────────────────────────────┘
```

**Benefits:**
- **Zero cross-core contention** - Each core has its own runtime
- **Cache locality** - Requests processed on same core
- **Horizontal scaling** - Linear speedup with cores
- **No work stealing** - Eliminates scheduler overhead

**Performance Impact:**
- **4-core:** ~620K req/s
- **8-core:** ~650K req/s
- **16-core:** ~680K req/s (diminishing returns due to RAM bandwidth)

### 3. Lock-Free Date Header Caching

**Problem:** Formatting HTTP Date header is expensive (~500ns).

**Solution:** Global epoch counter + thread-local cache.

```
┌────────────────────────────────────────┐
│    Background Task (every 500ms)       │
│    - Increments AtomicU64 epoch        │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│      Global: DATE_EPOCH (AtomicU64)    │
│      AtomicU64::load(Relaxed) ~1ns     │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│   Thread-local: (epoch, HeaderValue)   │
│   RefCell<(u64, HeaderValue)>          │
│                                         │
│   If epoch matches: clone (~5ns)       │
│   Else: format once (~500ns)           │
└────────────────────────────────────────┘
```

**Characteristics:**
- **Cache hits:** ~5ns (clone HeaderValue)
- **Cache misses:** ~500ns (format once per thread per 500ms)
- **Miss rate:** ~0.2% at 1M req/s
- **Synchronization:** Zero cross-thread locking

### 4. Performance Features (`perf` feature flag)

When compiled with `--features perf`:

```toml
[dependencies]
mimalloc = "0.1"     # Microsoft high-performance allocator
sonic-rs = "0.3"     # SIMD-accelerated JSON
```

**mimalloc Benefits:**
- 20-30% faster than glibc malloc
- Better cache locality
- Thread-local heaps reduce contention

**sonic-rs Benefits:**
- SIMD-accelerated parsing/serialization
- 40% faster than serde_json
- Zero-copy string parsing

### 5. TCP & HTTP Optimizations

```rust
// TCP_NODELAY - disable Nagle's algorithm
socket.set_nodelay(true)?;

// HTTP/1.1 with keep-alive
let http = http1::Builder::new()
    .keep_alive(true)
    .pipeline_flush(true);
```

**Impact:**
- **TCP_NODELAY:** Reduces latency for small payloads by 20-40%
- **Keep-alive:** Eliminates connection setup overhead (RTT)
- **Pipeline flush:** Reduces buffering delay

---

## Security Architecture

### Threat Model

Chopin's security features protect against:

1. **Authentication attacks** - Brute force, credential stuffing
2. **Authorization bypass** - Privilege escalation, role confusion
3. **Session attacks** - Fixation, hijacking, replay
4. **CSRF attacks** - Cross-site request forgery
5. **Token theft** - JWT leakage, refresh token compromise
6. **Timing attacks** - Password comparison, token validation

### Defense-in-Depth Layers

```
┌──────────────────────────────────────────────────────────┐
│ Layer 7: Application Logic                               │
│  - Role-based access control                             │
│  - Business logic validation                             │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 6: Authorization                                    │
│  - JWT validation                                         │
│  - Role extractors                                        │
│  - Session management                                     │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 5: Authentication                                   │
│  - Password verification (Argon2id)                       │
│  - 2FA/TOTP validation                                    │
│  - Refresh token rotation                                │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 4: Anti-Abuse                                       │
│  - Rate limiting (per-IP, per-user)                       │
│  - Account lockout                                        │
│  - Device tracking                                        │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 3: Request Validation                               │
│  - CSRF protection                                        │
│  - Input sanitization                                     │
│  - Content-type validation                                │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 2: Transport Security                               │
│  - TLS termination (reverse proxy)                        │
│  - Secure headers (HSTS, CSP)                             │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ Layer 1: Network Security                                 │
│  - Firewall rules                                         │
│  - DDoS protection (external)                             │
└──────────────────────────────────────────────────────────┘
```

### Authentication Flow

```
┌────────────────────────────────────────────────────────────┐
│                    Login Attempt                            │
└────────────────────┬───────────────────────────────────────┘
                     │
                     ▼
         ┌─────────────────────────┐
         │ Rate Limit Check        │
         │ (per-IP tracking)       │
         └────────┬────────────────┘
                  │
          [BLOCKED]│  [ALLOWED]
                  │        │
                  ▼        ▼
         ┌─────────────────────────┐
         │ Account Lockout Check   │
         │ (failed attempt count)  │
         └────────┬────────────────┘
                  │
          [LOCKED]│  [OK]
                  │        │
                  ▼        ▼
         ┌─────────────────────────┐
         │ Password Verification   │
         │ (Argon2id, timing-safe) │
         └────────┬────────────────┘
                  │
          [FAIL]  │  [SUCCESS]
                  │        │
         Track    │        ▼
         failure  │  ┌─────────────────────────┐
                  │  │ 2FA/TOTP Check          │
                  │  │ (if enabled)            │
                  │  └────────┬────────────────┘
                  │           │
                  │   [FAIL]  │  [SUCCESS]
                  │           │        │
                  └───────────┘        ▼
                              ┌─────────────────────────┐
                              │ Generate Tokens         │
                              │ - Access token (JWT)    │
                              │ - Refresh token         │
                              └────────┬────────────────┘
                                       │
                                       ▼
                              ┌─────────────────────────┐
                              │ Create Session          │
                              │ (if enabled)            │
                              └────────┬────────────────┘
                                       │
                                       ▼
                              ┌─────────────────────────┐
                              │ Track Device            │
                              │ (if enabled)            │
                              └────────┬────────────────┘
                                       │
                                       ▼
                              ┌─────────────────────────┐
                              │ Return Tokens           │
                              └─────────────────────────┘
```

### Token Refresh Flow

```
┌────────────────────────────────────────┐
│  Refresh Token Request                  │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│ Validate Refresh Token                  │
│ - Signature check                       │
│ - Expiry check                          │
│ - Blacklist check (if sessions enabled) │
└────────────────┬───────────────────────┘
                 │
         [INVALID]│  [VALID]
                 │        │
                 ▼        ▼
┌────────────────────────────────────────┐
│ Token Rotation (if enabled)             │
│ - Revoke old refresh token              │
│ - Generate new refresh token            │
│ - Update token family                   │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│ Generate New Access Token               │
│ - Same user ID                          │
│ - Same roles                            │
│ - Fresh expiry                          │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│ Return Token Pair                       │
└────────────────────────────────────────┘
```

### CSRF Protection

```
┌────────────────────────────────────────┐
│    State-Changing Request               │
│    (POST, PUT, DELETE, PATCH)           │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│ Extract CSRF Token                      │
│ - From X-CSRF-Token header              │
│ - From request body                     │
└────────────────┬───────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────┐
│ Verify Against Cookie                   │
│ - Compare with csrf_token cookie        │
│ - Timing-safe comparison                │
└────────────────┬───────────────────────┘
                 │
         [FAIL]  │  [SUCCESS]
                 │        │
         403     │        ▼
         Forbidden        Continue
```

---

## Design Principles

### 1. Static Modular Composition (Django Philosophy in Rust)

**Goal:** Compile-time verification of the entire application structure.

**Implementation:**
- **Trait-Based Apps:** Every feature implements `ChopinModule` trait
- **Explicit Registration:** No runtime magic - modules are composed in `main.rs`
- **Hub-and-Spoke Model:** All apps depend on thin `chopin-core`, preventing circular dependencies
- **MVSR Structure:** Model-View-Service-Router separates HTTP from business logic

**The "Composer" Pattern:**

```rust
// Explicit, type-safe composition in main.rs
App::new().await?
    .mount_module(BlogModule::new())
    .mount_module(AuthModule::new())
    .run().await?;
```

**Why This Wins:**
- ✅ Route conflicts caught at compile-time
- ✅ Missing module configurations cause compiler errors
- ✅ No `ImportError` or `KeyError` in production
- ✅ Feature-first folders (Django comfort)
- ✅ 100% unit-testable (services don't depend on HTTP)

### 2. MVSR Pattern: Separation of Concerns

**Structure:**

```
app/
├── models.rs      # M: Database entities (SeaORM)
├── services.rs    # S: Business logic (pure Rust functions)
├── handlers.rs    # V: HTTP extraction/response (Axum handlers)
└── routes.rs      # R: Router configuration (paths → handlers)
```

**Example:**

```rust
// services.rs - Pure business logic, 100% unit-testable
pub async fn get_tenant_posts(
    db: &DatabaseConnection,
    tenant_id: i32,
    page: u64,
) -> Result<Vec<Post>, ChopinError> {
    Post::find()
        .filter(post::Column::TenantId.eq(tenant_id))
        .paginate(db, page)
        .fetch_page(page)
        .await
        .map_err(Into::into)
}

// handlers.rs - HTTP concerns only
pub async fn list_posts(
    State(state): State<AppState>,
    Pagination { page, per_page }: Pagination,
) -> Result<ApiResponse<Vec<PostDto>>, ChopinError> {
    let posts = services::get_posts(&state.db, page, per_page).await?;
    Ok(ApiResponse::success(posts))
}
```

### 3. Zero Configuration Philosophy

**Goal:** Working application with sensible defaults, no config files required.

**Implementation:**
- Environment variables for all configuration
- Secure defaults for security features (all enabled)
- Auto-migration on startup
- In-memory fallbacks (SQLite, cache)

**Example:**

```rust
// This just works - no config files needed
let app = App::new().await?;
app.run().await?;
```

### 5. Progressive Enhancement with Modules

**Principle:** Start simple, add modules as needed.

**Levels:**

```rust
// Level 1: Bare minimum (built-in routes only)
App::new().await?.run().await?;

// Level 2: Add official modules from vendor/
App::new().await?
    .mount_module(AuthModule::new())
    .run().await?;

// Level 3: Add custom feature modules
App::new().await?
    .mount_module(AuthModule::new())
    .mount_module(BlogModule::new())
    .mount_module(BillingModule::new())
    .run().await?;

// Level 4: Add FastRoute for ultra-performance endpoints
App::new().await?
    .mount_module(BlogModule::new())
    .fast_route(FastRoute::json("/health", br#"{"status":"ok"}"#))
    .run().await?;

// Level 5: Enable all performance features
// REUSEPORT=true cargo run --release --features perf
```

### 3. Per-Route Trade-offs

**Philosophy:** Different endpoints have different requirements.

**Trade-off Matrix:**

| Endpoint | Performance Need | Feature Need | Solution |
|----------|-----------------|--------------|----------|
| `/health` | Critical | None | FastRoute bare |
| `/api/status` | High | CORS | FastRoute + `.cors()` |
| `/api/posts` | Medium | Auth, validation | Axum Router |
| `/api/admin` | Low | Full security | Axum + middleware |

**Implementation:**

```rust
app
    // Ultra-fast health check
    .fast_route(FastRoute::text("/health", b"OK"))
    
    // Fast with CORS
    .fast_route(
        FastRoute::json("/api/status", br#"{"status":"ok"}"#)
            .cors()
    )
    
    // Full features
    .route("/api/posts", get(list_posts))
    .route("/api/admin", get(admin_panel)
        .layer(auth_middleware));
```

### 4. Batteries Included

**Principle:** Common production needs should be built-in, not external crates.

**Included:**
- ✅ Authentication (JWT + Argon2id)
- ✅ Authorization (RBAC)
- ✅ Database ORM (SeaORM)
- ✅ Caching (Memory/Redis)
- ✅ File Storage (Local/S3)
- ✅ OpenAPI docs
- ✅ Testing utilities
- ✅ Rate limiting
- ✅ Session management
- ✅ 2FA/TOTP

**Not Included (use external crates):**
- ❌ Email sending (use `lettre`)
- ❌ WebSockets (use `axum` directly)
- ❌ Server-side rendering (use `askama`)
- ❌ Job queues (use `sidekiq.rs`)

### 5. Performance Without Compromise

**Principle:** Fast by default, no opt-in required for basic performance.

**Always On:**
- Connection pooling
- Request ID propagation
- Compression (when beneficial)
- Keep-alive connections

**Opt-In for Maximum Speed:**
- FastRoute (`app.fast_route()`)
- SO_REUSEPORT (`REUSEPORT=true`)
- Performance features (`--features perf`)

### 6. Security by Default

**Principle:** Secure unless explicitly disabled.

**Default State:**
- ✅ 2FA enabled
- ✅ Rate limiting enabled
- ✅ Account lockout enabled
- ✅ Refresh token rotation enabled
- ✅ Session management enabled
- ✅ CSRF protection enabled
- ✅ Device tracking enabled

**Opt-Out:**

```bash
# Disable specific features via environment
SECURITY_2FA=false
SECURITY_RATE_LIMIT=false
SECURITY_CSRF=false
```

---

## Technology Stack

### Core Dependencies

```
Runtime & Async
├─ tokio              - Async runtime
├─ hyper              - HTTP implementation
└─ tower              - Middleware framework

Web Framework
├─ axum               - Web application framework
├─ tower-http         - HTTP middleware
└─ hyper-util         - HTTP utilities

Database
├─ sea-orm            - ORM (SQLite, PostgreSQL, MySQL)
└─ sea-orm-migration  - Schema migrations

Serialization
├─ serde              - Serialization framework
├─ serde_json         - JSON (default)
└─ sonic-rs           - SIMD JSON (with perf feature)

Security
├─ argon2             - Password hashing
├─ jsonwebtoken       - JWT tokens
├─ totp-rs            - 2FA/TOTP
├─ ring               - Cryptographic primitives
└─ rand               - Secure random generation

API Documentation
├─ utoipa             - OpenAPI 3.0 code generation
└─ utoipa-scalar      - Scalar UI

Optional Features
├─ redis              - Redis cache client
├─ aws-sdk-s3         - S3-compatible storage
├─ async-graphql      - GraphQL server
└─ mimalloc           - High-performance allocator
```

### Compatibility

**Rust Version:** 1.75+

**Database Support:**
- SQLite 3.35+
- PostgreSQL 12+
- MySQL 8.0+

**Cache Support:**
- In-memory (built-in)
- Redis 6.0+

**Storage Support:**
- Local filesystem
- AWS S3
- Cloudflare R2
- MinIO
- Any S3-compatible service

---

## Directory Structure

### Framework Layout (Hub-and-Spoke)

```
chopin/
├── Cargo.toml                 # Workspace definition
├── README.md
├── LICENSE
├── CONTRIBUTING.md
├── ARCHITECTURE.md            # This document
│
├── chopin-core/               # The Hub: Minimal core framework
│   ├── Cargo.toml
│   ├── README.md
│   ├── src/
│   │   ├── lib.rs            # Public API exports
│   │   ├── prelude.rs        # Convenience imports
│   │   │
│   │   ├── app.rs            # Application lifecycle & ChopinModule trait
│   │   ├── server.rs         # HTTP server & FastRoute
│   │   ├── config.rs         # Configuration
│   │   ├── routing.rs        # Route builders
│   │   │
│   │   ├── db.rs             # Database connection
│   │   ├── cache.rs          # Cache abstraction
│   │   ├── storage.rs        # File storage
│   │   │
│   │   ├── error.rs          # Error types
│   │   ├── response.rs       # Response builders
│   │   ├── json.rs           # JSON utilities
│   │   ├── logging.rs        # Logging setup
│   │   ├── perf.rs           # Performance utils
│   │   ├── testing.rs        # Test utilities
│   │   │
│   │   ├── openapi.rs        # OpenAPI generation
│   │   ├── graphql.rs        # GraphQL integration (optional)
│   │   │
│   │   ├── extractors/       # Core extractors
│   │   │   ├── mod.rs
│   │   │   ├── json.rs
│   │   │   └── pagination.rs
│   │   │
│   │   └── shared/           # Shared types for modules
│   │       ├── mod.rs
│   │       └── user.rs       # User trait & types
│   │
│   └── tests/                # Core framework tests
│
├── vendor/                   # Official "First-Party" Modules
│   ├── chopin_auth/          # Built-in but Opt-in: Identity & Auth
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs        # Implements ChopinModule
│   │   │   ├── routes.rs     # /auth/login, /auth/signup, etc.
│   │   │   ├── handlers.rs   # HTTP handlers
│   │   │   ├── services.rs   # Auth business logic
│   │   │   ├── models.rs     # User, Session, RefreshToken
│   │   │   ├── jwt.rs
│   │   │   ├── password.rs
│   │   │   ├── totp.rs
│   │   │   ├── csrf.rs
│   │   │   ├── rate_limit.rs
│   │   │   ├── lockout.rs
│   │   │   ├── refresh.rs
│   │   │   ├── session.rs
│   │   │   └── device_tracking.rs
│   │   └── tests/
│   │
│   ├── chopin_admin/         # Optional: Django-style admin panel
│   └── chopin_cms/           # Optional: Content management
│
├── chopin-cli/               # CLI tool for project scaffolding
│   ├── Cargo.toml
│   └── src/
│       └── main.rs           # chopin new, chopin add-module, etc.
│
├── chopin-examples/          # Example projects
│   ├── hello-world/
│   ├── basic-api/            # Shows modular MVSR structure
│   ├── benchmark/
│   └── performance-mode/
│
└── docs/                     # Documentation
    ├── index.html
    ├── tutorial.html
    ├── modular-architecture.md   # Guide to ChopinModule
    └── ...
```

### Application Layout (User Project with Modules)

```
my_chopin_project/           # User's application
├── Cargo.toml               # Workspace with custom apps
├── src/
│   └── main.rs              # The "Composer": Mounts all modules
│
├── apps/                    # Custom Feature Modules (Django-style)
│   ├── blog/                # Feature: Blog functionality
│   │   ├── mod.rs           # Implements ChopinModule trait
│   │   ├── routes.rs        # Scoped: /blog/posts, /blog/categories
│   │   ├── handlers.rs      # View: HTTP extraction & responses
│   │   ├── services.rs      # Logic: "Get posts, create post"
│   │   ├── models.rs        # Data: Post, Category, Comment
│   │   └── dto.rs           # DTOs: PostResponse, CreatePostRequest
│   │
│   └── billing/             # Feature: Subscription management
│       ├── mod.rs
│       ├── routes.rs
│       ├── handlers.rs
│       ├── services.rs
│       └── models.rs
│
├── shared/                  # Shared types across custom apps
│   ├── mod.rs
│   └── permissions.rs       # Custom permission logic
│
├── migrations/              # Application-level migrations
│   └── m*.rs
│
└── tests/                   # Integration tests
    ├── blog_tests.rs
    └── ...
```

**Key Structural Benefits:**

1. **Django's Feature-First Folders:** Everything related to "Blog" lives in `apps/blog/`
2. **Rust's Compile-Time Safety:** Missing route registration or module dependency = compiler error
3. **No Circular Dependencies:** All apps depend on `chopin-core` (hub), never on each other
4. **Plug-and-Play Modules:** Auth is in `vendor/chopin_auth/`, completely optional but battle-tested
5. **100% Unit-Testable:** Services are pure Rust functions, handlers are thin HTTP adapters

---

## Extension Points

### 1. Custom Authentication

```rust
use chopin_core::{App, ChopinError};
use axum::middleware;

async fn custom_auth_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, ChopinError> {
    // Your custom auth logic
    let token = req.headers().get("X-Custom-Token");
    // ...
    next.run(req).await
}

let app = App::new().await?
    .layer(middleware::from_fn(custom_auth_middleware));
```

### 2. Custom Cache Backend

```rust
use chopin_core::cache::{CacheBackend, CacheService};

struct MyCustomCache;

#[async_trait::async_trait]
impl CacheBackend for MyCustomCache {
    async fn get(&self, key: &str) -> Result<Option<String>, ChopinError> {
        // Custom implementation
    }
    // ... implement other methods
}

let cache = CacheService::new(MyCustomCache);
let app = App::with_config(config.with_cache(cache)).await?;
```

### 3. Custom Storage Backend

```rust
use chopin_core::storage::StorageBackend;

struct MyCustomStorage;

#[async_trait::async_trait]
impl StorageBackend for MyCustomStorage {
    async fn store(&self, ...) -> Result<UploadedFile, ChopinError> {
        // Custom implementation
    }
    // ... implement other methods
}
```

### 4. Custom Middleware

```rust
use axum::middleware::Next;
use axum::response::Response;
use hyper::Request;

async fn my_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    // Pre-processing
    let start = Instant::now();
    
    let response = next.run(req).await;
    
    // Post-processing
    let elapsed = start.elapsed();
    tracing::info!("Request took {:?}", elapsed);
    
    response
}

app.layer(middleware::from_fn(my_middleware));
```

### 5. Custom Extractors

```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use async_trait::async_trait;

pub struct CustomExtractor {
    pub value: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for CustomExtractor
where
    S: Send + Sync,
{
    type Rejection = ChopinError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Extract custom data from request
        Ok(CustomExtractor {
            value: "extracted".to_string(),
        })
    }
}

// Usage in handlers
async fn handler(CustomExtractor { value }: CustomExtractor) -> String {
    value
}
```

### 6. Custom OpenAPI Documentation

```rust
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(my_handler),
    components(schemas(MyRequest, MyResponse)),
)]
struct MyApiDoc;

app.openapi(MyApiDoc::openapi());
```

---

## Future Architecture Considerations

### Planned Features

1. **HTTP/2 Support**
   - Multiplexing for concurrent requests
   - Server push for critical resources
   - Backward compatible with HTTP/1.1

2. **gRPC Integration**
   - Protocol Buffers schema generation
   - Bi-directional streaming
   - Shared types with REST API

3. **WebSocket Support**
   - Real-time communication
   - Pub/sub abstractions
   - Connection state management

4. **Distributed Tracing**
   - OpenTelemetry integration
   - Jaeger/Zipkin exporters
   - Request correlation

5. **Metrics & Observability**
   - Prometheus metrics
   - Custom business metrics
   - Health check endpoints

6. **Job Queue Integration**
   - Background job processing
   - Scheduled tasks
   - Retry mechanisms

### Performance Roadmap

1. **io_uring Support** (Linux 5.19+)
   - Zero-copy I/O
   - Reduced syscall overhead
   - ~30% latency improvement

2. **HTTP/3 / QUIC**
   - UDP-based transport
   - 0-RTT connection establishment
   - Improved mobile performance

3. **JIT Route Compilation**
   - Compile hot paths to native code
   - Eliminate interpreter overhead
   - ~50% speedup for complex routes

4. **SIMD Request Parsing**
   - Vectorized header parsing
   - Fast path detection
   - ~40% faster request parsing

---

## Conclusion

Chopin's architecture is designed around four core principles:

1. **Modularity** - Django's feature-first philosophy with Rust's compile-time verification via `ChopinModule` trait
2. **Safety** - Hub-and-spoke dependency model eliminates circular dependencies and runtime errors
3. **Performance** - Extreme optimization through FastRoute, SO_REUSEPORT, and zero-allocation design while maintaining modularity
4. **Multi-tenancy** - Data isolation built into the type system via extractors, not an afterthought

### The Chopin Promise

**"Django's comfort, Rust's safety, production-grade performance."**

The framework achieves:
- ✅ **650K+ req/s** throughput with modular architecture
- ✅ **Compile-time route verification** - no `KeyError` at 3 AM
- ✅ **Feature-first structure** - everything related to "Blog" lives in `apps/blog/`
- ✅ **Zero circular dependencies** - enforced by hub-and-spoke model
- ✅ **100% unit-testable** - MVSR separates HTTP from business logic
- ✅ **Batteries included, not hard-coded** - Auth, Admin, CMS are official modules you opt into

By combining static modular composition with extreme performance optimization, Chopin enables teams to build production-ready APIs with confidence - where configuration errors are caught before deployment, not discovered in production.

---

## References

- [Chopin GitHub Repository](https://github.com/kowito/chopin)
- [Chopin Documentation](https://kowito.github.io/chopin/)
- [Benchmark Methodology](https://github.com/kowito/chopin/tree/main/chopin-examples/benchmark)
- [Axum Documentation](https://docs.rs/axum/)
- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [Tower Middleware](https://docs.rs/tower/)
