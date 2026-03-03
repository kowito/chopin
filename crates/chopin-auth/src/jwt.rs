// src/jwt.rs
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

use crate::revocation::TokenBlacklist;

// ─── Error type ──────────────────────────────────────────────────────────────

/// Errors that can occur during JWT encode / decode operations.
#[derive(Debug)]
pub enum AuthError {
    /// The token is syntactically invalid, has a bad signature, or fails
    /// validation (e.g. wrong algorithm, audience, issuer).
    InvalidToken(String),
    /// The token's `exp` claim has passed.
    Expired,
    /// The token's JTI is on the revocation blacklist.
    Revoked,
    /// The manager has no encoding key; cannot sign tokens.
    EncodingKeyMissing,
    /// Signing the claims failed.
    Encode(String),
    /// A configuration or internal error unrelated to the token itself.
    Internal(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken(e) => write!(f, "invalid token: {e}"),
            Self::Expired => f.write_str("token expired"),
            Self::Revoked => f.write_str("token revoked"),
            Self::EncodingKeyMissing => f.write_str("no encoding key configured"),
            Self::Encode(e) => write!(f, "encoding failed: {e}"),
            Self::Internal(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

// ─── HasJti trait ─────────────────────────────────────────────────────────────

/// Implemented by claims types that carry a JWT ID (`jti`) for revocation checks.
///
/// The **default implementation returns `None`**, so any claims type can opt out
/// of revocation with an empty one-line impl:
///
/// ```rust
/// # use chopin_auth::HasJti;
/// struct MyClaims { sub: String, exp: u64 }
/// impl HasJti for MyClaims {}
/// ```
///
/// To enable revocation, return the `jti` field:
///
/// ```rust
/// # use chopin_auth::HasJti;
/// struct MyClaims { sub: String, jti: String, exp: u64 }
/// impl HasJti for MyClaims {
///     fn jti(&self) -> Option<&str> { Some(&self.jti) }
/// }
/// ```
pub trait HasJti {
    /// Return the `jti` claim value, or `None` if the claims do not carry one.
    fn jti(&self) -> Option<&str> {
        None
    }
}

// ─── JwtConfig ───────────────────────────────────────────────────────────────

/// Low-level configuration for a [`JwtManager`].
///
/// Prefer the constructor methods on [`JwtManager`] over constructing this directly.
pub struct JwtConfig {
    pub decoding_key: DecodingKey,
    pub encoding_key: Option<EncodingKey>,
    pub validation: Validation,
}

// ─── JwtManager ──────────────────────────────────────────────────────────────

/// A cloneable JWT manager for encoding and decoding tokens.
///
/// Constructors:
/// - [`JwtManager::new`]                – HMAC-SHA256 (sign + verify)
/// - [`JwtManager::verify_only`]        – HMAC-SHA256 (verify only)
/// - [`JwtManager::from_rsa_pem`]       – RS256 (sign + verify)
/// - [`JwtManager::from_rsa_public_pem`] – RS256 (verify only)
/// - [`JwtManager::from_ec_pem`]        – ES256 (sign + verify)
/// - [`JwtManager::from_ec_public_pem`] – ES256 (verify only)
/// - [`JwtManager::with_config`]        – fully custom config
///
/// Add a revocation blacklist with [`JwtManager::with_blacklist`].
#[derive(Clone)]
pub struct JwtManager {
    config: Arc<JwtConfig>,
    blacklist: Option<TokenBlacklist>,
}

impl JwtManager {
    /// Construct a manager using a shared HMAC-SHA256 secret (sign + verify).
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 60; // 1 minute clock-skew leeway
        Self {
            config: Arc::new(JwtConfig {
                decoding_key: DecodingKey::from_secret(secret),
                encoding_key: Some(EncodingKey::from_secret(secret)),
                validation,
            }),
            blacklist: None,
        }
    }

    /// Construct a verify-only manager (no signing key) for HMAC-SHA256.
    pub fn verify_only(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 60;
        Self {
            config: Arc::new(JwtConfig {
                decoding_key: DecodingKey::from_secret(secret),
                encoding_key: None,
                validation,
            }),
            blacklist: None,
        }
    }

    /// Construct a manager from a PEM-encoded RSA private/public key pair (RS256).
    ///
    /// Both the private key (for signing) and the public key (for verification)
    /// must be provided. For verify-only use, see [`JwtManager::from_rsa_public_pem`].
    pub fn from_rsa_pem(private_key_pem: &[u8], public_key_pem: &[u8]) -> Result<Self, AuthError> {
        let encoding_key = EncodingKey::from_rsa_pem(private_key_pem)
            .map_err(|e| AuthError::Internal(format!("RSA private key: {e}")))?;
        let decoding_key = DecodingKey::from_rsa_pem(public_key_pem)
            .map_err(|e| AuthError::Internal(format!("RSA public key: {e}")))?;
        Ok(Self {
            config: Arc::new(JwtConfig {
                encoding_key: Some(encoding_key),
                decoding_key,
                validation: Validation::new(Algorithm::RS256),
            }),
            blacklist: None,
        })
    }

    /// Construct a **verify-only** manager from a PEM-encoded RSA public key (RS256).
    ///
    /// No signing key is stored; calling [`JwtManager::encode`] will return
    /// [`AuthError::EncodingKeyMissing`]. Ideal for microservices that only
    /// consume tokens, never issue them.
    pub fn from_rsa_public_pem(public_key_pem: &[u8]) -> Result<Self, AuthError> {
        let decoding_key = DecodingKey::from_rsa_pem(public_key_pem)
            .map_err(|e| AuthError::Internal(format!("RSA public key: {e}")))?;
        Ok(Self {
            config: Arc::new(JwtConfig {
                encoding_key: None,
                decoding_key,
                validation: Validation::new(Algorithm::RS256),
            }),
            blacklist: None,
        })
    }

    /// Construct a manager from a PEM-encoded EC private/public key pair (ES256).
    ///
    /// Both keys are required for signing and verification. For verify-only use,
    /// see [`JwtManager::from_ec_public_pem`].
    pub fn from_ec_pem(private_key_pem: &[u8], public_key_pem: &[u8]) -> Result<Self, AuthError> {
        let encoding_key = EncodingKey::from_ec_pem(private_key_pem)
            .map_err(|e| AuthError::Internal(format!("EC private key: {e}")))?;
        let decoding_key = DecodingKey::from_ec_pem(public_key_pem)
            .map_err(|e| AuthError::Internal(format!("EC public key: {e}")))?;
        Ok(Self {
            config: Arc::new(JwtConfig {
                encoding_key: Some(encoding_key),
                decoding_key,
                validation: Validation::new(Algorithm::ES256),
            }),
            blacklist: None,
        })
    }

    /// Construct a **verify-only** manager from a PEM-encoded EC public key (ES256).
    ///
    /// No signing key is stored; calling [`JwtManager::encode`] will return
    /// [`AuthError::EncodingKeyMissing`]. Ideal for microservices that only
    /// consume tokens, never issue them.
    pub fn from_ec_public_pem(public_key_pem: &[u8]) -> Result<Self, AuthError> {
        let decoding_key = DecodingKey::from_ec_pem(public_key_pem)
            .map_err(|e| AuthError::Internal(format!("EC public key: {e}")))?;
        Ok(Self {
            config: Arc::new(JwtConfig {
                encoding_key: None,
                decoding_key,
                validation: Validation::new(Algorithm::ES256),
            }),
            blacklist: None,
        })
    }

    /// Construct a manager from a fully custom [`JwtConfig`].
    pub fn with_config(config: JwtConfig) -> Self {
        Self {
            config: Arc::new(config),
            blacklist: None,
        }
    }

    /// Attach a revocation blacklist, returning the updated manager.
    ///
    /// ```rust,ignore
    /// let bl = TokenBlacklist::new();
    /// let manager = JwtManager::new(b"secret").with_blacklist(bl.clone());
    /// // Later: bl.revoke(jti, Some(exp));
    /// ```
    pub fn with_blacklist(mut self, blacklist: TokenBlacklist) -> Self {
        self.blacklist = Some(blacklist);
        self
    }

    /// Decode and verify a JWT, optionally checking revocation.
    ///
    /// If a [`TokenBlacklist`] is attached and `T::jti()` returns `Some(jti)`,
    /// the JTI is checked for revocation after the signature is verified.
    ///
    /// # Errors
    /// - [`AuthError::Expired`]      – the `exp` claim has passed.
    /// - [`AuthError::Revoked`]      – the JTI is on the blacklist.
    /// - [`AuthError::InvalidToken`] – signature or format error.
    pub fn decode<T>(&self, token: &str) -> Result<T, AuthError>
    where
        T: for<'de> Deserialize<'de> + HasJti,
    {
        let token_data = decode::<T>(token, &self.config.decoding_key, &self.config.validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        if let Some(bl) = &self.blacklist
            && let Some(jti) = token_data.claims.jti()
            && bl.is_revoked(jti)
        {
            return Err(AuthError::Revoked);
        }

        Ok(token_data.claims)
    }

    /// Sign a set of claims, returning a compact JWT string.
    ///
    /// # Errors
    /// - [`AuthError::EncodingKeyMissing`] – no signing key is configured.
    /// - [`AuthError::Encode`]             – claims serialisation failed.
    pub fn encode<T: Serialize>(&self, claims: &T) -> Result<String, AuthError> {
        let key = self
            .config
            .encoding_key
            .as_ref()
            .ok_or(AuthError::EncodingKeyMissing)?;
        encode(&Header::default(), claims, key).map_err(|e| AuthError::Encode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        exp: u64,
    }

    // Opt out of revocation — no `jti` field.
    impl HasJti for TestClaims {}

    fn far_future_exp() -> u64 {
        // 9999-01-01 00:00:00 UTC as a Unix timestamp
        253_370_764_800_u64
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mgr = JwtManager::new(b"test-secret-key");
        let claims = TestClaims {
            sub: "user-42".to_string(),
            exp: far_future_exp(),
        };
        let token = mgr.encode(&claims).expect("encode should succeed");
        assert!(!token.is_empty());
        let decoded: TestClaims = mgr.decode(&token).expect("decode should succeed");
        assert_eq!(decoded, claims);
    }

    #[test]
    fn test_decode_wrong_secret_fails() {
        let mgr_sign = JwtManager::new(b"correct-secret");
        let mgr_verify = JwtManager::new(b"wrong-secret");
        let claims = TestClaims {
            sub: "user-1".to_string(),
            exp: far_future_exp(),
        };
        let token = mgr_sign.encode(&claims).expect("encode must succeed");
        let result: Result<TestClaims, _> = mgr_verify.decode(&token);
        assert!(result.is_err(), "decode with wrong secret should fail");
    }

    #[test]
    fn test_decode_invalid_token_fails() {
        let mgr = JwtManager::new(b"any-secret");
        let result: Result<TestClaims, _> = mgr.decode("not.a.jwt");
        assert!(result.is_err(), "decode of garbage should fail");
    }

    #[test]
    fn test_decode_mangled_token_fails() {
        let mgr = JwtManager::new(b"secret");
        let claims = TestClaims {
            sub: "u".to_string(),
            exp: far_future_exp(),
        };
        let mut token = mgr.encode(&claims).expect("encode ok");
        // flip last byte of signature
        let last = token.pop().unwrap();
        token.push(if last == 'A' { 'B' } else { 'A' });
        let result: Result<TestClaims, _> = mgr.decode(&token);
        assert!(result.is_err(), "mangled token should fail");
    }

    #[test]
    fn test_clone_shares_key() {
        let mgr1 = JwtManager::new(b"shared-key");
        let mgr2 = mgr1.clone();
        let claims = TestClaims {
            sub: "u".to_string(),
            exp: far_future_exp(),
        };
        let token = mgr1.encode(&claims).unwrap();
        // mgr2 must decode tokens signed with mgr1 (shares Arc<JwtConfig>)
        let decoded: TestClaims = mgr2.decode(&token).expect("clone should decode");
        assert_eq!(decoded.sub, "u");
    }

    #[test]
    fn test_encode_without_key_returns_error() {
        let config = JwtConfig {
            decoding_key: DecodingKey::from_secret(b"secret"),
            encoding_key: None,
            validation: Validation::new(Algorithm::HS256),
        };
        let mgr = JwtManager::with_config(config);
        let claims = TestClaims {
            sub: "x".to_string(),
            exp: far_future_exp(),
        };
        let result = mgr.encode(&claims);
        assert!(
            matches!(result, Err(AuthError::EncodingKeyMissing)),
            "expected EncodingKeyMissing, got {result:?}"
        );
    }

    #[test]
    fn test_revoked_token_rejected() {
        use crate::revocation::TokenBlacklist;

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct ClaimsWithJti {
            sub: String,
            jti: String,
            exp: u64,
        }
        impl HasJti for ClaimsWithJti {
            fn jti(&self) -> Option<&str> {
                Some(&self.jti)
            }
        }

        let blacklist = TokenBlacklist::new();
        let mgr = JwtManager::new(b"s").with_blacklist(blacklist.clone());
        let claims = ClaimsWithJti {
            sub: "u".into(),
            jti: "unique-jti-1".into(),
            exp: far_future_exp(),
        };
        let token = mgr.encode(&claims).unwrap();

        // Valid before revocation.
        mgr.decode::<ClaimsWithJti>(&token)
            .expect("should be valid before revocation");

        // Revoke and verify rejection.
        blacklist.revoke("unique-jti-1".into(), None);
        let result = mgr.decode::<ClaimsWithJti>(&token);
        assert!(
            matches!(result, Err(AuthError::Revoked)),
            "revoked token should be rejected, got {result:?}"
        );
    }

    #[test]
    fn test_expired_token_returns_expired_error() {
        let mgr = JwtManager::new(b"secret");
        // exp = 1 — Unix epoch 1970-01-01, long past
        let claims = serde_json::json!({ "sub": "u", "exp": 1_u64 });
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let result: Result<TestClaims, _> = mgr.decode(&token);
        assert!(
            matches!(result, Err(AuthError::Expired)),
            "expected Expired, got {result:?}"
        );
    }
}
