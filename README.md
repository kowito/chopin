# 🎹 Chopin

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)

**Chopin: High-fidelity engineering for the modern virtuoso.**

**Last Updated:** February 2026

Chopin gives you the full-stack experience — auth, database, caching, file uploads, OpenAPI docs — with the performance to beat raw frameworks in benchmarks.

## Features

- **Triple Server Modes** — Standard (full middleware) | Performance (hyper + SO_REUSEPORT) | Raw (hyper bypassed, max speed)
- **FastRoute API** — Zero-allocation endpoints via `ChopinBody` + direct header manipulation for extreme performance
- **Built-in Auth** — JWT + Argon2id with signup/login endpoints out of the box
- **Role-Based Access** — User, Moderator, Admin, SuperAdmin with extractors and middleware
- **SeaORM Database** — SQLite, PostgreSQL, MySQL with auto-migrations
- **OpenAPI Docs** — Auto-generated Scalar UI at `/api-docs`
- **Caching** — In-memory or Redis
- **File Uploads** — Local filesystem or S3-compatible (R2, MinIO)
- **GraphQL** — Optional async-graphql integration
- **CLI** — Project scaffolding, code generation, database management
- **Testing** — `TestApp` with in-memory SQLite and HTTP client

## Quick Start

```bash
# Install the CLI
cargo install chopin-cli

# Create a new project
chopin new my-app
cd my-app

# Run the server
cargo run
```

```
🎹 Chopin server is running!
   → Mode:    standard
   → Server:  http://127.0.0.1:3000
   → API docs: http://127.0.0.1:3000/api-docs
```

## Server Modes

### Performance Mode

For maximum throughput with full Axum compatibility:

```bash
SERVER_MODE=performance cargo run --release --features perf
```

- **SO_REUSEPORT** — N accept loops (one per CPU core)
- **mimalloc** — Microsoft's high-performance allocator
- **Zero-alloc FastRoutes** — pre-baked static responses bypass Axum
- **Lock-free date cache** — thread_local + atomic epoch (8ns per request)
- **TCP_NODELAY** — disable Nagle's algorithm
- **HTTP/1.1 pipeline_flush** — immediate response flushing

### Raw Mode (NEW)

For absolute maximum throughput (benchmarks only):

```bash
SERVER_MODE=raw cargo run --release --features perf
```

- **Hyper completely bypassed** — raw TCP reads/writes
- **Pre-serialized HTTP** — only 29-byte Date header patched per request
- **~45% faster than Performance mode** — 240ns vs 450ns per request
- **Limitations:** Only FastRoute endpoints (no Axum, no middleware)
- **Best for:** TechEmpower benchmarks, >1M req/s targets

## Example

```rust
use axum::{Router, routing::get, extract::State};
use chopin_core::{App, ApiResponse, controllers::AppState};

async fn hello(State(state): State<AppState>) -> ApiResponse<String> {
    ApiResponse::success("Hello from Chopin!".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

## Server Modes

| Feature | Standard | Performance | Raw |
|---------|----------|-------------|-----|
| Ease of use | ✅ Full middleware | FastRoute + Axum | FastRoute only |
| Server | `axum::serve` | Raw `hyper::http1` | Raw TCP |
| Accept loops | 1 | N (per CPU core) | N (per CPU core) |
| FastRoute path | Through Axum | Zero-alloc hyper | Zero-alloc raw |
| Allocator | System | mimalloc | mimalloc |
| Per-request cost | ~800ns | ~450ns | ~240ns |
| Best for | Development, APIs | Production high-load | Benchmarks, >1M req/s |

## Documentation

See the [docs/](docs/README.md) directory:

- [Getting Started](docs/getting-started.md)
- [Architecture](docs/architecture.md)
- [Configuration](docs/configuration.md)
- [Controllers & Routing](docs/controllers-routing.md)
- [Models & Database](docs/models-database.md)
- [Security](docs/security.md)
- [Performance](docs/performance.md)
- [Testing](docs/testing.md)
- [CLI](docs/cli.md)
- [LLM Learning Guide](docs/llm-learning-guide.md)

## Examples

| Example | Description |
|---------|-------------|
| [hello-world](chopin-examples/hello-world/) | Minimal server — one file, zero config |
| [basic-api](chopin-examples/basic-api/) | Full CRUD API with auth, pagination, tests |
| [performance-mode](chopin-examples/performance-mode/) | FastRoute + zero-alloc JSON responses for maximum throughput |
| [benchmark](chopin-examples/benchmark/) | Performance mode showcase for benchmarking |

## Tech Stack

| Component | Library |
|-----------|---------|
| HTTP | Axum 0.8 + Hyper 1.x |
| Runtime | Tokio (multi-thread) |
| Database | SeaORM 1.x |
| JSON | sonic-rs (SIMD) |
| Auth | jsonwebtoken + argon2 |
| Docs | utoipa + Scalar |

## License

MIT
