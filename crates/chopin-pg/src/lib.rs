//! # chopin-pg
//!
//! A zero-copy, high-performance PostgreSQL driver designed for the
//! Chopin thread-per-core, Shared-Nothing architecture.
//!
//! ## Features
//! - **Thread-per-core**: Each worker owns its own PG connections.
//! - **Non-blocking I/O**: Socket is set to non-blocking after connect; all
//!   reads/writes go through poll-based primitives with configurable timeouts.
//! - **Zero-copy**: Row data is sliced directly from the read buffer.
//! - **SCRAM-SHA-256**: Full authentication support.
//! - **Extended Query Protocol**: Parse/Bind/Execute with implicit caching.
//! - **Transaction support**: Safe closure-based API with auto-rollback.
//! - **COPY protocol**: Both COPY IN (writer) and COPY OUT (reader).
//! - **LISTEN/NOTIFY**: Notification buffering during query processing.
//! - **Rich types**: UUID, Date, Time, Timestamp, Interval, Numeric, INET, Arrays.
//! - **Binary INET/CIDR**: Proper binary encoding/decoding for network types.
//! - **Type-safe queries**: `ToSql`/`FromSql` traits for ergonomic parameter passing.
//! - **Connection pool**: Worker-local pool with RAII `ConnectionGuard`, FIFO idle
//!   queue, `try_get()` / `get()` with timeout, and automatic return on drop.
//! - **Error classification**: Transient vs permanent errors for retry logic.
//!
//! ## Quick Start
//! ```ignore
//! use chopin_pg::{PgConfig, PgConnection};
//!
//! let config = PgConfig::new("localhost", 5432, "user", "pass", "mydb");
//! let mut conn = PgConnection::connect(&config)?;
//!
//! // Simple query
//! let rows = conn.query("SELECT $1::int4 + $2::int4", &[&1i32, &2i32])?;
//! let sum: i32 = rows[0].get_typed(0)?;
//!
//! // Transaction with closure
//! conn.transaction(|tx| {
//!     tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Alice"])?;
//!     Ok(())
//! })?;
//! ```
//!
//! ## Pool Usage
//! ```ignore
//! use chopin_pg::{PgConfig, PgPool, PgPoolConfig};
//!
//! let config = PgConfig::new("localhost", 5432, "user", "pass", "mydb");
//! let pool_cfg = PgPoolConfig::new().max_size(25).checkout_timeout(Duration::from_secs(3));
//! let mut pool = PgPool::connect_with_config(config, pool_cfg)?;
//!
//! // Get a connection — returned to pool when guard drops
//! let mut guard = pool.get()?;
//! let rows = guard.query("SELECT 1", &[])?;
//! // guard drops → connection returned to idle queue
//! ```

pub mod auth;
pub mod codec;
pub mod connection;
pub mod error;
pub mod pool;
pub mod protocol;
pub mod row;
pub mod statement;
#[cfg(feature = "tls")]
pub mod tls;
pub mod types;

pub use connection::{CopyReader, CopyWriter, Notification, PgConfig, PgConnection, Transaction};
pub use error::{ErrorClass, PgError, PgResult};
pub use pool::{ConnectionGuard, PgPool, PgPoolConfig, PoolStats};
pub use row::Row;
pub use statement::Statement;
#[cfg(feature = "tls")]
pub use tls::SslMode;
pub use types::{FromSql, PgValue, ToParam, ToSql, TypeRegistry, encode_inet_binary};
