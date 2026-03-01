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
