//! Cross-module integration tests for `chopin-auth`.
//!
//! Per-module unit tests live alongside the implementation (e.g. `jwt.rs`,
//! `crypto.rs`). This file exercises the public surface together to catch
//! cross-cutting regressions (PKCE round-trip, password hashing presets,
//! revocation, AuthError classification, JWKS parsing).

use chopin_auth::{
    AuthError, HasJti, JwksProvider, JwtManager, PasswordHasher, TokenBlacklist,
    code_challenge_s256, code_verifier, hash_password, verify_password,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct TestClaims {
    sub: String,
    jti: String,
    exp: u64,
}

impl HasJti for TestClaims {
    fn jti(&self) -> Option<&str> {
        Some(&self.jti)
    }
}

// ─── PKCE ────────────────────────────────────────────────────────────────────

#[test]
fn pkce_verifier_meets_rfc7636_length_bounds() {
    let v = code_verifier();
    // RFC 7636 §4.1: 43–128 base64url characters
    assert!(
        (43..=128).contains(&v.len()),
        "verifier length {} out of range",
        v.len()
    );
    // base64url alphabet only
    assert!(
        v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "verifier contains non-base64url characters: {v}"
    );
}

#[test]
fn pkce_verifier_is_unique_across_calls() {
    let a = code_verifier();
    let b = code_verifier();
    assert_ne!(a, b, "two verifiers in a row collided — RNG bug?");
}

#[test]
fn pkce_s256_challenge_is_deterministic_for_same_verifier() {
    let v = code_verifier();
    assert_eq!(code_challenge_s256(&v), code_challenge_s256(&v));
}

#[test]
fn pkce_s256_challenge_changes_with_verifier() {
    let v1 = code_verifier();
    let v2 = code_verifier();
    assert_ne!(code_challenge_s256(&v1), code_challenge_s256(&v2));
}

// ─── Password hashing ────────────────────────────────────────────────────────

#[test]
fn password_interactive_roundtrip() {
    let pw = b"correct horse battery staple";
    let hash = hash_password(pw).expect("hash should succeed");
    assert!(verify_password(pw, &hash).expect("verify should succeed"));
}

#[test]
fn password_wrong_password_rejected() {
    let hash = hash_password(b"correct-pw").unwrap();
    assert!(!verify_password(b"wrong-pw", &hash).unwrap());
}

#[test]
fn password_hashes_are_salted() {
    // Two hashes of the same password must differ (random salt).
    let h1 = hash_password(b"same").unwrap();
    let h2 = hash_password(b"same").unwrap();
    assert_ne!(h1, h2, "hashes should differ due to random salt");
}

#[test]
fn password_invalid_hash_returns_err() {
    assert!(verify_password(b"x", "not-a-phc-string").is_err());
}

#[test]
fn password_custom_params_rejected_when_invalid() {
    // memory_kib below the argon2 minimum (8 KiB) must error.
    assert!(PasswordHasher::custom(0, 1, 1).is_err());
}

// ─── JWT + revocation ────────────────────────────────────────────────────────

#[test]
fn jwt_revoked_token_returns_revoked_variant() {
    let bl = TokenBlacklist::new();
    let mgr = JwtManager::new(b"secret").with_blacklist(bl.clone());

    let claims = TestClaims {
        sub: "u1".into(),
        jti: "jti-xyz".into(),
        exp: now_secs() + 3600,
    };
    let token = mgr.encode(&claims).unwrap();

    // First decode succeeds.
    let decoded: TestClaims = mgr.decode(&token).unwrap();
    assert_eq!(decoded.sub, "u1");

    // Revoke and try again.
    bl.revoke("jti-xyz".into(), Some(claims.exp));
    let err = mgr.decode::<TestClaims>(&token).unwrap_err();
    assert!(matches!(err, AuthError::Revoked), "got {err:?}");
}

#[test]
fn jwt_expired_token_returns_expired_variant() {
    let mgr = JwtManager::new(b"secret");
    let claims = TestClaims {
        sub: "u".into(),
        jti: "j".into(),
        exp: 1, // 1970 — long expired
    };
    let token = mgr.encode(&claims).unwrap();
    let err = mgr.decode::<TestClaims>(&token).unwrap_err();
    assert!(matches!(err, AuthError::Expired), "got {err:?}");
    assert_eq!(err.http_status(), 401);
}

#[test]
fn jwt_bad_signature_returns_invalid_signature_variant() {
    let signer = JwtManager::new(b"signing-key");
    let verifier = JwtManager::new(b"different-key");

    let claims = TestClaims {
        sub: "u".into(),
        jti: "j".into(),
        exp: now_secs() + 3600,
    };
    let token = signer.encode(&claims).unwrap();
    let err = verifier.decode::<TestClaims>(&token).unwrap_err();
    assert!(
        matches!(err, AuthError::InvalidSignature),
        "expected InvalidSignature, got {err:?}"
    );
}

#[test]
fn jwt_malformed_token_returns_malformed_variant() {
    let mgr = JwtManager::new(b"k");
    // Garbage — not even valid base64/json.
    let err = mgr.decode::<TestClaims>("not.a.jwt").unwrap_err();
    // Could be Malformed OR InvalidToken depending on jsonwebtoken's classification.
    assert!(
        matches!(err, AuthError::Malformed(_) | AuthError::InvalidToken(_)),
        "got {err:?}"
    );
}

#[test]
fn auth_error_http_status_classification() {
    assert_eq!(AuthError::Expired.http_status(), 401);
    assert_eq!(AuthError::Revoked.http_status(), 401);
    assert_eq!(AuthError::InvalidSignature.http_status(), 401);
    assert_eq!(AuthError::MissingKid("x".into()).http_status(), 401);
    assert_eq!(AuthError::EncodingKeyMissing.http_status(), 500);
    assert_eq!(AuthError::Internal("x".into()).http_status(), 500);
}

// ─── Revocation lifecycle ────────────────────────────────────────────────────

#[test]
fn revocation_expired_entry_no_longer_blocks() {
    let bl = TokenBlacklist::new();
    // Revoke until 1 second ago (already past).
    bl.revoke("old-jti".into(), Some(now_secs() - 1));
    assert!(
        !bl.is_revoked("old-jti"),
        "expired revocation entry should not block"
    );
}

#[test]
fn revocation_indefinite_persists() {
    let bl = TokenBlacklist::new();
    bl.revoke("forever".into(), None);
    assert!(bl.is_revoked("forever"));
    bl.cleanup();
    assert!(bl.is_revoked("forever"), "indefinite revocation survived cleanup");
}

#[test]
fn revocation_cleanup_removes_expired_only() {
    let bl = TokenBlacklist::new();
    bl.revoke("expired".into(), Some(now_secs() - 100));
    bl.revoke("future".into(), Some(now_secs() + 3600));
    bl.revoke("forever".into(), None);
    bl.cleanup();
    assert!(!bl.is_revoked("expired"));
    assert!(bl.is_revoked("future"));
    assert!(bl.is_revoked("forever"));
}

// ─── JWKS ────────────────────────────────────────────────────────────────────

const SAMPLE_JWKS: &str = r#"{
    "keys": [
        {
            "kty": "RSA",
            "kid": "key-1",
            "use": "sig",
            "alg": "RS256",
            "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
            "e": "AQAB"
        }
    ]
}"#;

#[test]
fn jwks_parses_sample_set() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).expect("JWKS should parse");
    assert_eq!(provider.key_count(), 1);
}

#[test]
fn jwks_refresh_resets_age() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let before = provider.age();
    assert!(before >= Duration::from_millis(20));

    provider.refresh(SAMPLE_JWKS).unwrap();
    let after = provider.age();
    assert!(after < before, "refresh should reset age (before={before:?}, after={after:?})");
}

#[test]
fn jwks_is_stale_respects_ttl() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
    // Just-built provider is not stale for a generous TTL.
    assert!(!provider.is_stale(Duration::from_secs(60)));
    // Zero-TTL means "always stale".
    assert!(provider.is_stale(Duration::from_secs(0)));
}

#[test]
fn jwks_refresh_if_stale_skips_fetch_when_fresh() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
    let refreshed = provider
        .refresh_if_stale(Duration::from_secs(3600), || {
            panic!("fetch closure must not run when key set is fresh")
        })
        .unwrap();
    assert!(!refreshed);
}

#[test]
fn jwks_refresh_if_stale_invokes_fetch_when_stale() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
    let refreshed = provider
        .refresh_if_stale(Duration::from_secs(0), || Ok(SAMPLE_JWKS.to_string()))
        .unwrap();
    assert!(refreshed);
}

#[test]
fn jwks_unknown_kid_returns_missing_kid_variant() {
    let provider = JwksProvider::from_json(SAMPLE_JWKS).unwrap();
    // Build a token with kid=unknown signed by an unrelated HS256 key —
    // header parsing happens first, so we should hit MissingKid before
    // signature verification ever runs.
    let header = r#"{"alg":"RS256","kid":"does-not-exist","typ":"JWT"}"#;
    let payload = r#"{"sub":"x","jti":"j","exp":9999999999}"#;
    let token = format!(
        "{}.{}.AAAA",
        base64url_no_pad(header.as_bytes()),
        base64url_no_pad(payload.as_bytes())
    );
    let err = provider.decode::<TestClaims>(&token).unwrap_err();
    assert!(matches!(err, AuthError::MissingKid(_)), "got {err:?}");
}

fn base64url_no_pad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 2 < bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
    }
    out
}
