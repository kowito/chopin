//! Implicit statement caching with LRU eviction.
//!
//! Each connection maintains its own local cache — no synchronization needed.
//! When the cache exceeds `max_capacity`, the least-recently-used entry is
//! evicted and a Close message should be sent to the server.

use crate::codec::ColumnDesc;
use std::collections::HashMap;

/// Default maximum number of cached statements before LRU eviction kicks in.
const DEFAULT_MAX_CAPACITY: usize = 256;

/// A cached prepared statement with its row description.
#[derive(Debug, Clone)]
pub struct CachedStatement {
    /// The server-side statement name (e.g., "s0", "s1", ...).
    pub name: String,
    /// Number of parameter slots.
    pub param_count: usize,
    /// Cached RowDescription from the first execution (if available).
    pub columns: Option<Vec<ColumnDesc>>,
    /// Monotonic access counter for LRU ordering.
    access_tick: u64,
}

/// Implicit statement cache: maps SQL text → CachedStatement.
///
/// This is local to each connection (worker-local), so no synchronization needed.
/// Uses LRU eviction when the cache exceeds `max_capacity`.
pub struct StatementCache {
    cache: HashMap<u64, CachedStatement>,
    counter: u32,
    /// Monotonic tick counter — incremented on every access.
    tick: u64,
    /// Maximum number of entries before LRU eviction.
    max_capacity: usize,
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

/// Info about an evicted statement, so the caller can send a Close message.
#[derive(Debug)]
pub struct EvictedStatement {
    /// The server-side statement name that should be closed.
    pub name: String,
}

impl Default for StatementCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StatementCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_capacity(DEFAULT_MAX_CAPACITY),
            counter: 0,
            tick: 0,
            max_capacity: DEFAULT_MAX_CAPACITY,
        }
    }

    /// Create a cache with a custom maximum capacity.
    pub fn with_capacity(max_capacity: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_capacity.min(256)),
            counter: 0,
            tick: 0,
            max_capacity,
        }
    }

    /// Set the maximum capacity. Does not immediately evict.
    pub fn set_max_capacity(&mut self, max_capacity: usize) {
        self.max_capacity = max_capacity;
    }

    /// Get the maximum capacity.
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Look up or create a statement for the given SQL.
    /// Returns a Statement reference indicating whether it's new or cached.
    pub fn get_or_create(&mut self, sql: &str) -> Statement {
        let hash = Self::hash_sql(sql);
        self.tick += 1;
        let current_tick = self.tick;

        if let Some(cached) = self.cache.get_mut(&hash) {
            cached.access_tick = current_tick;
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
    /// Returns an evicted statement name if the cache was full.
    pub fn insert(
        &mut self,
        sql: &str,
        name: String,
        param_count: usize,
        columns: Option<Vec<ColumnDesc>>,
    ) -> Option<EvictedStatement> {
        let hash = Self::hash_sql(sql);
        self.tick += 1;
        let evicted = if self.cache.len() >= self.max_capacity && !self.cache.contains_key(&hash) {
            self.evict_lru()
        } else {
            None
        };

        self.cache.insert(
            hash,
            CachedStatement {
                name,
                param_count,
                columns,
                access_tick: self.tick,
            },
        );
        evicted
    }

    /// Evict the least-recently-used entry. Returns its name for Close.
    fn evict_lru(&mut self) -> Option<EvictedStatement> {
        if self.cache.is_empty() {
            return None;
        }
        let lru_key = *self
            .cache
            .iter()
            .min_by_key(|(_, v)| v.access_tick)
            .map(|(k, _)| k)?;
        let evicted = self.cache.remove(&lru_key)?;
        Some(EvictedStatement { name: evicted.name })
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
    ///
    /// NOTE: The statement name counter is NOT reset to zero.  This prevents
    /// name collisions with server-side prepared statements that might still
    /// exist (e.g. when `DEALLOCATE ALL` hasn't been sent yet).
    pub fn clear(&mut self) {
        self.cache.clear();
        // Intentionally keep self.counter so new names don't collide with
        // stale server-side statements.
        self.tick = 0;
    }

    /// Return the names of all cached statements (for server-side Close).
    pub fn cached_names(&self) -> Vec<String> {
        self.cache.values().map(|c| c.name.clone()).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let mut cache = StatementCache::new();
        let stmt = cache.get_or_create("SELECT 1");
        assert!(stmt.is_new);
        assert_eq!(stmt.name, "s0");

        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        let stmt2 = cache.get_or_create("SELECT 1");
        assert!(!stmt2.is_new);
        assert_eq!(stmt2.name, "s0");
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = StatementCache::with_capacity(3);

        // Fill cache to capacity
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        cache.insert("SELECT 2", "s1".to_string(), 0, None);
        cache.insert("SELECT 3", "s2".to_string(), 0, None);
        assert_eq!(cache.len(), 3);

        // Access "SELECT 1" to make it recently used
        let _ = cache.get_or_create("SELECT 1");

        // Insert a 4th — should evict "SELECT 2" (oldest access_tick)
        let evicted = cache.insert("SELECT 4", "s3".to_string(), 0, None);
        assert!(evicted.is_some());
        assert_eq!(evicted.unwrap().name, "s1"); // s1 = "SELECT 2"
        assert_eq!(cache.len(), 3);

        // "SELECT 2" should be gone, "SELECT 1" still present
        let stmt = cache.get_or_create("SELECT 2");
        assert!(stmt.is_new);
        let stmt = cache.get_or_create("SELECT 1");
        assert!(!stmt.is_new);
    }

    #[test]
    fn test_cache_no_eviction_under_capacity() {
        let mut cache = StatementCache::with_capacity(10);
        let evicted = cache.insert("SELECT 1", "s0".to_string(), 0, None);
        assert!(evicted.is_none());
        let evicted = cache.insert("SELECT 2", "s1".to_string(), 0, None);
        assert!(evicted.is_none());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = StatementCache::new();
        // Use get_or_create to properly advance the counter (counter only
        // increments in get_or_create, not in insert).
        let s0 = cache.get_or_create("SELECT 1"); // counter → 1, name = "s0"
        cache.insert("SELECT 1", s0.name, 0, None);
        let s1 = cache.get_or_create("SELECT 2"); // counter → 2, name = "s1"
        cache.insert("SELECT 2", s1.name, 0, None);
        assert_eq!(cache.len(), 2);
        cache.clear(); // clears entries but preserves counter = 2
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        // Counter should NOT reset — new names start after last used counter
        let stmt = cache.get_or_create("SELECT 3");
        assert!(stmt.is_new);
        assert_eq!(stmt.name, "s2"); // s2, not s0!
    }

    // ─── cached_names() ───────────────────────────────────────────────────────

    #[test]
    fn test_cached_names_empty() {
        let cache = StatementCache::new();
        let names = cache.cached_names();
        assert!(names.is_empty());
    }

    #[test]
    fn test_cached_names_after_inserts() {
        let mut cache = StatementCache::new();
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        cache.insert("SELECT 2", "s1".to_string(), 0, None);
        cache.insert("SELECT 3", "s2".to_string(), 0, None);
        let mut names = cache.cached_names();
        names.sort(); // HashMap order is non-deterministic
        assert_eq!(names, vec!["s0", "s1", "s2"]);
    }

    #[test]
    fn test_cached_names_clear_then_empty() {
        let mut cache = StatementCache::new();
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        cache.clear();
        assert!(cache.cached_names().is_empty());
    }

    // ─── update_columns() ────────────────────────────────────────────────────

    #[test]
    fn test_update_columns_existing_statement() {
        use crate::codec::ColumnDesc;
        use crate::protocol::FormatCode;
        let mut cache = StatementCache::new();
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        // Initially no columns
        let stmt = cache.get_or_create("SELECT 1");
        assert!(stmt.columns.is_none());
        // Update columns
        let cols = vec![ColumnDesc {
            name: "id".to_string(),
            table_oid: 0,
            col_attr: 0,
            type_oid: 23,
            type_size: 4,
            type_modifier: -1,
            format_code: FormatCode::Text,
        }];
        cache.update_columns("SELECT 1", cols);
        let stmt2 = cache.get_or_create("SELECT 1");
        assert!(!stmt2.is_new);
        assert!(stmt2.columns.is_some());
        let cached_cols = stmt2.columns.unwrap();
        assert_eq!(cached_cols[0].name, "id");
    }

    #[test]
    fn test_update_columns_missing_statement_no_panic() {
        let mut cache = StatementCache::new();
        // update_columns on a key that doesn't exist — must not panic
        cache.update_columns("SELECT nonexistent", vec![]);
        // cache remains empty (no insertion)
        assert_eq!(cache.len(), 0);
    }

    // ─── set_max_capacity() ──────────────────────────────────────────────────

    #[test]
    fn test_set_max_capacity_reduces_future_capacity() {
        let mut cache = StatementCache::with_capacity(10);
        assert_eq!(cache.max_capacity(), 10);
        cache.set_max_capacity(3);
        assert_eq!(cache.max_capacity(), 3);
    }

    #[test]
    fn test_set_max_capacity_to_one_evicts_on_next_insert() {
        let mut cache = StatementCache::with_capacity(1);
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        assert_eq!(cache.len(), 1);
        // A second insert should evict the first
        let evicted = cache.insert("SELECT 2", "s1".to_string(), 0, None);
        assert!(evicted.is_some(), "Should evict when over capacity");
        assert_eq!(cache.len(), 1);
    }

    // ─── Counter preservation — reliability ──────────────────────────────────

    #[test]
    fn test_counter_never_resets_across_clears() {
        let mut cache = StatementCache::new();
        // Fill via get_or_create (which advances the counter) then clear 3 times.
        for i in 0..3 {
            let sql = format!("SELECT {}", i);
            let stmt = cache.get_or_create(&sql); // advances counter each round
            cache.insert(&sql, stmt.name, 0, None);
            cache.clear(); // clears entries, keeps counter
        }
        // Counter should be >= 3, definitely not 0
        let stmt = cache.get_or_create("SELECT final");
        assert!(stmt.is_new);
        // Name should be s3 or higher — never s0 again after clears
        let counter: u32 = stmt.name.trim_start_matches('s').parse().unwrap();
        assert!(
            counter >= 3,
            "Counter should not reset to 0 after clear — got {}",
            stmt.name
        );
    }

    #[test]
    fn test_counter_strictly_increases() {
        let mut cache = StatementCache::new();
        let s0 = cache.get_or_create("SELECT 0");
        cache.insert("SELECT 0", s0.name.clone(), 0, None);
        let s1 = cache.get_or_create("SELECT 1");
        cache.insert("SELECT 1", s1.name.clone(), 0, None);
        let s2 = cache.get_or_create("SELECT 2");
        // Names should be s0, s1, s2
        assert_eq!(s0.name, "s0");
        assert_eq!(s1.name, "s1");
        assert_eq!(s2.name, "s2");
    }

    // ─── Hash consistency — reliability ──────────────────────────────────────

    #[test]
    fn test_same_sql_always_hits_cache() {
        let mut cache = StatementCache::new();
        let sql = "SELECT id, name FROM users WHERE id = $1";
        cache.insert(sql, "s0".to_string(), 1, None);
        // 100 lookups should all be cache hits
        for _ in 0..100 {
            let stmt = cache.get_or_create(sql);
            assert!(!stmt.is_new, "repeat lookup should be a cache hit");
            assert_eq!(stmt.name, "s0");
        }
    }

    #[test]
    fn test_different_sql_different_cache_entry() {
        let mut cache = StatementCache::new();
        let stmt_a = cache.get_or_create("SELECT 1");
        cache.insert("SELECT 1", stmt_a.name.clone(), 0, None);
        let stmt_b = cache.get_or_create("SELECT 2");
        cache.insert("SELECT 2", stmt_b.name.clone(), 0, None);
        assert_ne!(stmt_a.name, stmt_b.name);
        // Both still in cache afterwards
        let hit_a = cache.get_or_create("SELECT 1");
        let hit_b = cache.get_or_create("SELECT 2");
        assert!(!hit_a.is_new);
        assert!(!hit_b.is_new);
    }

    #[test]
    fn test_whitespace_difference_creates_separate_entry() {
        // "SELECT 1" and "SELECT  1" (extra space) should be separate cache entries
        let mut cache = StatementCache::new();
        let a = cache.get_or_create("SELECT 1");
        cache.insert("SELECT 1", a.name.clone(), 0, None);
        let b = cache.get_or_create("SELECT  1");
        assert!(
            b.is_new,
            "SQL with different whitespace should be a new statement"
        );
        assert_ne!(a.name, b.name);
    }

    // ─── LRU ordering — scalability ──────────────────────────────────────────

    #[test]
    fn test_lru_evicts_true_lru_not_fifo() {
        let mut cache = StatementCache::with_capacity(3);
        cache.insert("A", "s0".to_string(), 0, None);
        cache.insert("B", "s1".to_string(), 0, None);
        cache.insert("C", "s2".to_string(), 0, None);
        // Access A and C — makes B the least recently used
        let _ = cache.get_or_create("A");
        let _ = cache.get_or_create("C");
        // Insert D — should evict B (LRU)
        let evicted = cache.insert("D", "s3".to_string(), 0, None);
        assert!(evicted.is_some());
        assert_eq!(
            evicted.unwrap().name,
            "s1",
            "B (s1) should be evicted as LRU"
        );
        // A and C still accessible
        assert!(!cache.get_or_create("A").is_new);
        assert!(!cache.get_or_create("C").is_new);
        // B is gone
        assert!(cache.get_or_create("B").is_new);
    }

    #[test]
    fn test_insert_same_key_no_eviction() {
        let mut cache = StatementCache::with_capacity(2);
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        cache.insert("SELECT 2", "s1".to_string(), 0, None);
        // Re-inserting existing key should not trigger eviction
        let evicted = cache.insert("SELECT 1", "s0".to_string(), 1, None);
        assert!(
            evicted.is_none(),
            "Re-inserting an existing key should not evict"
        );
        assert_eq!(cache.len(), 2);
    }

    // ─── Scale: large cache ───────────────────────────────────────────────────

    #[test]
    fn test_large_cache_stays_under_capacity() {
        let capacity = 256;
        let mut cache = StatementCache::with_capacity(capacity);
        // Insert 300 different statements — cache should not exceed capacity
        for i in 0..300 {
            let sql = format!("SELECT {} FROM t", i);
            let name = format!("s{}", i);
            cache.insert(&sql, name, 0, None);
        }
        assert!(
            cache.len() <= capacity,
            "Cache exceeded capacity: {}",
            cache.len()
        );
    }

    #[test]
    fn test_cache_len_is_empty_consistent() {
        let mut cache = StatementCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        cache.insert("SELECT 1", "s0".to_string(), 0, None);
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
