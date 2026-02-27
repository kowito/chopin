//! Implicit statement caching — automatically reuse parsed statements.

use crate::codec::ColumnDesc;
use std::collections::HashMap;

/// A cached prepared statement with its row description.
#[derive(Debug, Clone)]
pub struct CachedStatement {
    /// The server-side statement name (e.g., "s0", "s1", ...).
    pub name: String,
    /// Number of parameter slots.
    pub param_count: usize,
    /// Cached RowDescription from the first execution (if available).
    pub columns: Option<Vec<ColumnDesc>>,
}

/// Implicit statement cache: maps SQL text → CachedStatement.
///
/// This is local to each connection (worker-local), so no synchronization needed.
pub struct StatementCache {
    cache: HashMap<u64, CachedStatement>,
    counter: u32,
}

/// A reference to a cached or new statement.
pub struct Statement {
    /// Statement name on the server.
    pub name: String,
    /// Whether this was freshly parsed (needs Parse + Describe).
    pub is_new: bool,
    /// Cached column descriptions (if previously executed).
    pub columns: Option<Vec<ColumnDesc>>,
}

impl Default for StatementCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StatementCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_capacity(256),
            counter: 0,
        }
    }

    /// Look up or create a statement for the given SQL.
    /// Returns a Statement reference indicating whether it's new or cached.
    pub fn get_or_create(&mut self, sql: &str) -> Statement {
        let hash = Self::hash_sql(sql);

        if let Some(cached) = self.cache.get(&hash) {
            Statement {
                name: cached.name.clone(),
                is_new: false,
                columns: cached.columns.clone(),
            }
        } else {
            let name = format!("s{}", self.counter);
            self.counter += 1;
            Statement {
                name,
                is_new: true,
                columns: None,
            }
        }
    }

    /// Cache a statement after successful Parse + Describe.
    pub fn insert(
        &mut self,
        sql: &str,
        name: String,
        param_count: usize,
        columns: Option<Vec<ColumnDesc>>,
    ) {
        let hash = Self::hash_sql(sql);
        self.cache.insert(
            hash,
            CachedStatement {
                name,
                param_count,
                columns,
            },
        );
    }

    /// Update the cached RowDescription for a statement.
    pub fn update_columns(&mut self, sql: &str, columns: Vec<ColumnDesc>) {
        let hash = Self::hash_sql(sql);
        if let Some(cached) = self.cache.get_mut(&hash) {
            cached.columns = Some(columns);
        }
    }

    /// Number of cached statements.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear all cached statements.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.counter = 0;
    }

    /// FNV-1a hash for SQL strings (fast, no allocations).
    fn hash_sql(sql: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in sql.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}
