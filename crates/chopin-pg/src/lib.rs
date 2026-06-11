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

pub use connection::{
    CancelToken, CopyReader, CopyWriter, Cursor, Notification, PgConfig, PgConnection,
    SecretString, Transaction,
};
pub use error::{ErrorClass, PgError, PgResult};
pub use pool::{ConnectionGuard, PgPool, PgPoolConfig, PoolStats};
pub use row::{FromRow, Row};
pub use statement::CacheStats;
pub use statement::Statement;
#[cfg(feature = "tls")]
pub use tls::SslMode;
pub use types::{FromSql, PgValue, ToParam, ToSql, TypeRegistry, encode_inet_binary};

// ─── Thread-local pool ───────────────────────────────────────────────────────
//
// Each worker thread maintains its own heap-allocated PgPool via a raw pointer
// stored in a thread-local Cell.  There is no cross-thread synchronisation —
// the Shared-Nothing architecture guarantees that only the owning thread ever
// calls init_pool / pool.

use std::cell::Cell;

thread_local! {
    /// Raw pointer to the per-thread PgPool. Null until `init_pool()` is called.
    static POOL_PTR: Cell<*mut PgPool> = const { Cell::new(std::ptr::null_mut()) };
}

/// Initialise the calling thread's connection pool from a Postgres URL.
///
/// Call this once per worker thread, typically inside
/// [`Chopin::with_worker_init`](chopin_core::Chopin::with_worker_init):
///
/// ```rust,ignore
/// Chopin::new()
///     .with_worker_init(|| {
///         chopin_pg::init_pool("postgres://user:pass@localhost/mydb", 4)
///             .expect("pool init failed");
///     })
///     .mount_all_routes()
///     .serve("0.0.0.0:8080")
///     .unwrap();
/// ```
pub fn init_pool(url: &str, size: usize) -> PgResult<()> {
    let config = connection::PgConfig::from_url(url)?;
    let pg_pool =
        PgPool::connect_with_config(config, pool::PgPoolConfig::new().max_size(size).min_size(1))?;
    let raw = Box::into_raw(Box::new(pg_pool));
    // If a pool was already initialised on this thread (e.g. during hot reload),
    // drop it cleanly before replacing.
    let old = POOL_PTR.with(|p| p.replace(raw));
    if !old.is_null() {
        // SAFETY: old was allocated by a prior init_pool call on this same thread.
        drop(unsafe { Box::from_raw(old) });
    }
    Ok(())
}

/// Return a shared reference to the calling thread's [`PgPool`].
///
/// # Panics
/// Panics with a clear message if [`init_pool`] has not been called on this thread.
///
/// # Safety
/// The reference is valid for the lifetime of this thread. Thread-locals
/// guarantee that only one thread can hold this reference at a time.
///
/// PgPool uses interior mutability (`Cell`/`RefCell`), so all methods take
/// `&self`.  A [`ConnectionGuard`] can safely coexist with other `&PgPool`
/// references — there is no mutable aliasing.
pub fn pool() -> &'static PgPool {
    let ptr = POOL_PTR.with(|p| p.get());
    assert!(
        !ptr.is_null(),
        "chopin_pg: thread-local pool not initialized — \
         call chopin_pg::init_pool(url, size) inside Chopin::with_worker_init(...)"
    );
    // SAFETY: ptr is a valid Box<PgPool> allocated in init_pool(), it lives for
    // the duration of this thread, and thread-locals enforce single-threaded access.
    // Interior mutability makes shared references safe.
    unsafe { &*ptr }
}
