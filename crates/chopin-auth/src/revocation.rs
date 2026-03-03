//! Token revocation via a JWT ID (JTI) blacklist.
//!
//! [`TokenBlacklist`] stores revoked JTIs with optional expiry timestamps.
//! Entries are automatically treated as un-revoked after their expiry, and
//! [`TokenBlacklist::cleanup`] removes them from memory to prevent unbounded growth.
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A thread-safe blacklist of revoked JWT IDs (JTIs).
///
/// Each entry carries an optional expiry timestamp (Unix seconds). Once the
/// expiry passes the entry is treated as not-revoked. Call
/// [`cleanup`](TokenBlacklist::cleanup) periodically to reclaim memory.
///
/// # Example
/// ```
/// # use chopin_auth::TokenBlacklist;
/// let bl = TokenBlacklist::new();
/// // Revoke a token that expires at Unix timestamp 9_999_999_999.
/// bl.revoke("jti-abc".into(), Some(9_999_999_999));
/// assert!(bl.is_revoked("jti-abc"));
/// ```
#[derive(Clone, Default)]
pub struct TokenBlacklist {
    /// Maps jti → optional expiry (Unix seconds). `None` = revoked indefinitely.
    revoked: Arc<RwLock<HashMap<String, Option<u64>>>>,
}

impl TokenBlacklist {
    /// Create a new empty blacklist.
    pub fn new() -> Self {
        Self {
            revoked: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Revoke a token by its JTI.
    ///
    /// `expires_at` should be the token's own `exp` claim (Unix seconds) so
    /// the entry can be cleaned up once the token's natural lifetime ends.
    /// Pass `None` to revoke indefinitely.
    pub fn revoke(&self, jti: String, expires_at: Option<u64>) {
        if let Ok(mut lock) = self.revoked.write() {
            lock.insert(jti, expires_at);
        }
    }

    /// Returns `true` if `jti` is present **and** its expiry has not yet passed.
    ///
    /// Fails closed: returns `true` if the internal lock is poisoned.
    pub fn is_revoked(&self, jti: &str) -> bool {
        let Ok(lock) = self.revoked.read() else {
            return true; // fail closed
        };
        match lock.get(jti) {
            None => false,
            Some(None) => true,                    // revoked indefinitely
            Some(Some(exp)) => now_secs() <= *exp, // revoked until exp
        }
    }

    /// Remove all entries whose expiry timestamp has already passed.
    ///
    /// Call this from a background thread or task to prevent unbounded memory growth.
    pub fn cleanup(&self) {
        let now = now_secs();
        if let Ok(mut lock) = self.revoked.write() {
            lock.retain(|_, exp| match exp {
                None => true,
                Some(exp) => now <= *exp,
            });
        }
    }

    /// Spawn a background thread that calls [`TokenBlacklist::cleanup`] on a
    /// fixed `interval`, preventing unbounded memory growth from accumulated
    /// expired entries.
    ///
    /// The thread runs until the process exits. Because [`TokenBlacklist`] is
    /// backed by an [`Arc`], the spawned thread holds its own clone and will
    /// not prevent the original value from being dropped — the cleanup simply
    /// becomes a no-op once the last reference is gone.
    ///
    /// # Example
    /// ```rust,ignore
    /// use std::time::Duration;
    /// use chopin_auth::TokenBlacklist;
    ///
    /// let bl = TokenBlacklist::new();
    /// bl.start_cleanup_task(Duration::from_secs(300)); // clean up every 5 min
    /// ```
    pub fn start_cleanup_task(&self, interval: std::time::Duration) {
        let bl = self.clone();
        std::thread::Builder::new()
            .name("chopin-auth-blacklist-cleanup".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(interval);
                    bl.cleanup();
                }
            })
            .expect("failed to spawn blacklist cleanup thread");
    }

    /// Number of currently tracked entries (including expired ones not yet cleaned up).
    pub fn len(&self) -> usize {
        self.revoked.read().map(|l| l.len()).unwrap_or(0)
    }

    /// Returns `true` if the blacklist contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix timestamp far in the future (year 2286) — tokens revoked with this
    /// expiry are treated as currently revoked throughout the test suite.
    const FAR_FUTURE: u64 = 9_999_999_999;

    #[test]
    fn test_new_is_not_revoked() {
        let bl = TokenBlacklist::new();
        assert!(!bl.is_revoked("any-jti"));
    }

    #[test]
    fn test_default_equals_new_behavior() {
        let bl: TokenBlacklist = Default::default();
        assert!(!bl.is_revoked("x"));
    }

    #[test]
    fn test_revoke_then_is_revoked() {
        let bl = TokenBlacklist::new();
        bl.revoke("jti-abc".to_string(), Some(FAR_FUTURE));
        assert!(bl.is_revoked("jti-abc"));
    }

    #[test]
    fn test_revoke_indefinitely() {
        let bl = TokenBlacklist::new();
        bl.revoke("jti-perm".to_string(), None);
        assert!(bl.is_revoked("jti-perm"));
    }

    #[test]
    fn test_expired_entry_not_revoked() {
        let bl = TokenBlacklist::new();
        // expiry of 1 second past Unix epoch — already expired
        bl.revoke("jti-old".to_string(), Some(1));
        assert!(!bl.is_revoked("jti-old"));
    }

    #[test]
    fn test_not_revoked_unknown_jti() {
        let bl = TokenBlacklist::new();
        bl.revoke("jti-1".to_string(), None);
        assert!(!bl.is_revoked("jti-2"));
    }

    #[test]
    fn test_revoke_idempotent() {
        let bl = TokenBlacklist::new();
        bl.revoke("same-jti".to_string(), Some(FAR_FUTURE));
        bl.revoke("same-jti".to_string(), Some(FAR_FUTURE));
        assert!(bl.is_revoked("same-jti"));
    }

    #[test]
    fn test_clone_shares_arc_state() {
        let bl1 = TokenBlacklist::new();
        let bl2 = bl1.clone();
        bl1.revoke("shared-jti".to_string(), None);
        // Both clones share the underlying Arc<RwLock<HashMap>>.
        assert!(bl2.is_revoked("shared-jti"));
    }

    #[test]
    fn test_revoke_multiple_unique_jtis() {
        let bl = TokenBlacklist::new();
        for i in 0..10 {
            bl.revoke(format!("jti-{i}"), None);
        }
        for i in 0..10 {
            assert!(bl.is_revoked(&format!("jti-{i}")));
        }
        assert!(!bl.is_revoked("jti-99"));
    }

    #[test]
    fn test_scale_100_jtis() {
        let bl = TokenBlacklist::new();
        for i in 0..100 {
            bl.revoke(format!("scale-jti-{i}"), Some(FAR_FUTURE));
        }
        for i in 0..100 {
            assert!(bl.is_revoked(&format!("scale-jti-{i}")));
        }
        assert!(!bl.is_revoked("scale-jti-100"));
    }

    #[test]
    fn test_cleanup_removes_expired_entries() {
        let bl = TokenBlacklist::new();
        bl.revoke("expired".to_string(), Some(1)); // Unix epoch 1 — already passed
        bl.revoke("live".to_string(), Some(FAR_FUTURE));
        bl.revoke("perm".to_string(), None);
        assert_eq!(bl.len(), 3);
        bl.cleanup();
        assert_eq!(bl.len(), 2);
        assert!(!bl.is_revoked("expired"));
        assert!(bl.is_revoked("live"));
        assert!(bl.is_revoked("perm"));
    }

    #[test]
    fn test_len_and_is_empty() {
        let bl = TokenBlacklist::new();
        assert!(bl.is_empty());
        bl.revoke("j".to_string(), None);
        assert_eq!(bl.len(), 1);
        assert!(!bl.is_empty());
    }
}
