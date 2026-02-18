# 🎹 Chopin

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

**Django meets Rust.** Chopin brings Django's "batteries-included" philosophy to the world of high-performance systems programming. Build **modular, type-safe APIs** at **650K+ req/s** with compile-time verification and zero circular dependencies.

```rust
// Explicit, type-safe composition
App::new().await?
    .mount_module(AuthModule::new())     // vendor/chopin_auth
    .mount_module(BlogModule::new())     // apps/blog
    .mount_module(BillingModule::new())  // apps/billing
    .run().await?;
```

```bash
# Get started in 60 seconds
cargo install chopin-cli
chopin new my-api && cd my-api
REUSEPORT=true cargo run --release --features perf

# Your API is now serving 650K+ req/s 🚀
```

---

## 🏆 Why Chopin?

### ⚡ Blazing Fast Performance

**Benchmarked against 7 industry-leading frameworks across Rust, JavaScript, TypeScript, and Python:**

```
JSON Throughput Benchmark (req/s @ 256 connections)
┌──────────────────────────────────────────────────────────────────┐
│ Chopin         ████████████████████████████████████████  657,152 │ 🏆 FASTEST
│ may-minihttp   ███████████████████████████████████████   642,795 │ (Rust)
│ Axum           ██████████████████████████████████        607,807 │ (Rust)
│ Express        ██████████████                            289,410 │ (Node.js)
│ Hono (Bun)     ████████████                              243,177 │ (Bun)
│ FastAPI        ███████                                   150,082 │ (Python)
│ NestJS         ████                                       80,890 │ (Node.js)
└──────────────────────────────────────────────────────────────────┘

Average Latency @ 256 connections (lower is better)
┌──────────────────────────────────────────────────────────────────┐
│ may-minihttp   ████                                        452µs │ 🏆 LOWEST
│ Chopin         █████                                       612µs │ 🏆 BEST OVERALL
│ Axum           ██████                                      690µs │ (Rust)
│ Express        ███████████                                1,140µs │ (Node.js)
│ Hono (Bun)     █████████████                              1,330µs │ (Bun)
│ FastAPI        ███████████████████                        1,920µs │ (Python)
│ NestJS         █████████████████████████████████████     3,730µs │ (Node.js)
└──────────────────────────────────────────────────────────────────┘

99th Percentile Latency (lower is better)
┌──────────────────────────────────────────────────────────────────┐
│ may-minihttp   ████                                      3.66ms  │ 🏆 LOWEST
│ Chopin         ████                                      3.75ms  │ 🏆 BEST OVERALL
│ Axum           █████                                     4.24ms  │ (Rust)
│ Express        ███████                                   5.64ms  │ (Node.js)
│ Hono (Bun)     ████████                                  6.87ms  │ (Bun)
│ FastAPI        █████████                                 7.59ms  │ (Python)
│ NestJS         █████████████████████                    17.02ms  │ (Node.js)
└──────────────────────────────────────────────────────────────────┘
```

**[→ See full benchmark report with cost analysis](https://kowito.github.io/chopin/)**

**What this means:**
- 🏆 **#1 JSON throughput** — 657K req/s (handle 57 billion requests/day on one server)
- 🏆 **Best overall latency** — 612µs average, 3.75ms p99 (optimal for production)
- ✅ **2.3x faster than Express** (most popular Node.js framework)
- ✅ **2.7x faster than Hono/Bun** (despite Bun's speed claims)
- ✅ **4.4x faster than FastAPI** (best Python async framework)
- ✅ **8.1x faster than NestJS** (enterprise TypeScript framework)
- 💰 **Save $16,800/year** vs Node.js, $33,600/year vs NestJS

### 🎁 Django's Comfort, Rust's Safety

**Modular Architecture:**
- **ChopinModule Trait** — Every feature (Auth, Blog, Billing) is a self-contained module
- **Hub-and-Spoke** — Thin `chopin-core` hub prevents circular dependencies
- **MVSR Pattern** — Model-View-Service-Router separates HTTP from business logic
- **Compile-Time Verified** — Route conflicts and missing configs caught before deployment

**Batteries Included (But Not Hard-Coded):**

| Feature | Status | Description |
|---------|--------|-------------|
| **Auth Module** | ✅ Opt-in | JWT + Argon2id, 2FA/TOTP, rate limiting, refresh tokens (vendor/chopin_auth) || **RBAC Permissions** | ✅ Core | Database-configurable role-based access control with caching || **Database ORM** | ✅ Core | SeaORM with auto-migrations (SQLite/PostgreSQL/MySQL) |
| **OpenAPI Docs** | ✅ Core | Auto-generated Scalar UI at `/api-docs` |
| **Admin Panel** | 🔜 Opt-in | Django-style admin interface (vendor/chopin_admin) |
| **CMS Module** | 🔜 Opt-in | Content management system (vendor/chopin_cms) |
| **Caching** | ✅ Core | In-memory or Redis support |
| **File Storage** | ✅ Core | Local filesystem or S3-compatible (R2, MinIO) |
| **GraphQL** | ✅ Core | Optional async-graphql integration |
| **Testing Utils** | ✅ Core | `TestApp` with in-memory SQLite |
| **FastRoute** | ✅ Core | Zero-alloc static responses (~35ns/req) |

**Translation:** Django's feature-first folders + Rust's compile-time safety = No `KeyError` at 3 AM.

### 💰 Real Cost Savings

**Before Chopin (Node.js/TypeScript):**
- 10 servers @ $200/mo = **$2,000/month**
- Handling 200K req/s
- 5-10ms p99 latency

**After Chopin:**
- 3 servers @ $200/mo = **$600/month**
- Handling 1.9M req/s (2x traffic!)
- 3.75ms p99 latency

**💰 Savings: $16,800/year**

---

## 🚀 Quick Start

### Installation

```bash
# Install the CLI
cargo install chopin-cli

# Create a new modular project
chopin new my-blog-api
cd my-blog-api

# Run in development mode
cargo run

# Run with maximum performance (SO_REUSEPORT multi-core)
REUSEPORT=true cargo run --release --features perf
```

### Your First Modular App (2 minutes)

**Step 1: Create a Blog Module**

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

**Step 2: Compose Your Application**

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

### With Authentication & RBAC

```rust
use chopin_core::prelude::*;

// Simple login required
#[login_required]
async fn get_profile() -> Result<Json<ApiResponse<UserProfile>>, ChopinError> {
    // __chopin_auth is auto-injected
    Ok(Json(ApiResponse::success(UserProfile {
        id: __chopin_auth.user_id.clone(),
        email: __chopin_auth.email.clone(),
    })))
}

// Permission-based access control
#[permission_required("can_publish_post")]
async fn publish_post(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
) -> Result<Json<ApiResponse<PostResponse>>, ChopinError> {
    // Only users with "can_publish_post" permission can access
    let post = services::publish(&state.db, post_id, &__chopin_auth.user_id).await?;
    Ok(Json(ApiResponse::success(post)))
}

// Fine-grained permission checks
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    chopin_core::init_logging();
    
    let app = App::new().await?  // AuthModule + RBAC included
        .route("/profile", get(get_profile))
        .route("/posts/:id/publish", put(publish_post))
        .route("/reports", get(get_report));
    
    app.run().await?;
    Ok(())
}
```

**RBAC Features:**
- ✅ `#[login_required]` — Enforces JWT validation
- ✅ `#[permission_required("codename")]` — Enforces permission checks
- ✅ `PermissionGuard` extractor — Fine-grained conditional permission checks
- ✅ Database-configurable — Create/assign permissions at runtime without redeploying
- ✅ In-memory cache (5-min TTL) — Zero DB overhead for repeated checks
- ✅ Superuser bypass — `role = "superuser"` always passes all checks

**Built-in endpoints** (no code required):
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

### Production Security (Enabled by Default)

Chopin ships with **9 production security features**, all enabled by default. No extra setup — just deploy:

| Feature | Endpoint / Mechanism | Description |
|---------|---------------------|-------------|
| **2FA/TOTP** | `POST /api/auth/totp/setup`, `/enable`, `/disable` | Google Authenticator compatible |
| **Rate Limiting** | Automatic on login | 5 attempts per 5 min (configurable) |
| **Account Lockout** | Automatic on login | Locks after 5 failed attempts for 15 min |
| **Refresh Tokens** | `POST /api/auth/refresh` | Automatic rotation with reuse detection |
| **Session Management** | `POST /api/auth/logout` | Server-side sessions, revoke one or all |
| **Password Reset** | `POST /api/auth/password-reset/request`, `/confirm` | Secure token-based flow |
| **Email Verification** | `POST /api/auth/verify-email` | Required on signup when enabled |
| **CSRF Protection** | Automatic | Token issued on login, verified on mutations |
| **IP/Device Tracking** | Automatic | Audit log of all login events |

**Configure via environment variables:**
```bash
# Toggle features on/off
SECURITY_2FA=true
SECURITY_RATE_LIMIT=true
SECURITY_ACCOUNT_LOCKOUT=true
SECURITY_REFRESH_TOKENS=true
SECURITY_SESSION_MANAGEMENT=true
SECURITY_PASSWORD_RESET=true
SECURITY_EMAIL_VERIFICATION=true
SECURITY_CSRF=true
SECURITY_DEVICE_TRACKING=true

# Tune parameters
SECURITY_RATE_LIMIT_MAX=5            # Max attempts per window
SECURITY_RATE_LIMIT_WINDOW=300       # Window in seconds (5 min)
SECURITY_LOCKOUT_MAX=5               # Failed attempts before lockout
SECURITY_LOCKOUT_DURATION=900        # Lockout duration in seconds (15 min)
SECURITY_REFRESH_EXPIRY_DAYS=30      # Refresh token lifetime
SECURITY_RESET_EXPIRY=3600           # Password reset token TTL (1 hr)
SECURITY_EMAIL_VERIFY_EXPIRY=86400   # Email verification TTL (24 hrs)
SECURITY_MIN_PASSWORD_LENGTH=12      # Minimum password length
```

### With Database

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

**Database configured via `.env`:**
```bash
DATABASE_URL=sqlite://database.db?mode=rwc
# Or PostgreSQL: postgresql://user:pass@localhost/dbname
# Or MySQL: mysql://user:pass@localhost/dbname
```

---

## 📊 Real-World Use Cases

### ✅ Fintech APIs
- **Low latency** (612µs avg) for trading platforms
- **High throughput** (657K req/s) for payment processing
- **Built-in auth** for secure financial transactions

### ✅ Gaming Backends
- **3.75ms p99** for real-time multiplayer
- **Predictable performance** under load spikes
- **WebSocket support** via Axum ecosystem

### ✅ Microservices
- **Lightweight** — small binary size, fast cold starts
- **High-scale** internal APIs (millions of requests/day)
- **OpenAPI** for auto-generated client SDKs

### ✅ SaaS Platforms
- **Production features** out of the box (auth, DB, file uploads)
- **50%+ cost savings** vs Node.js/Python
- **Ship faster** — no framework integration hell

---

## 🔥 The Secret Sauce

Chopin achieves extreme performance through:

1. **Unified ChopinService** — Raw hyper HTTP/1.1 dispatcher with FastRoute zero-alloc fast path
2. **Per-route trade-offs** — Choose per-path: `.cors()`, `.cache_control()`, `.get_only()`, `.header()` — all pre-computed, zero per-request cost
3. **sonic-rs SIMD** — 40% faster JSON serialization via AVX2/NEON instructions
3. **mimalloc** — Microsoft's high-concurrency allocator (better than jemalloc)
4. **Zero-alloc Bodies** — `ChopinBody` avoids `Box::pin` overhead
5. **Cached Headers** — Lock-free Date header updated every 500ms via `AtomicU64`
6. **CPU-specific Builds** — Native SIMD instructions for your hardware

**Enable with:**
```bash
REUSEPORT=true cargo run --release --features perf
```

This gives you:
- **SO_REUSEPORT** — N workers (one per CPU core) with per-core tokio runtimes
- **TCP_NODELAY** — Disable Nagle's algorithm for lower latency
- **FastRoute** — Zero-alloc static responses with per-route CORS, Cache-Control, and method filtering
- **mimalloc** globally enabled
- **sonic-rs** for all JSON operations (vs serde_json)

---

## 💡 Migration from Axum

Chopin is built on Axum — **7% faster with zero breaking changes:**

```rust
// Before (Axum)
use axum::{Router, routing::get};

let app = Router::new()
    .route("/users", get(list_users));

let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, app).await?;

// After (Chopin) — 7% faster + auth + DB + OpenAPI
use chopin_core::{App, get, Json};

let app = App::new().await?  // Auto auth + DB + OpenAPI
    .route("/users", get(list_users));

app.run().await?;
```

**What you get:**
- ✅ All Axum extractors and middleware work unchanged
- ✅ Full Tower/hyper compatibility
- ✅ 7% higher throughput + 12% lower latency
- ✅ Built-in auth, database, OpenAPI, caching, file uploads

---

## 📚 Documentation

- **[Website & Tutorial](https://kowito.github.io/chopin/)** — Getting started, full tutorial, and architecture overview
- **[Debugging & Logging](docs/debugging-and-logging.md)** — Enable request logging and debugging (important for development!)
- **[Examples](chopin-examples/)** — Hello world, CRUD API, benchmarks
- **[API Docs (docs.rs)](https://docs.rs/chopin)** — Complete Rust API reference

---

## 🎯 Examples

Check out the [`chopin-examples/`](chopin-examples/) directory:

| Example | Description |
|---------|-------------|
| **[hello-world](chopin-examples/hello-world/)** | Minimal Chopin API (3 lines of code) |
| **[basic-api](chopin-examples/basic-api/)** | CRUD API with auth and database |
| **[performance-mode](chopin-examples/performance-mode/)** | Maximum throughput configuration |
| **[benchmark](chopin-examples/benchmark/)** | TechEmpower-style benchmarks |

---

## 🤝 Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

**Areas we'd love help with:**
- More database adapters (MongoDB, DynamoDB)
- WebSocket examples and utilities
- gRPC integration
- Benchmark improvements
- Documentation and examples

---

## ⚖️ License

**WTFPL** (Do What The Fuck You Want To Public License)

See [LICENSE](LICENSE) for details.

---

## 🌟 Star History

If Chopin helps you build faster, more efficient APIs, **give us a star** ⭐ on GitHub!

---

**Ready to build the fastest API of your career?**

```bash
cargo install chopin-cli
chopin new my-api
cd my-api
REUSEPORT=true cargo run --release --features perf
```

**[Website](https://kowito.github.io/chopin/) • [Tutorial](https://kowito.github.io/chopin/tutorial.html) • [Examples](chopin-examples/) • [Discord](https://discord.gg/chopin)**

---

<p align="center">
  Made with 🎹 by the Chopin team<br>
  <em>High-fidelity engineering for the modern virtuoso</em>
</p>