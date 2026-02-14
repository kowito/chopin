//! Chopin prelude — import everything you need with one line.
//!
//! ```rust,ignore
//! use chopin_core::prelude::*;
//! ```
//!
//! This re-exports the most commonly used types, traits, extractors, and
//! routing functions so Chopin users never need to depend on `axum` directly.

// ── Core types ─────────────────────────────────────────────────
pub use crate::ApiResponse;
pub use crate::App;
pub use crate::ChopinError;
pub use crate::Config;
pub use crate::FastRoute;

// ── Router & routing ───────────────────────────────────────────
pub use crate::routing::{any, delete, get, head, options, patch, post, put};
pub use crate::Router;

// ── Extractors ─────────────────────────────────────────────────
pub use crate::extract::{Extension, Path, Query, State};
pub use crate::extractors::{AuthUser, Json, Pagination};

// ── HTTP types ─────────────────────────────────────────────────
pub use crate::http::{HeaderMap, StatusCode};
pub use crate::response::IntoResponse;
pub use crate::Method;

// ── Serde (almost every handler needs these) ───────────────────
pub use serde::{Deserialize, Serialize};
