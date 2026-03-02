// src/lib.rs

// Use mimalloc as the global allocator for all binaries that link chopin-core.
// mimalloc significantly outperforms the system allocator under high concurrency
// due to its per-thread free-lists, low fragmentation, and cache-aware design.
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod conn;
pub mod error;
pub mod extract;
pub mod http;
pub mod http_date;
pub mod json;
pub mod metrics;
pub mod multipart;
pub mod parser;
pub mod router;
pub mod server;
pub mod slab;
pub mod syscalls;
pub mod timer;
pub mod worker;

// Re-exports for users
pub use error::{ChopinError, ChopinResult};
pub use extract::{FromRequest, Json, Query};
pub use http::{Body, Context, Method, OwnedFd, Request, Response};
pub use json::KJson;
pub use router::{RouteDef, Router};
pub use server::{Chopin, Server};

// Re-export for macros
pub use chopin_macros::*;
pub use inventory;
