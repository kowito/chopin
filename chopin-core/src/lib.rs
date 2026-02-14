// ─── Global allocator: mimalloc (enable `perf` feature) ───
// mimalloc is a compact general-purpose allocator by Microsoft that
// dramatically outperforms glibc malloc and jemalloc under high concurrency.
#[cfg(feature = "perf")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod app;
pub mod auth;
pub mod cache;
pub mod config;
pub mod controllers;
pub mod db;
pub mod error;
pub mod extractors;
#[cfg(feature = "graphql")]
pub mod graphql;
pub mod json;
pub mod migrations;
pub mod models;
pub mod openapi;
pub mod perf;
pub mod prelude;
pub mod response;
pub mod routing;
pub mod server;
pub mod storage;
pub mod testing;

// ── Core type re-exports ───────────────────────────────────────
pub use app::App;
pub use cache::CacheService;
pub use config::Config;
pub use error::ChopinError;
pub use response::ApiResponse;
pub use server::FastRoute;
pub use testing::{TestApp, TestClient, TestResponse};

// ── Axum re-exports ────────────────────────────────────────────
// Users should never need `axum` in their Cargo.toml.
pub use axum::{
    Extension,
    Router,
    serve,
};

// ── HTTP re-exports ────────────────────────────────────────────
pub use axum::http;
pub use axum::http::{HeaderMap, Method, StatusCode};
pub use axum::body;
pub use axum::middleware;

/// Axum extractor re-exports.
///
/// ```rust,ignore
/// use chopin::extract::{Path, Query, State};
/// ```
pub mod extract {
    pub use axum::extract::*;
}
