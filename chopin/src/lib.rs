// src/lib.rs
pub mod conn;
pub mod error;
pub mod extract;
pub mod http;
pub mod json;
pub mod metrics;
pub mod parser;
pub mod router;
pub mod server;
pub mod slab;
pub mod syscalls;
pub mod worker;

// Re-exports for users
pub use error::{ChopinError, ChopinResult};
pub use extract::{FromRequest, Json, Query};
pub use http::{Context, Method, Request, Response};
pub use json::KJson;
pub use router::Router;
pub use server::Server;
