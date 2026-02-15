# 🎹 Chopin Core

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![GitHub stars](https://img.shields.io/github/stars/kowito/chopin.svg)](https://github.com/kowito/chopin)

**Chopin: High-fidelity engineering for the modern virtuoso.**

A high-performance Rust web framework combining the ease of Axum with production-ready features like authentication, database integration, caching, and file uploads—all optimized for extreme throughput.

## Features

- **Unified ChopinService** — FastRoute zero-alloc fast path + Axum Router for all other routes
- **Per-route trade-offs** — `.cors()`, `.cache_control()`, `.methods()`, `.header()` decorators (all pre-computed, zero per-request cost)
- **SO_REUSEPORT** — Multi-core accept loops with per-core tokio runtimes (enable with `REUSEPORT=true`)
- **FastRoute API** — Zero-allocation endpoints with per-route CORS, method filtering, and custom headers
- **Built-in Auth** — JWT + Argon2id with signup/login endpoints out of the box
- **Role-Based Access Control** — User, Moderator, Admin, SuperAdmin with extractors and middleware
- **SeaORM Database** — SQLite, PostgreSQL, MySQL with auto-migrations
- **OpenAPI Docs** — Auto-generated Scalar UI at `/api-docs`
- **Caching** — In-memory or Redis support
- **File Uploads** — Local filesystem or S3-compatible (R2, MinIO)
- **GraphQL** — Optional async-graphql integration
- **Testing** — `TestApp` with in-memory SQLite and HTTP client

## Installation

```toml
[dependencies]
chopin-core = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Quick Start

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see request traces
    init_logging();
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

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

- [Debugging & Logging](https://github.com/kowito/chopin/blob/main/docs/debugging-and-logging.md) — Enable request logging (required for debugging!)
- [JSON Performance](https://github.com/kowito/chopin/blob/main/docs/json-performance.md) — SIMD JSON optimization guide
- [API Reference](https://docs.rs/chopin-core) — Complete API documentation

## License

WTFPL (Do What The Fuck You Want To Public License)
