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

pub mod auth;
pub mod codec;
pub mod connection;
pub mod error;
pub mod pool;
pub mod protocol;
pub mod row;
pub mod statement;
pub mod types;

pub use connection::{PgConfig, PgConnection};
pub use error::{PgError, PgResult};
pub use pool::PgPool;
pub use row::Row;
pub use statement::Statement;
pub use types::PgValue;
