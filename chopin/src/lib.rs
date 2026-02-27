// src/lib.rs
pub mod http;
pub mod worker;
pub mod conn;
pub mod slab;
pub mod parser;
pub mod router;
pub mod syscalls;
pub mod server;
pub mod metrics;
pub mod extract;

// Re-exports for users
pub use server::Server;
pub use router::Router;
pub use http::{Method, Request, Response, Context};
pub use extract::{FromRequest, Json, Query};
