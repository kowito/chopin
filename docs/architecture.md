# Architecture

## Overview

Chopin is a full-stack Rust web framework built on top of proven libraries:

| Layer | Library | Purpose |
|-------|---------|---------|
| HTTP server | **Axum 0.8** / **Hyper 1.x** | Request routing and middleware |
| Database ORM | **SeaORM 1.x** | Models, migrations, queries |
| Async runtime | **Tokio** | Multi-threaded async I/O |
| Serialization | **sonic-rs** | ARM NEON optimized JSON |
| Auth | **jsonwebtoken** + **argon2** | JWT tokens + password hashing |
| API docs | **utoipa** + **Scalar** | OpenAPI 3.1 auto-generation |
| Caching | In-memory / **Redis** | Key-value cache abstraction |
| Storage | Local / **AWS S3** | File upload handling |

## Workspace Structure

```
chopin/
├── chopin-core/       # The framework library
│   └── src/
│       ├── app.rs          # App struct, router builder, server runner
│       ├── config.rs       # ServerMode enum, Config from env vars
│       ├── server.rs       # Raw hyper server (performance mode)
│       ├── perf.rs         # Date header caching, perf utilities
│       ├── lib.rs          # Module exports, mimalloc allocator
│       ├── routing.rs      # Route builder (nests auth routes)
│       ├── response.rs     # ApiResponse<T> (sonic-rs serialization)
│       ├── error.rs        # ChopinError → HTTP status codes
│       ├── db.rs           # SeaORM connection pool
│       ├── cache.rs        # CacheService (in-memory + Redis)
│       ├── storage.rs      # FileUploadService (local + S3)
│       ├── openapi.rs      # OpenAPI doc definition
│       ├── graphql.rs      # async-graphql integration (optional)
│       ├── testing.rs      # TestApp, TestClient helpers
│       ├── auth/           # JWT + password hashing
│       ├── controllers/    # Built-in auth handlers + AppState
│       ├── extractors/     # AuthUser, Pagination, Role, Json
│       ├── migrations/     # Core migrations (users table)
│       └── models/         # User entity + Role enum
├── chopin-cli/        # CLI tool (chopin new, generate, etc.)
├── chopin-examples/   # Example applications
│   ├── hello-world/   # Minimal server
│   ├── basic-api/     # CRUD API with auth
│   └── benchmark/     # Performance mode showcase
└── docs/              # Documentation
```

## Dual-Mode Server Architecture

Chopin runs in one of two modes, controlled by the `SERVER_MODE` environment variable:

### Standard Mode (default)

```
CLIENT → tokio::TcpListener → axum::serve
           → CorsLayer
           → TraceLayer (dev only)
           → RequestId (dev only)
           → Router
             → /            → welcome()
             → /api/auth/*  → auth controllers
             → /api-docs    → Scalar UI
             → user routes  → your controllers
```

This is the **easy mode**. Full middleware stack, tracing, OpenAPI docs. Best for development and typical production.

### Performance Mode

```
CLIENT → SO_REUSEPORT × N CPU cores
           → per-core TcpListener (backlog 8192)
             → TCP_NODELAY
               → hyper HTTP/1.1 (keep_alive + pipeline_flush)
                 → ChopinService::call(req)
                   → /json      → 27-byte static response (ZERO alloc)
                   → /plaintext → 13-byte static response (ZERO alloc)
                   → *          → Axum Router (full middleware)
```

The **fast mode**. Key differences:

| Feature | Standard | Performance |
|---------|----------|-------------|
| Server | `axum::serve` | Raw `hyper::http1` |
| Accept loops | 1 | N (one per CPU core) |
| SO_REUSEPORT | No | Yes |
| `/json`, `/plaintext` | Through Axum | Bypass Axum entirely |
| Middleware on bench endpoints | Yes | Zero |
| Allocator | System | mimalloc (with `perf` feature) |
| Date header | Per-request | Cached (500ms refresh) |

## Request Lifecycle

### Standard Mode

1. `tokio::TcpListener::accept()` → TCP connection
2. `axum::serve` creates a hyper service
3. Middleware layers execute (CORS, tracing, request-id)
4. Axum Router matches the path
5. Handler extracts `State`, `Json`, `AuthUser`, etc.
6. Handler returns `ApiResponse<T>` or `ChopinError`
7. `IntoResponse` serializes with `sonic_rs`
8. Response sent to client

### Performance Mode

1. Kernel distributes connection to a core via SO_REUSEPORT
2. `TcpListener::accept()` on that core
3. `TCP_NODELAY` set on the socket
4. `ChopinService::call()` checks path:
   - `/json` → pre-computed static `Bytes` + cached Date header → response
   - `/plaintext` → same, zero allocation
   - Everything else → Axum Router (same as standard mode)
5. `hyper::http1` sends response with `pipeline_flush`

## AppState

All handlers share state through Axum's `State` extractor:

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,     // SeaORM connection pool
    pub config: Arc<Config>,        // Shared configuration
    pub cache: CacheService,        // Cache backend
}
```

## Feature Flags

| Feature | Cargo Flag | What it enables |
|---------|-----------|-----------------|
| Redis caching | `--features redis` | Redis-backed `CacheService` |
| GraphQL | `--features graphql` | `async-graphql` integration |
| S3 storage | `--features s3` | AWS S3 / R2 file uploads |
| Performance | `--features perf` | mimalloc global allocator |

## Compilation Profile

The workspace uses an aggressive release profile:

```toml
[profile.release]
opt-level = 3        # Maximum optimization
lto = "fat"          # Full link-time optimization
codegen-units = 1    # Single codegen unit (slower compile, faster binary)
strip = true         # Strip debug symbols
panic = "abort"      # No unwinding (smaller binary)
```

Combined with `.cargo/config.toml` targeting native CPU features:

```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]
```
