//! # Performance Mode Example
//!
//! Deep dive into Chopin's dual-mode architecture with benchmarks and code patterns.
//!
//! ## Standard Mode (Default)
//!
//! Full Axum pipeline with all middleware:
//! ```bash
//! cargo run -p chopin-performance-mode
//! ```
//!
//! **Features:**
//! - Full middleware stack
//! - CORS, tracing, request-id
//! - Graceful shutdown
//! - ~150K-300K req/s typical
//!
//! ## Performance Mode
//!
//! Raw hyper HTTP/1.1 with SO_REUSEPORT:
//! ```bash
//! SERVER_MODE=performance cargo run -p chopin-performance-mode --release
//! ```
//!
//! **Features:**
//! - Raw hyper service
//! - SO_REUSEPORT multi-core accept loops
//! - `/json` and `/plaintext` pre-computed (zero-alloc)
//! - mimalloc global allocator
//! - ~500K-1.7M+ req/s benchmark
//!
//! ## Benchmark with wrk
//!
//! ```bash
//! # Install wrk
//! brew install wrk
//!
//! # Start server (in another terminal)
//! SERVER_MODE=performance cargo run -p chopin-performance-mode --release
//!
//! # Benchmark endpoints
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/json
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext
//! wrk -t4 -c256 -d10s http://127.0.0.1:3000/  # Through Axum
//! ```
//!
//! ## Code Patterns
//!
//! ### Standard Mode (Default)
//!
//! ```rust ignore
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let app = chopin_core::App::new().await?;
//!     app.run().await?;  // Uses axum::serve with full middleware
//!     Ok(())
//! }
//! ```
//!
//! **Request flow:**
//! ```
//! Client → TCP Listener → Axum Router → Middleware Stack → Handler
//! ```
//!
//! ### Performance Mode
//!
//! ```rust ignore
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     std::env::set_var("SERVER_MODE", "performance");
//!     
//!     let app = chopin_core::App::new().await?;
//!     app.run().await?;  // Uses raw hyper + SO_REUSEPORT
//!     Ok(())
//! }
//! ```
//!
//! **Request flow:**
//! ```
//! Client → SO_REUSEPORT X N cores
//!   ├─ /json → ChopinService (pre-computed) → Response
//!   ├─ /plaintext → ChopinService (pre-computed) → Response
//!   └─ /* → Axum Router → Handler
//! ```
//!
//! ### Environment Setup
//!
//! Create `.env` or set env vars:
//!
//! ```bash
//! # Standard mode (default)
//! SERVER_MODE=standard
//! DATABASE_URL=sqlite:./app.db
//! JWT_SECRET=your-secret-key
//!
//! # Performance mode
//! SERVER_MODE=performance
//! DATABASE_URL=sqlite::memory:
//! JWT_SECRET=perf-benchmark
//! ```
//!
//! ## Architecture Comparison
//!
//! | Aspect | Standard | Performance |
//! |--------|----------|-------------|
//! | Server | axum::serve | raw hyper |
//! | TCP | Single listener | SO_REUSEPORT × CPU cores |
//! | Middleware | Full stack | Minimal (perf paths only) |
//! | `/json` | Through Axum | Pre-computed bytes (raw hyper) |
//! | `/plaintext` | Through Axum | Pre-computed bytes (raw hyper) |
//! | DateTime headers | Fresh per request | Cached 500ms |
//! | Allocator | System malloc | mimalloc (via `perf` feature) |
//! | LTO | enabled | fat (enabled) |
//! | Codegen units | 1 | 1 |
//! | Target CPU | native | native |
//! | Use case | Dev, typical prod | Benchmarks, extreme throughput |

use chopin_core::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Demonstrate performance mode via env var
    let server_mode = std::env::var("SERVER_MODE").unwrap_or_else(|_| "standard".to_string());

    println!("\n🎹 Chopin Performance Mode Example");
    println!("   → Mode: {}", server_mode);
    println!(
        "   → Run with: SERVER_MODE=performance cargo run -p chopin-performance-mode --release"
    );
    println!();

    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
    }
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "perf-example-secret");
    }

    let app = App::new()
        .await?
        // Register benchmark endpoints as FastRoutes (zero-alloc in performance mode)
        .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));

    app.run().await?;

    Ok(())
}
