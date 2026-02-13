# 🎹 Chopin Core

**Chopin: High-fidelity engineering for the modern virtuoso.**

A high-performance Rust web framework combining the ease of Axum with production-ready features like authentication, database integration, caching, and file uploads—all optimized for extreme throughput.

## Features

- **Dual Server Modes** — Standard (easy, full middleware) or Performance (raw hyper, SO_REUSEPORT, zero-alloc)
- **FastRoute API** — Zero-allocation endpoints via `ChopinBody` + direct header manipulation for extreme performance
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
use chopin_core::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

## Performance Mode

For maximum throughput:

```bash
SERVER_MODE=performance cargo run --release --features perf
```

Enables:
- **SO_REUSEPORT** — N accept loops (one per CPU core)
- **mimalloc** — Microsoft's high-performance allocator
- **Zero-alloc endpoints** — pre-baked static responses
- **Cached Date header** — updated every 500ms
- **TCP_NODELAY** — disable Nagle's algorithm

## Documentation

See the [main repository](https://github.com/kowito/chopin) for comprehensive guides:

- [Getting Started](https://github.com/kowito/chopin/blob/main/docs/getting-started.md)
- [Architecture](https://github.com/kowito/chopin/blob/main/docs/architecture.md)
- [Security](https://github.com/kowito/chopin/blob/main/docs/security.md)
- [Performance Guide](https://github.com/kowito/chopin/blob/main/docs/performance.md)

## License

WTFPL (Do What The Fuck You Want To Public License)
