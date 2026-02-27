//! # chopin-pg
//!
//! A zero-copy, high-performance PostgreSQL driver designed for the
//! Chopin thread-per-core, Shared-Nothing architecture.
//!
//! ## Features
//! - **Thread-per-core**: Each worker owns its own PG connections.
//! - **Zero-copy**: Row data is sliced directly from the read buffer.
//! - **SCRAM-SHA-256**: Full authentication support.
//! - **Extended Query Protocol**: Parse/Bind/Execute with implicit caching.

pub mod protocol;
pub mod codec;
pub mod auth;
pub mod connection;
pub mod types;
pub mod error;
pub mod row;
pub mod pool;
pub mod statement;

pub use connection::{PgConnection, PgConfig};
pub use pool::PgPool;
pub use row::Row;
pub use types::PgValue;
pub use error::{PgError, PgResult};
pub use statement::Statement;
