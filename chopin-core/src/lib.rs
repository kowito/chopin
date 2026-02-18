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
pub mod logging;
pub mod migrations;
pub mod models;
pub mod module;
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
pub use config::{Config, SecurityConfig};
pub use controllers::AuthModule;
pub use error::ChopinError;
pub use logging::{init_logging, init_logging_json, init_logging_pretty, init_logging_with_level};
pub use module::ChopinModule;
pub use response::ApiResponse;
pub use server::FastRoute;
pub use testing::{TestApp, TestClient, TestResponse};

// ── RBAC re-exports ────────────────────────────────────────────
pub use auth::rbac::RbacService;
pub use extractors::PermissionGuard;

// ── Proc macro re-exports ──────────────────────────────────────
pub use chopin_macros::{login_required, permission_required};

// ── Axum re-exports ────────────────────────────────────────────
// Users should never need `axum` in their Cargo.toml.
pub use axum::{serve, Extension, Router};

// ── HTTP re-exports ────────────────────────────────────────────
pub use axum::body;
pub use axum::http;
pub use axum::http::{HeaderMap, Method, StatusCode};
pub use axum::middleware;

// ── OpenAPI re-exports ─────────────────────────────────────────
// Users should never need `utoipa` in their Cargo.toml.
pub use utoipa;
pub use utoipa::openapi as openapi_spec;
pub use utoipa::OpenApi;
pub use utoipa::ToSchema;
pub use utoipa_scalar;
pub use utoipa_scalar::{Scalar, Servable};

/// Axum extractor re-exports.
///
/// ```rust,ignore
/// use chopin_core::extract::{Path, Query, State};
/// ```
pub mod extract {
    pub use axum::extract::*;
}
