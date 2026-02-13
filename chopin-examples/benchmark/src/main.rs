//! # Chopin Benchmark Server
//!
//! A minimal server designed for maximum throughput benchmarking.
//! Uses `ServerMode::Performance` — raw hyper HTTP/1.1 with SO_REUSEPORT.
//!
//! Fast routes are registered via the `FastRoute` API — these bypass Axum
//! entirely and serve pre-computed responses with zero heap allocation.
//!
//! ## Endpoints
//!
//! - `GET /json`      → `{"message":"Hello, World!"}`   (FastRoute, zero-alloc)
//! - `GET /plaintext` → `Hello, World!`                 (FastRoute, zero-alloc)
//! - `GET /`          → Welcome JSON via Axum
//! - `GET /api-docs`  → Scalar OpenAPI explorer
//!
//! ## Usage
//!
//! ```bash
//! SERVER_MODE=performance DATABASE_URL=sqlite::memory: JWT_SECRET=bench \
//!   cargo run -p chopin-benchmark --release
//! ```
//!
//! ## Benchmark
//!
//! ```bash
//! # JSON endpoint (FastRoute fast-path)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/json
//!
//! # Plaintext endpoint (FastRoute fast-path)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext
//!
//! # Axum route (standard middleware path)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/
//! ```

use chopin_core::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    // SERVER_MODE=performance activates raw hyper + SO_REUSEPORT
    // Set via environment or .env file
    if std::env::var("SERVER_MODE").is_err() {
        std::env::set_var("SERVER_MODE", "performance");
    }
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
    }
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "benchmark-secret");
    }

    let app = App::new()
        .await?
        // Register benchmark endpoints as FastRoutes.
        // These bypass Axum entirely — zero allocation, maximum throughput.
        .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));

    app.run().await?;

    Ok(())
}
