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
