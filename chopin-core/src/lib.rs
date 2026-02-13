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
pub mod response;
pub mod routing;
pub mod server;
pub mod storage;
pub mod testing;

pub use app::App;
pub use cache::CacheService;
pub use config::{Config, ServerMode};
pub use error::ChopinError;
pub use response::ApiResponse;
pub use server::FastRoute;
pub use testing::{TestApp, TestClient, TestResponse};
