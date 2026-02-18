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

// ── Module system ──────────────────────────────────────────────
pub use crate::AuthModule;
pub use crate::ChopinModule;
pub use async_trait::async_trait;

// ── Logging ────────────────────────────────────────────────────
pub use crate::logging::{
    init_logging, init_logging_json, init_logging_pretty, init_logging_with_level,
};

// ── Router & routing ───────────────────────────────────────────
pub use crate::routing::{any, delete, get, head, options, patch, post, put};
pub use crate::Router;

// ── Extractors ─────────────────────────────────────────────────
pub use crate::extract::{Extension, Path, Query, State};
pub use crate::extractors::{AuthUser, Json, Pagination, PermissionGuard};

// ── RBAC & Auth Macros ─────────────────────────────────────────
pub use crate::auth::middleware::{login_required_layer, permission_required_layer};
pub use crate::auth::rbac::RbacService;
pub use chopin_macros::{login_required, permission_required};

// ── HTTP types ─────────────────────────────────────────────────
pub use crate::http::{HeaderMap, StatusCode};
pub use crate::response::IntoResponse;
pub use crate::Method;

// ── Serde (almost every handler needs these) ───────────────────
pub use serde::{Deserialize, Serialize};

// ── OpenAPI (API documentation) ────────────────────────────────
pub use crate::openapi::SecurityAddon;
pub use crate::{OpenApi, Scalar, Servable, ToSchema};
