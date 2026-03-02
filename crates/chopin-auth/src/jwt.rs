// src/jwt.rs
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct JwtConfig {
    pub decoding_key: DecodingKey,
    pub encoding_key: Option<EncodingKey>,
    pub validation: Validation,
}

#[derive(Clone)]
pub struct JwtManager {
    config: Arc<JwtConfig>,
}

impl JwtManager {
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 60; // 1 minute leeway for clock skew

        Self {
            config: Arc::new(JwtConfig {
                decoding_key: DecodingKey::from_secret(secret),
                encoding_key: Some(EncodingKey::from_secret(secret)),
                validation,
            }),
        }
    }

    pub fn with_config(config: JwtConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub fn decode<T: for<'de> Deserialize<'de>>(
        &self,
        token: &str,
    ) -> chopin_core::error::ChopinResult<T> {
        let token_data = decode::<T>(token, &self.config.decoding_key, &self.config.validation)
            .map_err(|e| chopin_core::error::ChopinError::Other(e.to_string()))?;
        Ok(token_data.claims)
    }

    pub fn encode<T: Serialize>(&self, claims: &T) -> chopin_core::error::ChopinResult<String> {
        let encoding_key = self
            .config
            .encoding_key
            .as_ref()
            .expect("Encoding key must be set to sign tokens");
        encode(&Header::default(), claims, encoding_key)
            .map_err(|e| chopin_core::error::ChopinError::Other(e.to_string()))
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

    fn far_future_exp() -> u64 {
        // 9999-01-01 00:00:00 UTC as a Unix timestamp
        253_370_764_800_u64
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mgr = JwtManager::new(b"test-secret-key");
        let claims = TestClaims { sub: "user-42".to_string(), exp: far_future_exp() };
        let token = mgr.encode(&claims).expect("encode should succeed");
        assert!(!token.is_empty());
        let decoded: TestClaims = mgr.decode(&token).expect("decode should succeed");
        assert_eq!(decoded, claims);
    }

    #[test]
    fn test_decode_wrong_secret_fails() {
        let mgr_sign   = JwtManager::new(b"correct-secret");
        let mgr_verify = JwtManager::new(b"wrong-secret");
        let claims = TestClaims { sub: "user-1".to_string(), exp: far_future_exp() };
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
        let claims = TestClaims { sub: "u".to_string(), exp: far_future_exp() };
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
        let claims = TestClaims { sub: "u".to_string(), exp: far_future_exp() };
        let token = mgr1.encode(&claims).unwrap();
        // mgr2 must decode tokens signed with mgr1 (shares Arc<JwtConfig>)
        let decoded: TestClaims = mgr2.decode(&token).expect("clone should decode");
        assert_eq!(decoded.sub, "u");
    }
}
