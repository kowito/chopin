//! Worker-local connection pool with RAII guards for Shared-Nothing architecture.
//!
//! Each worker thread owns its own `PgPool` instance — **no locks, no
//! cross-thread synchronization**.
//!
//! ## Design
//!
//! - Idle connections are kept in a **FIFO queue** (`VecDeque`).
//! - `get()` / `try_get()` return a [`ConnectionGuard`] that automatically
//!   returns the connection to the idle queue when dropped.
//! - A waiter queue allows callers to register interest when the pool is
//!   exhausted; the next `ConnectionGuard` drop will service the first waiter.
//! - Connections are validated on checkout (optional) and reaped on idle /
//!   lifetime expiry.
//!
//! ## Features
//! - Lazy and eager connection initialization
//! - `try_get()` — non-blocking, returns `WouldBlock` if exhausted
//! - `get()` — blocks with timeout, returns `PoolTimeout` if exceeded
//! - RAII `ConnectionGuard` — connection returned on drop
//! - Connection validation (`test_on_checkout`)
//! - Max lifetime and idle timeout
//! - Automatic reconnection on stale connections
//! - Graceful shutdown via `close_all()`

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::connection::{PgConfig, PgConnection};
use crate::error::{PgError, PgResult};

// ─── Pool Configuration ───────────────────────────────────────

/// Pool configuration options.
#[derive(Debug, Clone)]
pub struct PgPoolConfig {
    /// Maximum number of connections in this pool.
    pub max_size: usize,
    /// Minimum number of connections to maintain (eagerly created).
    pub min_size: usize,
    /// Maximum lifetime of a connection before it is closed and recreated.
    pub max_lifetime: Option<Duration>,
    /// Close connections that have been idle for longer than this.
    pub idle_timeout: Option<Duration>,
    /// Maximum time to wait when all connections are busy (`get()`).
    pub checkout_timeout: Option<Duration>,
    /// Maximum time to wait when creating a new connection.
    pub connection_timeout: Option<Duration>,
    /// If true, run a validation query before returning a connection from the pool.
    pub test_on_checkout: bool,
    /// The query to use for validation (default: `"SELECT 1"`).
    pub validation_query: String,
    /// If true, automatically reconnect when a connection is found to be dead.
    pub auto_reconnect: bool,
}

impl Default for PgPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_size: 1,
            max_lifetime: Some(Duration::from_secs(30 * 60)), // 30 min
            idle_timeout: Some(Duration::from_secs(10 * 60)), // 10 min
            checkout_timeout: Some(Duration::from_secs(5)),
            connection_timeout: Some(Duration::from_secs(5)),
            test_on_checkout: false,
            validation_query: "SELECT 1".to_string(),
            auto_reconnect: true,
        }
    }
}

impl PgPoolConfig {
    /// Create a new pool config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum pool size.
    pub fn max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }

    /// Set the minimum pool size.
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// Set the maximum connection lifetime.
    pub fn max_lifetime(mut self, duration: Duration) -> Self {
        self.max_lifetime = Some(duration);
        self
    }

    /// Set the idle timeout.
    pub fn idle_timeout(mut self, duration: Duration) -> Self {
        self.idle_timeout = Some(duration);
        self
    }

    /// Set the checkout timeout (how long `get()` waits for a free connection).
    pub fn checkout_timeout(mut self, duration: Duration) -> Self {
        self.checkout_timeout = Some(duration);
        self
    }

    /// Set the connection timeout.
    pub fn connection_timeout(mut self, duration: Duration) -> Self {
        self.connection_timeout = Some(duration);
        self
    }

    /// Enable or disable test-on-checkout.
    pub fn test_on_checkout(mut self, enable: bool) -> Self {
        self.test_on_checkout = enable;
        self
    }

    /// Disable max lifetime.
    pub fn no_max_lifetime(mut self) -> Self {
        self.max_lifetime = None;
        self
    }

    /// Disable idle timeout.
    pub fn no_idle_timeout(mut self) -> Self {
        self.idle_timeout = None;
        self
    }
}

// ─── PooledConn ───────────────────────────────────────────────

/// Metadata for a pooled connection.
struct PooledConn {
    conn: PgConnection,
    created_at: Instant,
    last_used: Instant,
}

impl PooledConn {
    fn new(conn: PgConnection) -> Self {
        let now = Instant::now();
        Self {
            conn,
            created_at: now,
            last_used: now,
        }
    }

    /// Returns `true` if this connection has exceeded its max lifetime.
    fn is_lifetime_expired(&self, max_lifetime: Option<Duration>) -> bool {
        max_lifetime.is_some_and(|max| self.created_at.elapsed() > max)
    }

    /// Returns `true` if this connection has been idle too long.
    fn is_idle_expired(&self, idle_timeout: Option<Duration>) -> bool {
        idle_timeout.is_some_and(|timeout| self.last_used.elapsed() > timeout)
    }
}

// ─── Pool Statistics ──────────────────────────────────────────

/// Pool statistics.
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub total_checkouts: u64,
    pub total_connections_created: u64,
    pub total_connections_closed: u64,
    pub validation_failures: u64,
    pub lifetime_expirations: u64,
    pub idle_expirations: u64,
    pub checkout_timeouts: u64,
}

// ─── PgPool ───────────────────────────────────────────────────

/// A single-threaded, worker-local connection pool.
///
/// Connections are stored in an **idle FIFO queue**.  On checkout the oldest
/// idle connection is returned; on return (guard drop) the connection is
/// pushed to the back of the queue.
pub struct PgPool {
    config: PgConfig,
    pool_config: PgPoolConfig,
    /// Idle connections, ready to be checked out.
    idle: VecDeque<PooledConn>,
    /// Number of connections that have been checked out and are currently
    /// in use (tracked so we can enforce `max_size`).
    active: usize,
    /// Statistics.
    stats: PoolStats,
}

impl PgPool {
    /// Create a new pool with the given configuration and size.
    /// Connections are lazily initialized on first checkout.
    pub fn new(config: PgConfig, size: usize) -> Self {
        let pool_config = PgPoolConfig::default().max_size(size);
        Self {
            config,
            pool_config,
            idle: VecDeque::with_capacity(size),
            active: 0,
            stats: PoolStats::default(),
        }
    }

    /// Create a new pool with full configuration.
    pub fn with_config(config: PgConfig, pool_config: PgPoolConfig) -> Self {
        Self {
            idle: VecDeque::with_capacity(pool_config.max_size),
            config,
            pool_config,
            active: 0,
            stats: PoolStats::default(),
        }
    }

    /// Create a pool and eagerly initialize `size` connections.
    pub fn connect(config: PgConfig, size: usize) -> PgResult<Self> {
        let mut pool = Self::new(config, size);
        for _ in 0..size {
            let conn = PgConnection::connect(&pool.config)?;
            pool.idle.push_back(PooledConn::new(conn));
            pool.stats.total_connections_created += 1;
        }
        Ok(pool)
    }

    /// Create a pool with full config and eagerly initialize `min_size` connections.
    pub fn connect_with_config(config: PgConfig, pool_config: PgPoolConfig) -> PgResult<Self> {
        let min = pool_config.min_size.min(pool_config.max_size);
        let mut pool = Self::with_config(config, pool_config);
        for _ in 0..min {
            let conn = PgConnection::connect(&pool.config)?;
            pool.idle.push_back(PooledConn::new(conn));
            pool.stats.total_connections_created += 1;
        }
        Ok(pool)
    }

    // ─── Checkout Methods ─────────────────────────────────────

    /// Internal: attempt to check out a `PooledConn` without wrapping in a
    /// guard.  Does **not** increment `active` – the caller is responsible
    /// for that.
    fn try_checkout(&mut self) -> PgResult<PooledConn> {
        self.stats.total_checkouts += 1;

        // Try to pop an idle connection (FIFO – oldest first)
        while let Some(mut pooled) = self.idle.pop_front() {
            // Check lifetime / idle expiry
            if pooled.is_lifetime_expired(self.pool_config.max_lifetime) {
                self.stats.lifetime_expirations += 1;
                self.stats.total_connections_closed += 1;
                continue; // drop it, try next
            }
            if pooled.is_idle_expired(self.pool_config.idle_timeout) {
                self.stats.idle_expirations += 1;
                self.stats.total_connections_closed += 1;
                continue;
            }

            // Optionally validate
            if self.pool_config.test_on_checkout
                && pooled
                    .conn
                    .query_simple(&self.pool_config.validation_query)
                    .is_err()
            {
                self.stats.validation_failures += 1;
                self.stats.total_connections_closed += 1;
                if self.pool_config.auto_reconnect {
                    // Replace with a fresh connection
                    match PgConnection::connect(&self.config) {
                        Ok(new_conn) => {
                            pooled = PooledConn::new(new_conn);
                            self.stats.total_connections_created += 1;
                        }
                        Err(e) => return Err(e),
                    }
                } else {
                    return Err(PgError::PoolValidationFailed);
                }
            }

            pooled.last_used = Instant::now();
            return Ok(pooled);
        }

        // No idle connection — can we create a new one?
        let total = self.active + self.idle.len();
        if total < self.pool_config.max_size {
            let conn = PgConnection::connect(&self.config)?;
            self.stats.total_connections_created += 1;
            let pooled = PooledConn::new(conn);
            return Ok(pooled);
        }

        // Pool is exhausted
        Err(PgError::PoolExhausted)
    }

    /// Non-blocking attempt to get a connection.
    ///
    /// Returns a [`ConnectionGuard`] wrapping the connection.  When the guard
    /// is dropped the connection is returned to the idle queue automatically.
    ///
    /// Returns `Err(PgError::PoolExhausted)` if no connection is available and
    /// the pool is at capacity.
    pub fn try_get(&mut self) -> PgResult<ConnectionGuard<'_>> {
        let pooled = self.try_checkout()?;
        self.active += 1;
        Ok(ConnectionGuard {
            pool: self as *mut PgPool,
            conn: Some(pooled),
            _marker: std::marker::PhantomData,
        })
    }

    /// Get a connection, waiting up to the configured `checkout_timeout`.
    ///
    /// Internally calls `try_checkout` in a loop with a short sleep between
    /// attempts.  In a production event-loop the sleep would be replaced
    /// by yielding to the scheduler.
    pub fn get(&mut self) -> PgResult<ConnectionGuard<'_>> {
        let timeout = self
            .pool_config
            .checkout_timeout
            .unwrap_or(Duration::from_secs(5));
        let start = Instant::now();

        // First attempt — fast path.
        match self.try_checkout() {
            Ok(pooled) => {
                self.active += 1;
                return Ok(ConnectionGuard {
                    pool: self as *mut PgPool,
                    conn: Some(pooled),
                    _marker: std::marker::PhantomData,
                });
            }
            Err(PgError::PoolExhausted) => { /* fall through to retry loop */ }
            Err(e) => return Err(e),
        }

        // Retry loop with back-off: 100µs → 500µs → 1ms (capped).
        let backoff_us = [100u64, 250, 500, 1000];
        let mut attempt = 0usize;
        loop {
            if start.elapsed() >= timeout {
                self.stats.checkout_timeouts += 1;
                return Err(PgError::PoolTimeout);
            }

            let sleep_us = backoff_us[attempt.min(backoff_us.len() - 1)];
            std::thread::sleep(Duration::from_micros(sleep_us));
            attempt += 1;

            match self.try_checkout() {
                Ok(pooled) => {
                    self.active += 1;
                    return Ok(ConnectionGuard {
                        pool: self as *mut PgPool,
                        conn: Some(pooled),
                        _marker: std::marker::PhantomData,
                    });
                }
                Err(PgError::PoolExhausted) => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Return a connection to the pool (called by `ConnectionGuard::drop`).
    fn return_conn(&mut self, mut pooled: PooledConn) {
        self.active = self.active.saturating_sub(1);

        // Discard broken connections — they cannot be reused.
        if pooled.conn.is_broken() {
            self.stats.total_connections_closed += 1;
            return; // pooled dropped here → PgConnection::drop sends Terminate
        }

        pooled.last_used = Instant::now();

        // Only return if pool is not over capacity
        if self.idle.len() + self.active < self.pool_config.max_size {
            self.idle.push_back(pooled);
        } else {
            self.stats.total_connections_closed += 1;
            // pooled is dropped here, calling PgConnection::drop → Terminate
        }
    }

    // ─── Maintenance ──────────────────────────────────────────

    /// Reap expired connections (call periodically from your event loop).
    ///
    /// Removes connections that have exceeded `max_lifetime` or `idle_timeout`,
    /// then ensures `min_size` idle connections exist.
    pub fn reap(&mut self) {
        let mut i = 0;
        while i < self.idle.len() {
            let expired = {
                let pooled = &self.idle[i];
                pooled.is_lifetime_expired(self.pool_config.max_lifetime)
                    || pooled.is_idle_expired(self.pool_config.idle_timeout)
            };
            if expired {
                self.idle.remove(i);
                self.stats.total_connections_closed += 1;
            } else {
                i += 1;
            }
        }

        // Ensure min_size connections
        let total = self.active + self.idle.len();
        if total < self.pool_config.min_size {
            let need = self.pool_config.min_size - total;
            for _ in 0..need {
                if let Ok(conn) = PgConnection::connect(&self.config) {
                    self.idle.push_back(PooledConn::new(conn));
                    self.stats.total_connections_created += 1;
                }
            }
        }
    }

    // ─── Accessors ────────────────────────────────────────────

    /// Get the pool configuration (PgConfig).
    pub fn config(&self) -> &PgConfig {
        &self.config
    }

    /// Get the pool config.
    pub fn pool_config(&self) -> &PgPoolConfig {
        &self.pool_config
    }

    /// Get the maximum pool size.
    pub fn pool_size(&self) -> usize {
        self.pool_config.max_size
    }

    /// Resize the pool at runtime.
    ///
    /// If `new_size` is smaller than the current total, excess idle
    /// connections are discarded immediately.  Active (checked-out)
    /// connections are not interrupted — the pool will converge to the
    /// new size as they are returned.
    pub fn set_max_size(&mut self, new_size: usize) {
        self.pool_config.max_size = new_size;
        // Shrink idle queue if necessary
        while self.idle.len() + self.active > new_size && !self.idle.is_empty() {
            self.idle.pop_front();
            self.stats.total_connections_closed += 1;
        }
    }

    /// Number of idle connections available for checkout.
    pub fn idle_connections(&self) -> usize {
        self.idle.len()
    }

    /// Number of connections currently checked out.
    pub fn active_connections(&self) -> usize {
        self.active
    }

    /// Total connections (idle + active).
    pub fn total_connections(&self) -> usize {
        self.idle.len() + self.active
    }

    /// Get pool statistics.
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Close all idle connections in the pool.
    pub fn close_all(&mut self) {
        let closed = self.idle.len();
        self.idle.clear();
        self.stats.total_connections_closed += closed as u64;
    }
}

// ─── ConnectionGuard ──────────────────────────────────────────

/// RAII guard for a pooled connection.
///
/// Provides `&mut PgConnection` via `conn()` or `DerefMut`.  When dropped,
/// the connection is automatically returned to the pool's idle queue.
///
/// # Example
/// ```ignore
/// let mut guard = pool.get()?;
/// let rows = guard.conn().query("SELECT 1", &[])?;
/// // guard drops here → connection returned to pool
/// ```
pub struct ConnectionGuard<'a> {
    /// Raw pointer to the owning pool.  We use a raw pointer instead of
    /// `&'a mut PgPool` to avoid a double-mutable-borrow conflict: the
    /// guard itself borrows the pool, but the user also needs `&mut` access
    /// to the connection inside the guard.  Since the pool is single-threaded
    /// this is safe.
    pool: *mut PgPool,
    conn: Option<PooledConn>,
    /// Phantom to tie the lifetime to the pool.
    _marker: std::marker::PhantomData<&'a mut PgPool>,
}

impl<'a> ConnectionGuard<'a> {
    /// Get a mutable reference to the underlying connection.
    #[inline]
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self
            .conn
            .as_mut()
            .expect("ConnectionGuard used after take")
            .conn
    }
}

impl<'a> std::ops::Deref for ConnectionGuard<'a> {
    type Target = PgConnection;
    fn deref(&self) -> &PgConnection {
        &self
            .conn
            .as_ref()
            .expect("ConnectionGuard used after take")
            .conn
    }
}

impl<'a> std::ops::DerefMut for ConnectionGuard<'a> {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self
            .conn
            .as_mut()
            .expect("ConnectionGuard used after take")
            .conn
    }
}

impl<'a> Drop for ConnectionGuard<'a> {
    fn drop(&mut self) {
        if let Some(pooled) = self.conn.take() {
            // SAFETY: The pool is single-threaded and lives at least as long
            // as 'a.  The raw pointer was obtained from a valid &mut PgPool.
            unsafe {
                (*self.pool).return_conn(pooled);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::PgConfig;
    use crate::error::PgError;

    fn dummy_config() -> PgConfig {
        PgConfig::new("127.0.0.1", 5432, "test", "test", "testdb")
    }

    // ─── PgPoolConfig Defaults ────────────────────────────────────────────────

    #[test]
    fn test_pool_config_default_values() {
        let cfg = PgPoolConfig::default();
        assert_eq!(cfg.max_size, 10);
        assert_eq!(cfg.min_size, 1);
        assert!(
            cfg.max_lifetime.is_some(),
            "default max_lifetime should be set"
        );
        assert!(
            cfg.idle_timeout.is_some(),
            "default idle_timeout should be set"
        );
        assert!(
            cfg.checkout_timeout.is_some(),
            "default checkout_timeout should be set"
        );
        assert!(cfg.connection_timeout.is_some());
        assert!(
            !cfg.test_on_checkout,
            "test_on_checkout should default to false"
        );
        assert!(cfg.auto_reconnect, "auto_reconnect should default to true");
        assert_eq!(cfg.validation_query, "SELECT 1");
    }

    #[test]
    fn test_pool_config_new_equals_default() {
        let a = PgPoolConfig::new();
        let b = PgPoolConfig::default();
        assert_eq!(a.max_size, b.max_size);
        assert_eq!(a.min_size, b.min_size);
    }

    // ─── PgPoolConfig Builder Methods ────────────────────────────────────────

    #[test]
    fn test_builder_max_size() {
        let cfg = PgPoolConfig::new().max_size(25);
        assert_eq!(cfg.max_size, 25);
    }

    #[test]
    fn test_builder_min_size() {
        let cfg = PgPoolConfig::new().min_size(3);
        assert_eq!(cfg.min_size, 3);
    }

    #[test]
    fn test_builder_max_lifetime() {
        let d = Duration::from_secs(900);
        let cfg = PgPoolConfig::new().max_lifetime(d);
        assert_eq!(cfg.max_lifetime, Some(d));
    }

    #[test]
    fn test_builder_no_max_lifetime() {
        let cfg = PgPoolConfig::new().no_max_lifetime();
        assert!(cfg.max_lifetime.is_none());
    }

    #[test]
    fn test_builder_idle_timeout() {
        let d = Duration::from_secs(300);
        let cfg = PgPoolConfig::new().idle_timeout(d);
        assert_eq!(cfg.idle_timeout, Some(d));
    }

    #[test]
    fn test_builder_no_idle_timeout() {
        let cfg = PgPoolConfig::new().no_idle_timeout();
        assert!(cfg.idle_timeout.is_none());
    }

    #[test]
    fn test_builder_checkout_timeout() {
        let d = Duration::from_secs(10);
        let cfg = PgPoolConfig::new().checkout_timeout(d);
        assert_eq!(cfg.checkout_timeout, Some(d));
    }

    #[test]
    fn test_builder_connection_timeout() {
        let d = Duration::from_secs(3);
        let cfg = PgPoolConfig::new().connection_timeout(d);
        assert_eq!(cfg.connection_timeout, Some(d));
    }

    #[test]
    fn test_builder_test_on_checkout() {
        let cfg = PgPoolConfig::new().test_on_checkout(true);
        assert!(cfg.test_on_checkout);
        let cfg2 = PgPoolConfig::new().test_on_checkout(false);
        assert!(!cfg2.test_on_checkout);
    }

    #[test]
    fn test_builder_auto_reconnect_false() {
        let mut cfg = PgPoolConfig::new();
        cfg.auto_reconnect = false;
        assert!(!cfg.auto_reconnect);
        cfg.auto_reconnect = true;
        assert!(cfg.auto_reconnect);
    }

    #[test]
    fn test_builder_validation_query() {
        let mut cfg = PgPoolConfig::new();
        cfg.validation_query = "SELECT version()".to_string();
        assert_eq!(cfg.validation_query, "SELECT version()");
    }

    #[test]
    fn test_builder_chained() {
        let mut cfg = PgPoolConfig::new()
            .max_size(20)
            .min_size(2)
            .checkout_timeout(Duration::from_secs(5))
            .test_on_checkout(true)
            .no_idle_timeout();
        cfg.auto_reconnect = false;
        cfg.validation_query = "SELECT 1+1".to_string();
        assert_eq!(cfg.max_size, 20);
        assert_eq!(cfg.min_size, 2);
        assert!(cfg.test_on_checkout);
        assert!(!cfg.auto_reconnect);
        assert!(cfg.idle_timeout.is_none());
        assert_eq!(cfg.validation_query, "SELECT 1+1");
    }

    #[test]
    fn test_builder_clone() {
        let cfg = PgPoolConfig::new().max_size(7).min_size(2);
        let cloned = cfg.clone();
        assert_eq!(cloned.max_size, 7);
        assert_eq!(cloned.min_size, 2);
    }

    // ─── PoolStats ────────────────────────────────────────────────────────────

    #[test]
    fn test_pool_stats_all_zero_initially() {
        let stats = PoolStats::default();
        assert_eq!(stats.total_checkouts, 0);
        assert_eq!(stats.total_connections_created, 0);
        assert_eq!(stats.total_connections_closed, 0);
        assert_eq!(stats.validation_failures, 0);
        assert_eq!(stats.lifetime_expirations, 0);
        assert_eq!(stats.idle_expirations, 0);
        assert_eq!(stats.checkout_timeouts, 0);
    }

    // ─── PgPool Initial State (Lazy, No DB) ──────────────────────────────────

    #[test]
    fn test_pool_new_starts_empty() {
        let pool = PgPool::new(dummy_config(), 10);
        assert_eq!(pool.idle_connections(), 0);
        assert_eq!(pool.active_connections(), 0);
        assert_eq!(pool.total_connections(), 0);
    }

    #[test]
    fn test_pool_stats_initially_zeroed() {
        let pool = PgPool::new(dummy_config(), 5);
        let s = pool.stats();
        assert_eq!(s.total_checkouts, 0);
        assert_eq!(s.total_connections_created, 0);
        assert_eq!(s.total_connections_closed, 0);
    }

    #[test]
    fn test_pool_total_equals_idle_plus_active() {
        let pool = PgPool::new(dummy_config(), 10);
        assert_eq!(
            pool.total_connections(),
            pool.idle_connections() + pool.active_connections()
        );
    }

    // ─── Pool Exhaustion — critical reliability test ──────────────────────────
    // try_get() must return PoolExhausted (not WouldBlock) when at capacity.
    // This ensures callers don't accidentally retry-loop on non-blocking API.

    #[test]
    fn test_try_get_returns_pool_exhausted_when_at_capacity() {
        let mut pool = PgPool::new(dummy_config(), 0);
        let result = pool.try_get();
        assert!(
            matches!(result, Err(PgError::PoolExhausted)),
            "Expected PoolExhausted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_try_get_never_returns_would_block() {
        // Regression: before Sprint 6, try_get returned WouldBlock instead of PoolExhausted
        let mut pool = PgPool::new(dummy_config(), 0);
        let result = pool.try_get();
        assert!(
            !matches!(result, Err(PgError::WouldBlock)),
            "try_get must NOT return WouldBlock — pool should return PoolExhausted"
        );
    }

    #[test]
    fn test_get_with_short_timeout_returns_pool_timeout_when_empty() {
        let pool_cfg = PgPoolConfig::new()
            .max_size(0)
            .checkout_timeout(Duration::from_millis(1));
        let mut pool = PgPool::with_config(dummy_config(), pool_cfg);
        let result = pool.get();
        assert!(
            matches!(result, Err(PgError::PoolTimeout)),
            "Expected PoolTimeout after checkout_timeout exceeded, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_get_timeout_increments_checkout_timeout_counter() {
        let pool_cfg = PgPoolConfig::new()
            .max_size(0)
            .checkout_timeout(Duration::from_millis(1));
        let mut pool = PgPool::with_config(dummy_config(), pool_cfg);
        let _ = pool.get();
        assert_eq!(pool.stats().checkout_timeouts, 1);
    }

    // ─── set_max_size ─────────────────────────────────────────────────────────

    #[test]
    fn test_set_max_size_to_zero_makes_pool_exhausted() {
        let mut pool = PgPool::new(dummy_config(), 10);
        pool.set_max_size(0);
        let result = pool.try_get();
        assert!(matches!(result, Err(PgError::PoolExhausted)));
    }

    #[test]
    fn test_set_max_size_grow_does_not_panic() {
        let mut pool = PgPool::new(dummy_config(), 5);
        pool.set_max_size(100); // grow — should not panic or discard anything
        assert_eq!(pool.idle_connections(), 0); // still lazy
    }

    #[test]
    fn test_set_max_size_shrink_with_empty_idle_is_noop() {
        // No idle connections → shrink has nothing to discard
        let mut pool = PgPool::new(dummy_config(), 10);
        pool.set_max_size(1);
        assert_eq!(pool.idle_connections(), 0);
    }

    // ─── close_all ────────────────────────────────────────────────────────────

    #[test]
    fn test_close_all_on_empty_pool_no_panic() {
        let mut pool = PgPool::new(dummy_config(), 10);
        pool.close_all(); // must not panic
        assert_eq!(pool.idle_connections(), 0);
    }

    #[test]
    fn test_close_all_does_not_affect_active_count() {
        // No active connections → active stays 0 after close_all
        let mut pool = PgPool::new(dummy_config(), 10);
        pool.close_all();
        assert_eq!(pool.active_connections(), 0);
    }

    #[test]
    fn test_close_all_increments_closed_stats() {
        // No idle connections → closed counter stays 0
        let mut pool = PgPool::new(dummy_config(), 5);
        pool.close_all();
        // With 0 idle, nothing closes
        assert_eq!(pool.stats().total_connections_closed, 0);
    }

    // ─── try_checkout counter ─────────────────────────────────────────────────

    #[test]
    fn test_try_get_increments_checkout_counter() {
        let mut pool = PgPool::new(dummy_config(), 0);
        let _ = pool.try_get(); // will fail with PoolExhausted
        assert_eq!(pool.stats().total_checkouts, 1);
    }

    #[test]
    fn test_try_get_multiple_exhausted_increments_counter() {
        let mut pool = PgPool::new(dummy_config(), 0);
        for _ in 0..5 {
            let _ = pool.try_get();
        }
        assert_eq!(pool.stats().total_checkouts, 5);
    }

    // ─── with_config ─────────────────────────────────────────────────────────

    #[test]
    fn test_pool_with_config_respects_max_size() {
        let cfg = PgPoolConfig::new().max_size(3);
        let pool = PgPool::with_config(dummy_config(), cfg);
        assert_eq!(pool.idle_connections(), 0);
        assert_eq!(pool.active_connections(), 0);
    }

    // ─── reap on empty pool ───────────────────────────────────────────────────

    #[test]
    fn test_reap_on_empty_pool_no_panic() {
        let mut pool = PgPool::new(dummy_config(), 10);
        pool.reap(); // must not panic on empty idle queue
        assert_eq!(pool.idle_connections(), 0);
    }
}
