// src/revocation.rs
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct TokenBlacklist {
    revoked_jtis: Arc<RwLock<HashSet<String>>>,
}

impl TokenBlacklist {
    pub fn new() -> Self {
        Self {
            revoked_jtis: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Revoke a specific JWT ID (jti)
    pub fn revoke(&self, jti: String) {
        if let Ok(mut lock) = self.revoked_jtis.write() {
            lock.insert(jti);
        }
    }

    /// Check if a jti is revoked
    pub fn is_revoked(&self, jti: &str) -> bool {
        if let Ok(lock) = self.revoked_jtis.read() {
            lock.contains(jti)
        } else {
            // If poisoned, fail closed
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        bl.revoke("jti-abc".to_string());
        assert!(bl.is_revoked("jti-abc"));
    }

    #[test]
    fn test_not_revoked_unknown_jti() {
        let bl = TokenBlacklist::new();
        bl.revoke("jti-1".to_string());
        assert!(!bl.is_revoked("jti-2"));
    }

    #[test]
    fn test_revoke_idempotent() {
        let bl = TokenBlacklist::new();
        bl.revoke("same-jti".to_string());
        bl.revoke("same-jti".to_string()); // no panic, still revoked
        assert!(bl.is_revoked("same-jti"));
    }

    #[test]
    fn test_clone_shares_arc_state() {
        let bl1 = TokenBlacklist::new();
        let bl2 = bl1.clone();
        bl1.revoke("shared-jti".to_string());
        // Both clones share the underlying Arc<RwLock<HashSet>>
        assert!(bl2.is_revoked("shared-jti"));
    }

    #[test]
    fn test_revoke_multiple_unique_jtis() {
        let bl = TokenBlacklist::new();
        for i in 0..10 {
            bl.revoke(format!("jti-{}", i));
        }
        for i in 0..10 {
            assert!(bl.is_revoked(&format!("jti-{}", i)));
        }
        assert!(!bl.is_revoked("jti-99"));
    }

    #[test]
    fn test_scale_100_jtis() {
        let bl = TokenBlacklist::new();
        for i in 0..100 {
            bl.revoke(format!("scale-jti-{}", i));
        }
        for i in 0..100 {
            assert!(bl.is_revoked(&format!("scale-jti-{}", i)));
        }
        assert!(!bl.is_revoked("scale-jti-100"));
    }
}
