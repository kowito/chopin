//! Logging and tracing initialization for Chopin.
//!
//! This module provides easy-to-use functions for initializing the tracing
//! subscriber, which is required to see logs and request traces.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use chopin_core::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize logging - call this BEFORE creating the App
//!     init_logging();
//!
//!     let app = App::new().await?;
//!     app.run().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Configuration
//!
//! The logging level can be controlled via the `RUST_LOG` environment variable:
//!
//! ```bash
//! # Show all logs including request traces
//! RUST_LOG=debug cargo run
//!
//! # Show only warnings and errors (production)
//! RUST_LOG=warn cargo run
//!
//! # Fine-grained control
//! RUST_LOG=chopin_core=debug,tower_http=debug,sqlx=warn cargo run
//! ```

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize logging with sensible defaults.
///
/// This function should be called once at the start of your application,
/// **before** creating the `App`. It sets up the tracing subscriber to
/// display formatted logs to stdout.
///
/// The log level is controlled by the `RUST_LOG` environment variable.
/// If not set, defaults to:
/// - `info` level for general logs
/// - Shows request traces in development mode
///
/// # Example
///
/// ```rust,no_run
/// use chopin_core::prelude::*;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     init_logging();
///
///     let app = App::new().await?;
///     app.run().await?;
///     Ok(())
/// }
/// ```
///
/// # Environment Variables
///
/// - `RUST_LOG=debug` - Show all debug logs including HTTP traces
/// - `RUST_LOG=info` - Show info, warn, and error logs (default)
/// - `RUST_LOG=warn` - Show only warnings and errors (production)
///
/// # Panics
///
/// This function will panic if called multiple times. Only call it once
/// at application startup.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Initialize logging with a specific log level.
///
/// This is useful when you want to programmatically set the log level
/// instead of using the `RUST_LOG` environment variable.
///
/// # Example
///
/// ```rust,no_run
/// use chopin_core::prelude::*;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Set debug level for development
///     init_logging_with_level("debug");
///
///     let app = App::new().await?;
///     app.run().await?;
///     Ok(())
/// }
/// ```
///
/// # Common Levels
///
/// - `"trace"` - Very verbose, shows everything
/// - `"debug"` - Debug information including HTTP request traces
/// - `"info"` - General information (recommended for development)
/// - `"warn"` - Only warnings and errors
/// - `"error"` - Only errors
///
/// # Panics
///
/// This function will panic if called multiple times. Only call it once
/// at application startup.
pub fn init_logging_with_level(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Initialize pretty-formatted logging (recommended for development).
///
/// This provides more readable, colorized output with timestamps and
/// spans for better debugging experience.
///
/// # Example
///
/// ```rust,no_run
/// use chopin_core::prelude::*;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     init_logging_pretty();
///
///     let app = App::new().await?;
///     app.run().await?;
///     Ok(())
/// }
/// ```
///
/// # Panics
///
/// This function will panic if called multiple times. Only call it once
/// at application startup.
pub fn init_logging_pretty() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_line_number(true)
                .with_thread_ids(true)
                .with_target(true),
        )
        .init();
}

/// Initialize JSON-formatted logging (recommended for production).
///
/// This outputs logs in JSON format, which is ideal for log aggregation
/// systems like ELK, Datadog, or CloudWatch.
///
/// # Example
///
/// ```rust,no_run
/// use chopin_core::prelude::*;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     init_logging_json();
///
///     let app = App::new().await?;
///     app.run().await?;
///     Ok(())
/// }
/// ```
///
/// # Panics
///
/// This function will panic if called multiple times. Only call it once
/// at application startup.
pub fn init_logging_json() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
