// src/jwks.rs — JSON Web Key Set (JWKS) support (RFC 7517)
//
// Parses JWKS JSON into a set of indexed decoding keys, enabling key lookup
// by `kid` (Key ID). Works with RSA (RS256/RS384/RS512) and EC (ES256/ES384)
// key types commonly served by identity providers.

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::jwt::{AuthError, JwtConfig, JwtManager};

// ── JWK types ────────────────────────────────────────────────────────────────

/// A single JSON Web Key.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key type: "RSA" or "EC".
    pub kty: String,
    /// Key ID — used to match tokens to the signing key.
    #[serde(default)]
    pub kid: Option<String>,
    /// Algorithm hint (e.g. "RS256", "ES256").
    #[serde(default)]
    pub alg: Option<String>,
    /// Key use: "sig" (signature) or "enc" (encryption).
    #[serde(rename = "use", default)]
    pub key_use: Option<String>,

    // RSA parameters (base64url-encoded)
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,

    // EC parameters (base64url-encoded)
    #[serde(default)]
    pub crv: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
}

/// A JSON Web Key Set — the top-level object returned by JWKS endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct JwkSet {
    pub keys: Vec<Jwk>,
}

// ── Key parsing ──────────────────────────────────────────────────────────────

/// A parsed key ready for verification.
struct ParsedKey {
    decoding_key: DecodingKey,
    algorithm: Algorithm,
}

fn algorithm_from_str(alg: &str) -> Option<Algorithm> {
    match alg {
        "RS256" => Some(Algorithm::RS256),
        "RS384" => Some(Algorithm::RS384),
        "RS512" => Some(Algorithm::RS512),
        "ES256" => Some(Algorithm::ES256),
        "ES384" => Some(Algorithm::ES384),
        "PS256" => Some(Algorithm::PS256),
        "PS384" => Some(Algorithm::PS384),
        "PS512" => Some(Algorithm::PS512),
        _ => None,
    }
}

fn parse_jwk(jwk: &Jwk) -> Result<ParsedKey, AuthError> {
    #[allow(clippy::unnecessary_lazy_evaluations)]
    let algorithm = jwk
        .alg
        .as_deref()
        .and_then(algorithm_from_str)
        .or_else(|| {
            // Infer algorithm from key type if not specified
            match jwk.kty.as_str() {
                "RSA" => Some(Algorithm::RS256),
                "EC" => match jwk.crv.as_deref() {
                    Some("P-384") => Some(Algorithm::ES384),
                    _ => Some(Algorithm::ES256), // Default to P-256
                },
                _ => None,
            }
        })
        .ok_or_else(|| AuthError::Internal("unsupported key type/algorithm".into()))?;

    let decoding_key = match jwk.kty.as_str() {
        "RSA" => {
            let n = jwk
                .n
                .as_deref()
                .ok_or_else(|| AuthError::Internal("RSA JWK missing 'n' parameter".into()))?;
            let e = jwk
                .e
                .as_deref()
                .ok_or_else(|| AuthError::Internal("RSA JWK missing 'e' parameter".into()))?;
            DecodingKey::from_rsa_components(n, e)
                .map_err(|err| AuthError::Internal(format!("RSA JWK parse error: {err}")))?
        }
        "EC" => {
            let x = jwk
                .x
                .as_deref()
                .ok_or_else(|| AuthError::Internal("EC JWK missing 'x' parameter".into()))?;
            let y = jwk
                .y
                .as_deref()
                .ok_or_else(|| AuthError::Internal("EC JWK missing 'y' parameter".into()))?;
            DecodingKey::from_ec_components(x, y)
                .map_err(|err| AuthError::Internal(format!("EC JWK parse error: {err}")))?
        }
        other => {
            return Err(AuthError::Internal(format!(
                "unsupported JWK key type: {other}"
            )));
        }
    };

    Ok(ParsedKey {
        decoding_key,
        algorithm,
    })
}

// ── JwksProvider ─────────────────────────────────────────────────────────────

/// A thread-safe JWKS-based key provider.
///
/// Holds a cached set of keys indexed by `kid`, and provides `JwtManager`
/// instances for any key in the set.
///
/// # Usage
///
/// ```rust,ignore
/// use chopin_auth::jwks::JwksProvider;
///
/// // Fetch JWKS JSON from your IdP (using your preferred HTTP client):
/// let json = fetch("https://idp.example.com/.well-known/jwks.json");
///
/// // Build the provider:
/// let provider = JwksProvider::from_json(&json)?;
///
/// // Decode a token — the `kid` from the JWT header selects the key:
/// let claims: MyClaims = provider.decode(token)?;
///
/// // Periodically refresh keys:
/// let new_json = fetch("https://idp.example.com/.well-known/jwks.json");
/// provider.refresh(&new_json)?;
/// ```
#[derive(Clone)]
pub struct JwksProvider {
    inner: Arc<RwLock<JwksInner>>,
}

struct JwksInner {
    /// kid → (DecodingKey, Algorithm)
    keys: HashMap<String, (DecodingKey, Algorithm)>,
    /// Fallback key when token has no `kid` (first sig key in the set).
    default_key: Option<(DecodingKey, Algorithm)>,
}

impl JwksProvider {
    /// Parse a JWKS JSON string and build the provider.
    ///
    /// Only keys with `"use": "sig"` (or no `use` field) are imported.
    /// Keys without a `kid` are used as the default fallback.
    pub fn from_json(jwks_json: &str) -> Result<Self, AuthError> {
        let jwk_set: JwkSet = serde_json::from_str(jwks_json)
            .map_err(|e| AuthError::Internal(format!("JWKS parse error: {e}")))?;

        let inner = Self::build_inner(&jwk_set)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Replace the cached keys with a freshly-fetched JWKS set.
    ///
    /// This is an atomic swap — concurrent decode calls will see either the
    /// old set or the new set, never a mix.
    pub fn refresh(&self, jwks_json: &str) -> Result<(), AuthError> {
        let jwk_set: JwkSet = serde_json::from_str(jwks_json)
            .map_err(|e| AuthError::Internal(format!("JWKS parse error: {e}")))?;

        let new_inner = Self::build_inner(&jwk_set)?;
        let mut guard = self
            .inner
            .write()
            .map_err(|_| AuthError::Internal("JWKS lock poisoned".into()))?;
        *guard = new_inner;
        Ok(())
    }

    /// Create a `JwtManager` for a specific `kid`.
    ///
    /// Returns `None` if the kid is not in the current key set.
    pub fn manager_for_kid(&self, kid: &str) -> Option<JwtManager> {
        let guard = self.inner.read().ok()?;
        let (dk, alg) = guard.keys.get(kid)?;
        let mut validation = Validation::new(*alg);
        validation.validate_exp = true;
        validation.leeway = 60;
        Some(JwtManager::with_config(JwtConfig {
            decoding_key: dk.clone(),
            encoding_key: None,
            validation,
        }))
    }

    /// Decode a JWT by first extracting the `kid` from the token header,
    /// then using the corresponding key.
    ///
    /// Falls back to the default key if the token has no `kid` header.
    pub fn decode<T>(&self, token: &str) -> Result<T, AuthError>
    where
        T: for<'de> serde::de::Deserialize<'de> + crate::jwt::HasJti,
    {
        // Extract kid from the JWT header (first segment, base64url-decoded)
        let kid = extract_kid_from_header(token);

        let guard = self
            .inner
            .read()
            .map_err(|_| AuthError::Internal("JWKS lock poisoned".into()))?;

        let (dk, alg) = if let Some(kid) = &kid {
            guard
                .keys
                .get(kid.as_str())
                .ok_or_else(|| AuthError::InvalidToken(format!("unknown kid: {kid}")))?
        } else {
            guard.default_key.as_ref().ok_or(AuthError::InvalidToken(
                "no kid in token and no default key".into(),
            ))?
        };

        let mut validation = Validation::new(*alg);
        validation.validate_exp = true;
        validation.leeway = 60;

        let token_data =
            jsonwebtoken::decode::<T>(token, dk, &validation).map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        Ok(token_data.claims)
    }

    /// Return the number of keys currently loaded.
    pub fn key_count(&self) -> usize {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard.keys.len() + if guard.default_key.is_some() { 1 } else { 0 }
    }

    fn build_inner(jwk_set: &JwkSet) -> Result<JwksInner, AuthError> {
        let mut keys = HashMap::new();
        let mut default_key = None;

        for jwk in &jwk_set.keys {
            // Skip encryption keys
            if jwk.key_use.as_deref() == Some("enc") {
                continue;
            }

            match parse_jwk(jwk) {
                Ok(parsed) => {
                    if let Some(kid) = &jwk.kid {
                        keys.insert(kid.clone(), (parsed.decoding_key, parsed.algorithm));
                    } else if default_key.is_none() {
                        default_key = Some((parsed.decoding_key, parsed.algorithm));
                    }
                }
                Err(_) => {
                    // Skip keys we can't parse (e.g. unsupported algorithm)
                    continue;
                }
            }
        }

        Ok(JwksInner { keys, default_key })
    }
}

// ── Helper: extract kid from JWT header ──────────────────────────────────────

fn extract_kid_from_header(token: &str) -> Option<String> {
    let header_b64 = token.split('.').next()?;
    let header_json = base64url_decode(header_b64)?;
    let header: serde_json::Value = serde_json::from_slice(&header_json).ok()?;
    header.get("kid")?.as_str().map(|s| s.to_string())
}

/// Decode base64url (no padding) to bytes.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: [u8; 128] = {
        let mut t = [255u8; 128];
        let mut i = 0u8;
        // A-Z
        while i < 26 {
            t[(b'A' + i) as usize] = i;
            i += 1;
        }
        // a-z
        i = 0;
        while i < 26 {
            t[(b'a' + i) as usize] = 26 + i;
            i += 1;
        }
        // 0-9
        i = 0;
        while i < 10 {
            t[(b'0' + i) as usize] = 52 + i;
            i += 1;
        }
        t[b'+' as usize] = 62;
        t[b'-' as usize] = 62; // base64url
        t[b'/' as usize] = 63;
        t[b'_' as usize] = 63; // base64url
        t
    };

    // Strip padding
    let input = input.trim_end_matches('=');
    let len = input.len();
    let mut out = Vec::with_capacity(len * 3 / 4);

    let mut i = 0;
    while i + 3 < len {
        let (a, b, c, d) = (
            TABLE[input.as_bytes()[i] as usize],
            TABLE[input.as_bytes()[i + 1] as usize],
            TABLE[input.as_bytes()[i + 2] as usize],
            TABLE[input.as_bytes()[i + 3] as usize],
        );
        if a == 255 || b == 255 || c == 255 || d == 255 {
            return None;
        }
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
        out.push((c << 6) | d);
        i += 4;
    }

    let remaining = len - i;
    if remaining >= 2 {
        let a = TABLE[input.as_bytes()[i] as usize];
        let b = TABLE[input.as_bytes()[i + 1] as usize];
        if a == 255 || b == 255 {
            return None;
        }
        out.push((a << 2) | (b >> 4));
        if remaining >= 3 {
            let c = TABLE[input.as_bytes()[i + 2] as usize];
            if c == 255 {
                return None;
            }
            out.push((b << 4) | (c >> 2));
        }
    }

    Some(out)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JWKS: &str = r#"{
        "keys": [
            {
                "kty": "RSA",
                "kid": "key-1",
                "use": "sig",
                "alg": "RS256",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            },
            {
                "kty": "RSA",
                "kid": "key-2",
                "use": "sig",
                "alg": "RS256",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            },
            {
                "kty": "RSA",
                "kid": "enc-key",
                "use": "enc",
                "alg": "RS256",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            }
        ]
    }"#;

    #[test]
    fn test_parse_jwks_json() {
        let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
        // 2 sig keys with kid, enc key skipped
        assert_eq!(provider.key_count(), 2);
    }

    #[test]
    fn test_manager_for_kid() {
        let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
        assert!(provider.manager_for_kid("key-1").is_some());
        assert!(provider.manager_for_kid("key-2").is_some());
        assert!(provider.manager_for_kid("nonexistent").is_none());
        // enc key should be skipped
        assert!(provider.manager_for_kid("enc-key").is_none());
    }

    #[test]
    fn test_refresh_replaces_keys() {
        let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
        assert_eq!(provider.key_count(), 2);

        // Refresh with a single-key JWKS
        let single = r#"{"keys": [{"kty": "RSA", "kid": "new-key", "use": "sig", "alg": "RS256", "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw", "e": "AQAB"}]}"#;
        provider.refresh(single).unwrap();
        assert_eq!(provider.key_count(), 1);
        assert!(provider.manager_for_kid("new-key").is_some());
        assert!(provider.manager_for_kid("key-1").is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(JwksProvider::from_json("not json").is_err());
    }

    #[test]
    fn test_empty_keyset() {
        let provider = JwksProvider::from_json(r#"{"keys": []}"#).unwrap();
        assert_eq!(provider.key_count(), 0);
    }

    #[test]
    fn test_default_key_no_kid() {
        // Key without kid becomes default
        let jwks = r#"{"keys": [{"kty": "RSA", "use": "sig", "alg": "RS256", "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw", "e": "AQAB"}]}"#;
        let provider = JwksProvider::from_json(jwks).unwrap();
        assert_eq!(provider.key_count(), 1); // default key
    }

    #[test]
    fn test_base64url_decode() {
        // Standard base64url decoding
        assert_eq!(base64url_decode("SGVsbG8").unwrap(), b"Hello");
        assert_eq!(base64url_decode("SGVsbG8gV29ybGQ").unwrap(), b"Hello World");
    }

    #[test]
    fn test_extract_kid_from_header() {
        // A JWT header with kid: {"alg":"RS256","kid":"my-key","typ":"JWT"}
        // base64url: eyJhbGciOiJSUzI1NiIsImtpZCI6Im15LWtleSIsInR5cCI6IkpXVCJ9
        let token = "eyJhbGciOiJSUzI1NiIsImtpZCI6Im15LWtleSIsInR5cCI6IkpXVCJ9.e30.sig";
        let kid = extract_kid_from_header(token);
        assert_eq!(kid.as_deref(), Some("my-key"));
    }

    #[test]
    fn test_extract_kid_no_kid() {
        // A JWT header without kid: {"alg":"HS256","typ":"JWT"}
        // base64url: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.e30.sig";
        let kid = extract_kid_from_header(token);
        assert!(kid.is_none());
    }

    #[test]
    fn test_algorithm_from_str() {
        assert_eq!(algorithm_from_str("RS256"), Some(Algorithm::RS256));
        assert_eq!(algorithm_from_str("ES256"), Some(Algorithm::ES256));
        assert_eq!(algorithm_from_str("PS512"), Some(Algorithm::PS512));
        assert_eq!(algorithm_from_str("HS256"), None); // HMAC not supported via JWK
        assert_eq!(algorithm_from_str("unknown"), None);
    }
}
