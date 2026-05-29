//! # Chopin
//!
//! A thread-per-core HTTP framework for Rust with zero async overhead.
//!
//! Chopin uses a **shared-nothing** architecture: each worker thread owns its own
//! epoll/kqueue event loop, socket accept, router clone, and connection pool.
//! There are no `Arc`, no `Mutex`, and no async runtimes in the hot path.
//!
//! ## When to use Chopin
//!
//! Chopin is a good fit when you need:
//! - Maximum single-node throughput with predictable, low tail latency.
//! - A simple synchronous handler model — no `async fn`, no `.await`.
//! - A batteries-included stack: routing, JSON, PostgreSQL, JWT, rate limiting, WebSocket.
//!
//! If your workload is dominated by long-running async I/O (e.g. streaming proxies),
//! an async runtime like Tokio/Axum may be a better fit.
//!
//! ## Crate layout
//!
//! | Crate | Purpose |
//! |---|---|
//! | [`chopin-core`](https://crates.io/crates/chopin-core) | HTTP engine, routing, extractors (this crate) |
//! | [`chopin-pg`](https://crates.io/crates/chopin-pg) | PostgreSQL driver + connection pool |
//! | [`chopin-orm`](https://crates.io/crates/chopin-orm) | Type-safe query builder and model layer |
//! | [`chopin-auth`](https://crates.io/crates/chopin-auth) | JWT, OAuth 2.0, PKCE, RBAC, password hashing |
//! | [`chopin-macros`](https://crates.io/crates/chopin-macros) | `#[get]`, `#[post]`, `#[derive(IntoResponse)]`, … |
//! | [`chopin-cli`](https://crates.io/crates/chopin-cli) | `chopin new`, `chopin generate scaffold`, migrations |
//!
//! ## Common imports
//!
//! ```rust,no_run
//! // The essentials — routing macro, handler context, response builder
//! use chopin_core::{get, post, Context, Response};
//!
//! // Typed extractors
//! use chopin_core::{Json, Query, FromRequest};
//!
//! // App builder and low-level server
//! use chopin_core::{Chopin, Server, Router};
//! ```
//!
//! ## Quick Start
//!
//! ### Minimal server with attribute-based routes
//!
//! ```rust,no_run
//! use chopin_core::{get, Context, Response, Chopin};
//!
//! #[get("/")]
//! fn index(_ctx: Context) -> Response {
//!     Response::text("Hello, world!")
//! }
//!
//! fn main() {
//!     Chopin::new()
//!         .mount_all_routes()
//!         .serve("0.0.0.0:8080")
//!         .unwrap();
//! }
//! ```
//!
//! ### Typed path parameters and JSON responses
//!
//! ```rust,no_run
//! use chopin_core::{get, post, Context, Json, KJson, Response};
//! use serde::Deserialize;
//!
//! #[derive(KJson)]
//! struct Post { id: i32, title: String }
//!
//! #[derive(Deserialize)]
//! struct CreatePost { title: String }
//!
//! // GET /posts/:id  →  parse id, return JSON
//! #[get("/posts/:id")]
//! fn show(ctx: Context) -> Response {
//!     let id: i32 = match ctx.param_parse("id") {
//!         Ok(v)  => v,
//!         Err(r) => return r,   // auto 400 Bad Request
//!     };
//!     let post = Post { id, title: "Hello".into() };
//!     Response::json(&post)
//! }
//!
//! // POST /posts  →  deserialise JSON body
//! #[post("/posts")]
//! fn create(ctx: Context) -> Response {
//!     let Json(body) = match ctx.extract::<Json<CreatePost>>() {
//!         Ok(v)  => v,
//!         Err(r) => return r,   // auto 400 Bad Request
//!     };
//!     Response::text(format!("created: {}", body.title))
//! }
//! ```
//!
//! ### With a per-thread database pool
//!
//! ```rust,no_run
//! use chopin_core::{get, Context, Response, Chopin};
//!
//! #[get("/health")]
//! fn health(_ctx: Context) -> Response {
//!     Response::text("ok")
//! }
//!
//! fn main() {
//!     Chopin::new()
//!         .mount_all_routes()
//!         .with_worker_init(|| {
//!             // Runs once per worker thread — safe to use thread-local state.
//!             chopin_pg::init_pool("postgres://localhost/myapp", 10)
//!                 .expect("DB pool init failed");
//!         })
//!         .serve("0.0.0.0:8080")
//!         .unwrap();
//! }
//! ```
//!
//! ## Feature flags
//!
//! | Flag | Description |
//! |---|---|
//! | `io-uring` | Linux io_uring backend (kernel ≥ 5.11). Up to 50 % lower latency. |
//! | `tls` | TLS 1.2/1.3 termination via rustls. |
//!
//! ```toml
//! [dependencies]
//! chopin-core = { version = "0.5.30", features = ["tls"] }
//! # Linux only:
//! chopin-core = { version = "0.5.30", features = ["io-uring"] }
//! ```

// Use mimalloc as the global allocator for all binaries that link chopin-core.
// mimalloc significantly outperforms the system allocator under high concurrency
// due to its per-thread free-lists, low fragmentation, and cache-aware design.
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod bufpool;
pub mod conn;
pub mod error;
pub mod extract;
pub mod filter;
pub mod headers;
pub mod http;
pub mod http2;
pub mod http_date;
pub mod json;
pub mod metrics;
pub mod multipart;
pub mod openapi;
pub mod parser;
pub mod rate_limit;
pub mod router;
pub mod server;
pub mod slab;
pub mod state;
pub mod syscalls;
pub mod timer;
#[cfg(feature = "tls")]
pub mod tls;
pub mod websocket;
pub mod worker;

// Re-exports for users
pub use bufpool::{BufGuard, get as buf_get, get_with_capacity as buf_get_with_capacity};
pub use error::{ChopinError, ChopinResult};
pub use extract::{FromRequest, Json, Query};
pub use filter::{Filter, FilterStack, LoggingFilter, PassthroughFilter};
pub use headers::{Header, HeaderValue, Headers, IntoHeaderValue};
pub use http::{Body, Context, IntoResponse, Method, OwnedFd, Request, Response};
pub use json::KJson;
pub use router::{RouteDef, Router};
pub use server::{Chopin, Server};
pub use state::{get_state, set_state, with_state};

// Re-export for macros
pub use chopin_macros::*;
pub use inventory;
