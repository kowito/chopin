//! # Chopin Benchmark Server
//!
//! A minimal server designed for maximum throughput benchmarking.
//! Uses SO_REUSEPORT multi-core accept loops with per-core runtimes.
//!
//! Fast routes are registered via the `FastRoute` API — these bypass Axum
//! entirely for maximum throughput.
//!
//! ## Endpoints
//!
//! - `GET /json`      → `{"message":"Hello, World!"}`   (FastRoute, per-request serialize, TFB compliant)
//! - `GET /plaintext` → `Hello, World!`                 (FastRoute, zero-alloc static)
//! - `GET /`          → Welcome JSON via Axum
//! - `GET /api-docs`  → Scalar OpenAPI explorer
//!
//! ## Usage
//!
//! ```bash
//! REUSEPORT=true DATABASE_URL=sqlite::memory: JWT_SECRET=bench \
//!   cargo run -p chopin-benchmark --release
//! ```
//!
//! ## Benchmark
//!
//! ```bash
//! # JSON endpoint (FastRoute, per-request serialization)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/json
//!
//! # Plaintext endpoint (FastRoute, zero-alloc static)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext
//!
//! # Axum route (standard middleware path)
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/
//! ```

use chopin_core::prelude::*;
use chopin_core::FastRoute;
use serde::Serialize;

/// TechEmpower JSON serialization test payload.
/// Serialized per-request to comply with TFB rules.
#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging_with_level("warn");

    // REUSEPORT=true activates SO_REUSEPORT multi-core accept loops
    // Set via environment or .env file
    if std::env::var("REUSEPORT").is_err() {
        std::env::set_var("REUSEPORT", "true");
    }
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
    }
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "benchmark-secret");
    }

    let app = App::new()
        .await?
        // JSON: per-request serialization (TechEmpower compliant).
        // Uses thread-local buffer reuse + sonic-rs SIMD (~100-150ns/req).
        .fast_route(
            FastRoute::json_serialize("/json", || Message {
                message: "Hello, World!",
            })
            .get_only(),
        )
        // Plaintext: static pre-computed bytes (zero-alloc, ~35ns/req).
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!").get_only());

    app.run().await?;

    Ok(())
}
