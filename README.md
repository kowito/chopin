# 🎹 Chopin (v0.1.1)

**The high-performance Rust web framework for perfectionists with deadlines.**

**Current Version:** 0.1.1 | **Last Updated:** February 2026

Chopin gives you the full-stack experience — auth, database, caching, file uploads, OpenAPI docs — with the performance to beat raw frameworks in benchmarks.

## Features

- **Dual Server Modes** — Standard (easy, full middleware) or Performance (raw hyper, SO_REUSEPORT, zero-alloc)
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

## Performance Mode

For maximum throughput:

```bash
SERVER_MODE=performance cargo run --release --features perf
```

This enables:
- **SO_REUSEPORT** — N accept loops (one per CPU core)
- **mimalloc** — Microsoft's high-performance allocator
- **Zero-alloc /json and /plaintext** — pre-baked static responses bypass Axum entirely
- **Cached Date header** — updated every 500ms by background task
- **TCP_NODELAY** — disable Nagle's algorithm
- **HTTP/1.1 pipeline_flush** — immediate response flushing

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

| Feature | Standard | Performance |
|---------|----------|-------------|
| Ease of use | ✅ Full middleware | Manual tuning |
| Server | `axum::serve` | Raw `hyper::http1` |
| Accept loops | 1 | N (per CPU core) |
| `/json` path | Through Axum | Zero-alloc bypass |
| Allocator | System | mimalloc |
| Best for | Development, production APIs | Benchmarks, extreme TPS |

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
