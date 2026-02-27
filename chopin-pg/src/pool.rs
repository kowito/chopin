//! Worker-local connection pool for Shared-Nothing architecture.
//!
//! Each worker thread owns its own `PgPool` instance.
//! No locks, no cross-thread synchronization.

use crate::connection::{PgConnection, PgConfig};
use crate::error::{PgError, PgResult};

/// A simple worker-local connection pool.
///
/// Maintains a fixed number of connections per worker thread.
pub struct PgPool {
    config: PgConfig,
    connections: Vec<Option<PgConnection>>,
    size: usize,
    checkout_index: usize,
}

impl PgPool {
    /// Create a new pool with the given configuration and size.
    /// Connections are lazily initialized on first use.
    pub fn new(config: PgConfig, size: usize) -> Self {
        let mut connections = Vec::with_capacity(size);
        for _ in 0..size {
            connections.push(None);
        }
        Self {
            config,
            connections,
            size,
            checkout_index: 0,
        }
    }

    /// Create a pool and eagerly initialize all connections.
    pub fn connect(config: PgConfig, size: usize) -> PgResult<Self> {
        let mut pool = Self::new(config, size);
        for i in 0..size {
            let conn = PgConnection::connect(&pool.config)?;
            pool.connections[i] = Some(conn);
        }
        Ok(pool)
    }

    /// Get a connection from the pool (round-robin).
    /// If the connection slot is empty, creates a new one.
    pub fn get(&mut self) -> PgResult<&mut PgConnection> {
        let idx = self.checkout_index % self.size;
        self.checkout_index = self.checkout_index.wrapping_add(1);

        if self.connections[idx].is_none() {
            let conn = PgConnection::connect(&self.config)?;
            self.connections[idx] = Some(conn);
        }

        self.connections[idx]
            .as_mut()
            .ok_or(PgError::ConnectionClosed)
    }

    /// Get the pool configuration.
    pub fn config(&self) -> &PgConfig {
        &self.config
    }

    /// Get the pool size.
    pub fn pool_size(&self) -> usize {
        self.size
    }

    /// Get the number of active (initialized) connections.
    pub fn active_connections(&self) -> usize {
        self.connections.iter().filter(|c| c.is_some()).count()
    }
}
