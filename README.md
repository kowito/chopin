# 🎹 Chopin

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin)](https://crates.io/crates/chopin)
[![Downloads](https://img.shields.io/crates/d/chopin.svg)](https://crates.io/crates/chopin)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

**The fastest production-ready Rust web framework.** Chopin delivers **650K+ req/s** for JSON APIs with **sub-millisecond latency** — all while giving you built-in auth, database, OpenAPI, and caching out of the box.

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

### 🎁 Production-Ready from Day 1

Unlike bare-metal frameworks, Chopin ships with everything you need:

| Feature | Chopin | Axum | Description |
|---------|--------|------|-------------|
| **Built-in Auth** | ✅ | ❌ | JWT + Argon2id with signup/login endpoints |
| **Database ORM** | ✅ | ❌ | SeaORM with auto-migrations (SQLite/PostgreSQL/MySQL) |
| **OpenAPI Docs** | ✅ | ❌ | Auto-generated Scalar UI at `/api-docs` |
| **Role-Based Access** | ✅ | ❌ | User, Moderator, Admin with extractors |
| **Caching** | ✅ | ❌ | In-memory or Redis support |
| **File Uploads** | ✅ | ❌ | Local filesystem or S3-compatible (R2, MinIO) |
| **GraphQL** | ✅ | ❌ | Optional async-graphql integration |
| **Testing Utils** | ✅ | Partial | `TestApp` with in-memory SQLite |
| **FastRoute** | ✅ | ❌ | Zero-alloc static responses with per-route decorators (.cors(), .cache_control(), .methods()) |
| **Axum Compatible** | ✅ | ✅ | Use any Tower/hyper middleware |

**Translation:** Prototype in 10 minutes. Deploy to production on day 1.

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

# Create a new project
chopin new my-api
cd my-api

# Run in development mode
cargo run

# Run with maximum performance (SO_REUSEPORT multi-core)
REUSEPORT=true cargo run --release --features perf
```

### Your First API (90 seconds)

```rust
use chopin::{App, Router, ApiResponse, get, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

// Simple handler
async fn hello() -> &'static str {
    "Hello, World!"
}

// JSON response
async fn get_user() -> ApiResponse<User> {
    ApiResponse::success(User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    })
}

// JSON extraction
async fn create_user(Json(user): Json<User>) -> ApiResponse<User> {
    // Auto-validation, database access, etc.
    ApiResponse::success(user)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?
        .route("/", get(hello))
        .route("/users/:id", get(get_user))
        .route("/users", post(create_user));
    
    app.run().await?;
    Ok(())
}
```

**That's it!** You now have:
- ✅ JSON serialization (with SIMD via sonic-rs in perf mode)
- ✅ Auto-generated OpenAPI docs at `/api-docs`
- ✅ Built-in auth endpoints at `/api/auth/signup` and `/api/auth/login`
- ✅ Database connection (via `.env` configuration)
- ✅ Graceful shutdown
- ✅ Request logging

### With Authentication

```rust
use chopin::{App, ApiResponse, get, middleware::RequireAuth, extractors::AuthUser};

async fn protected_route(user: AuthUser) -> ApiResponse<String> {
    ApiResponse::success(format!("Hello, {}! Your user ID is {}", user.username, user.id))
}

async fn admin_only(user: AuthUser) -> ApiResponse<&'static str> {
    // Automatically enforced by RequireAuth middleware with Role::Admin
    ApiResponse::success("Welcome, admin!")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?
        .route("/protected", get(protected_route).layer(RequireAuth::any()))
        .route("/admin", get(admin_only).layer(RequireAuth::admin()));
    
    app.run().await?;
    Ok(())
}
```

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

# Returns: {"token":"eyJ0eXAi..."}
```

### With Database

```rust
use chopin::{App, ApiResponse, get, database::DatabaseConnection};
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
use chopin::{App, get, Json};

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