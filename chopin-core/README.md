# 🎹 Chopin Core

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![GitHub stars](https://img.shields.io/github/stars/kowito/chopin.svg)](https://github.com/kowito/chopin)

**Django meets Rust**  
The composable web framework that gets out of your way.

A high-performance modular Rust web framework optimized for 650K+ req/s throughput. Built on Axum and SeaORM with zero-cost abstraction, Django-inspired modularity, and compile-time verification.

## Features

### Modular Architecture
- **ChopinModule trait** — Composable modules that self-register routes, services, and migrations
- **Hub-and-spoke design** — No circular dependencies, explicit module composition
- **MVSR Pattern** — Model-View-Service-Router separation for 100% unit-testable services

### Performance
- **FastRoute** — Zero-alloc static responses (~35ns/req) for high-traffic endpoints
- **SO_REUSEPORT** — Per-core accept loops with single-threaded tokio runtimes
- **SIMD JSON** — `sonic-rs` achieves 40% faster serialization vs serde_json
- **mimalloc** — Microsoft's high-performance allocator
- **Cached headers** — Date header updated every 500ms, lock-free

### Production Ready
- **Built-in Auth** — JWT + Argon2id, 2FA/TOTP, refresh tokens, device tracking
- **Role-Based Access Control** — User, Moderator, Admin, SuperAdmin with extractors
- **SeaORM Database** — SQLite, PostgreSQL, MySQL with auto-migrations
- **OpenAPI Docs** — Auto-generated Scalar UI at `/api-docs`
- **Caching** — In-memory or Redis support
- **File Uploads** — Local filesystem or S3-compatible (R2, MinIO)
- **Testing** — `TestApp` with in-memory SQLite and HTTP client

## Installation

```toml
[dependencies]
chopin-core = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
sea-orm = { version = "1", features = ["sqlx-sqlite", "runtime-tokio-rustls"] }
```

## Quick Start

### Simple App

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

### Modular App with ChopinModule

```rust
use chopin_core::prelude::*;

// Define a module using MVSR pattern
pub struct BlogModule;

#[async_trait]
impl ChopinModule for BlogModule {
    fn name(&self) -> &'static str {
        "blog"
    }

    async fn migrations(&self) -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20240101_create_posts::Migration)]
    }

    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/posts", get(handlers::list_posts).post(handlers::create_post))
            .route("/posts/:id", get(handlers::get_post).delete(handlers::delete_post))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    let app = App::new().await?
        .mount_module(BlogModule)  // Self-registers routes & migrations
        .build();
    
    app.run().await?;
    Ok(())
}
```

See [Modular Architecture Guide](https://github.com/kowito/chopin/blob/main/docs/modular-architecture.md) for complete details.

### With Logging and Debugging

To see request logs and debug output in your console, call `init_logging()` before creating the app:

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();  // Shows startup logs, migrations, and HTTP requests
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

For more control:
```rust
// Development: detailed logs including HTTP traces
init_logging_pretty();

// Production: JSON format for log aggregation
init_logging_json();

// Custom level
init_logging_with_level("debug");

// Or use RUST_LOG environment variable:
// RUST_LOG=debug cargo run
```

See [Debugging and Logging Guide](https://github.com/kowito/chopin/blob/main/docs/debugging-and-logging.md) for details.

## Maximum Performance

For maximum throughput, enable all performance flags:

```bash
# Enable SO_REUSEPORT multi-core + perf features
REUSEPORT=true cargo run --release --features perf
```

This enables:
- **SO_REUSEPORT** — Per-core accept loops with single-threaded tokio runtimes
- **FastRoute** — Zero-alloc static responses that bypass Axum
- **mimalloc** — Microsoft's high-performance allocator
- **sonic-rs** — SIMD-accelerated JSON (40% faster serialization vs serde_json)
- **Cached Date header** — updated every 500ms, lock-free
- **TCP_NODELAY** — disable Nagle's algorithm

## Documentation

See the [main repository](https://github.com/kowito/chopin) for comprehensive guides:

- [**Modular Architecture**](https://github.com/kowito/chopin/blob/main/docs/modular-architecture.md) — ChopinModule trait, MVSR pattern, hub-and-spoke design
- [**ARCHITECTURE.md**](https://github.com/kowito/chopin/blob/main/ARCHITECTURE.md) — Complete system design and component architecture
- [Debugging & Logging](https://github.com/kowito/chopin/blob/main/docs/debugging-and-logging.md) — Enable request logging (required for debugging!)
- [JSON Performance](https://github.com/kowito/chopin/blob/main/docs/json-performance.md) — SIMD JSON optimization guide
- [API Reference](https://docs.rs/chopin-core) — Complete API documentation

## License

WTFPL (Do What The Fuck You Want To Public License)
