//! # Chopin Hello World
//!
//! The simplest possible Chopin application.
//! Starts a server with built-in auth, OpenAPI docs, and welcome page.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p chopin-hello-world
//! ```
//!
//! ## Endpoints
//!
//! - `GET /` — Welcome JSON
//! - `POST /api/auth/signup` — Create user
//! - `POST /api/auth/login` — Login
//! - `GET /api-docs` — Scalar OpenAPI UI

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = chopin_core::App::new().await?;
    app.run().await?;

    Ok(())
}
