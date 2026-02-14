//! # Performance Mode Example
//!
//! Deep dive into Chopin's unified architecture with benchmarks and code patterns.
//!
//! Chopin uses a single `ChopinService` dispatcher for all requests:
//! - FastRoute match → zero-alloc pre-computed response
//! - No match → Axum Router with full middleware
//!
//! Each FastRoute can be individually configured with decorators:
//! - `.cors()` — permissive CORS + automatic OPTIONS preflight
//! - `.cache_control()` — Cache-Control header
//! - `.get_only()` / `.methods()` — HTTP method filtering
//! - `.header()` — any custom header
//!
//! All decorators are pre-computed at registration time — zero per-request cost.
//!
//! ## Default (single listener)
//!
//! ```bash
//! cargo run -p chopin-performance-mode
//! ```
//!
//! ## With SO_REUSEPORT (multi-core)
//!
//! ```bash
//! REUSEPORT=true cargo run -p chopin-performance-mode --release
//! ```
//!
//! ## Benchmark with wrk
//!
//! ```bash
//! # Install wrk
//! brew install wrk
//!
//! # Start server (in another terminal)
//! REUSEPORT=true cargo run -p chopin-performance-mode --release
//!
//! # Benchmark endpoints
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/json
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/  # Through Axum
//! ```
//!
//! ## Code Patterns
//!
//! ### Per-route trade-off with decorators
//!
//! ```rust ignore
//! use chopin::{App, FastRoute};
//!
//! let app = App::new().await?
//!     // Bare: maximum performance, no middleware
//!     .fast_route(FastRoute::json("/json", br#"{"message":"Hello"}"#))
//!
//!     // With CORS + method filter (still zero per-request cost)
//!     .fast_route(
//!         FastRoute::json("/api/status", br#"{"status":"ok"}"#)
//!             .cors()
//!             .get_only()
//!     )
//!
//!     // With Cache-Control
//!     .fast_route(
//!         FastRoute::text("/health", b"OK")
//!             .cache_control("public, max-age=60")
//!     );
//! ```
//!
//! **Request flow:**
//! ```
//! Client → ChopinService
//!   ├─ GET /json        → FastRoute (bare, ~35ns)
//!   ├─ GET /api/status  → FastRoute (+cors, auto OPTIONS, ~35ns)
//!   ├─ POST /api/status → falls through to Axum (method not allowed on FastRoute)
//!   ├─ OPTIONS /api/status → FastRoute (204 preflight response)
//!   └─ /* → Axum Router → Middleware Stack → Handler
//! ```
//!
//! ### Per-route trade-off matrix
//!
//! | Feature | FastRoute (bare) | FastRoute (+decorators) | Axum Router |
//! |---------|------------------|-------------------------|-------------|
//! | Performance | ~35ns | ~35ns | ~1,000-5,000ns |
//! | Throughput | ~28M req/s | ~28M req/s | ~200K-1M req/s |
//! | CORS | — | `.cors()` | CorsLayer |
//! | Cache-Control | — | `.cache_control()` | manual |
//! | Method filter | — | `.methods()` | built-in |
//! | Auth | — | — | middleware |
//! | Logging | — | — | TraceLayer |
//!
//! **FastRoute is 28-142× faster** — decorators add zero per-request overhead.

use chopin::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Show current configuration
    let reuseport = std::env::var("REUSEPORT").unwrap_or_else(|_| "false".to_string());

    println!("\n🎹 Chopin Performance Mode Example");
    println!("   → REUSEPORT: {}", reuseport);
    println!("   → Run with: REUSEPORT=true cargo run -p chopin-performance-mode --release");
    println!();

    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
    }
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "perf-example-secret");
    }

    let app = App::new()
        .await?
        // Bare: maximum performance benchmark endpoints (zero-alloc, no middleware)
        .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#).get_only())
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!").get_only())
        // With CORS: frontend-accessible status endpoint (still zero per-request cost)
        .fast_route(
            FastRoute::json("/api/status", br#"{"status":"ok"}"#)
                .cors()
                .get_only()
                .cache_control("public, max-age=5"),
        );

    // All other routes (/, /api/auth/*, /api-docs) go through Axum with full middleware
    app.run().await?;

    Ok(())
}
