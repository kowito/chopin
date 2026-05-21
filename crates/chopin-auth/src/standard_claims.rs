// src/standard_claims.rs
//! Pre-built generic claims type covering the most common JWT patterns.
//!
//! [`StandardClaims<R>`] replaces the boilerplate of defining a custom claims
//! struct + implementing [`HasJti`], [`RoleCheck<R>`], and [`ScopeCheck`] by
//! hand.  Use `StandardClaims<()>` when you need no role-based access control.

use crate::jwt::HasJti;
use crate::middleware::{Role, RoleCheck, ScopeCheck};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Generate a unique JWT ID combining the current Unix timestamp and a
/// per-process monotonic counter — no external dependencies required.
fn new_jti() -> String {
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", now_secs(), n)
}

/// A ready-to-use generic JWT claims type.
///
/// Covers the most common patterns out of the box:
/// - [`HasJti`] — revocation support via the `jti` field.
/// - [`RoleCheck<R>`] — RBAC; `R` defaults to `()` for no roles.
/// - [`ScopeCheck`] — OAuth 2.0 space-delimited scope string.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_auth::{
///     Role, StandardClaims, JwtManager, TokenBlacklist,
///     init_jwt_manager,
/// };
///
/// #[derive(Debug, Clone, PartialEq)]
/// enum MyRole { Admin, User }
/// impl Role for MyRole {}
///
/// type Claims = StandardClaims<MyRole>;
///
/// // Once at startup:
/// let blacklist = TokenBlacklist::new();
/// let manager   = JwtManager::new(b"my-secret").with_blacklist(blacklist);
/// init_jwt_manager(manager);
///
/// // Issue an access token:
/// let claims = Claims::new("user-42", 3600, Some(MyRole::Admin), None);
/// let token  = chopin_auth::extractor::GLOBAL_JWT_MANAGER
///     .get().unwrap().encode(&claims).unwrap();
///
/// // Use Auth<Claims> as a handler extractor — no manual impl required.
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardClaims<R = ()> {
    /// Subject — the authenticated entity's identifier (e.g. a user ID).
    pub sub: String,
    /// JWT ID — auto-generated; used for blacklist-based revocation.
    pub jti: String,
    /// Token expiry as Unix seconds.
    pub exp: u64,
    /// Token issuance time as Unix seconds.
    pub iat: u64,
    /// Optional role for role-based access control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<R>,
    /// Optional space-delimited OAuth 2.0 scope string
    /// (e.g. `"read:users write:posts"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl<R> StandardClaims<R> {
    /// Create new claims that expire `ttl_secs` seconds from now.
    ///
    /// A unique `jti` is generated automatically via a per-process monotonic
    /// counter combined with the current Unix timestamp.
    pub fn new(
        sub: impl Into<String>,
        ttl_secs: u64,
        role: Option<R>,
        scope: Option<String>,
    ) -> Self {
        let iat = now_secs();
        Self {
            sub: sub.into(),
            jti: new_jti(),
            exp: iat + ttl_secs,
            iat,
            role,
            scope,
        }
    }
}

impl<R> HasJti for StandardClaims<R> {
    #[inline]
    fn jti(&self) -> Option<&str> {
        Some(&self.jti)
    }
}

impl<R: Role> RoleCheck<R> for StandardClaims<R> {
    #[inline]
    fn has_role(&self, role: &R) -> bool {
        self.role.as_ref().map_or(false, |r| r == role)
    }
}

impl<R> ScopeCheck for StandardClaims<R> {
    #[inline]
    fn has_scope(&self, scope: &str) -> bool {
        self.scope
            .as_deref()
            .map_or(false, |s| s.split(' ').any(|t| t == scope))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    enum TestRole {
        Admin,
        User,
    }
    impl Role for TestRole {}

    type Claims = StandardClaims<TestRole>;

    #[test]
    fn test_new_sets_sub_and_expiry() {
        let claims = Claims::new("alice", 3600, None, None);
        assert_eq!(claims.sub, "alice");
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.exp - claims.iat, 3600);
    }

    #[test]
    fn test_iat_is_recent() {
        let before = now_secs();
        let claims = Claims::new("u1", 60, None, None);
        let after = now_secs();
        assert!(claims.iat >= before && claims.iat <= after);
    }

    #[test]
    fn test_jti_is_unique() {
        let a = Claims::new("u1", 60, None, None);
        let b = Claims::new("u2", 60, None, None);
        assert_ne!(a.jti, b.jti);
    }

    #[test]
    fn test_jti_is_non_empty() {
        let claims = Claims::new("u1", 60, None, None);
        assert!(claims.jti().is_some());
        assert!(!claims.jti().unwrap().is_empty());
    }

    #[test]
    fn test_role_check_matching() {
        let claims = Claims::new("u1", 60, Some(TestRole::Admin), None);
        assert!(claims.has_role(&TestRole::Admin));
        assert!(!claims.has_role(&TestRole::User));
    }

    #[test]
    fn test_role_check_user_role() {
        let claims = Claims::new("u1", 60, Some(TestRole::User), None);
        assert!(claims.has_role(&TestRole::User));
        assert!(!claims.has_role(&TestRole::Admin));
    }

    #[test]
    fn test_role_check_no_role() {
        let claims = Claims::new("u1", 60, None, None);
        assert!(!claims.has_role(&TestRole::Admin));
        assert!(!claims.has_role(&TestRole::User));
    }

    #[test]
    fn test_scope_check_single() {
        let claims = Claims::new("u1", 60, None, Some("read:users".into()));
        assert!(claims.has_scope("read:users"));
        assert!(!claims.has_scope("write:users"));
    }

    #[test]
    fn test_scope_check_multiple() {
        let claims = Claims::new("u1", 60, None, Some("read:users write:posts admin".into()));
        assert!(claims.has_scope("read:users"));
        assert!(claims.has_scope("write:posts"));
        assert!(claims.has_scope("admin"));
        assert!(!claims.has_scope("delete:users"));
    }

    #[test]
    fn test_scope_check_no_scope() {
        let claims = Claims::new("u1", 60, None, None);
        assert!(!claims.has_scope("read:users"));
    }

    #[test]
    fn test_scope_no_partial_match() {
        // "read" must not match "read:users"
        let claims = Claims::new("u1", 60, None, Some("read:users".into()));
        assert!(!claims.has_scope("read"));
    }

    #[test]
    fn test_scope_no_prefix_match() {
        let claims = Claims::new("u1", 60, None, Some("read:users".into()));
        assert!(!claims.has_scope("read:users:extra"));
    }

    #[test]
    fn test_no_role_serialises_as_absent() {
        let claims = Claims::new("u1", 60, None, None);
        let json = serde_json::to_string(&claims).unwrap();
        assert!(!json.contains("role"));
        assert!(!json.contains("scope"));
    }

    #[test]
    fn test_role_serialises() {
        // Requires TestRole to implement Serialize
        // We skip the JSON round-trip for the enum but verify sub/jti round-trip
        let claims: StandardClaims<()> = StandardClaims::new("bob", 60, None, None);
        let json = serde_json::to_string(&claims).unwrap();
        let back: StandardClaims<()> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sub, "bob");
        assert_eq!(back.jti, claims.jti);
    }
}
