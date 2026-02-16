use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ChopinError;

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: usize,
    /// Issued at (Unix timestamp)
    pub iat: usize,
}

/// Create a JWT token for a given user ID (string or numeric).
pub fn create_token(user_id: &str, secret: &str, expiry_hours: u64) -> Result<String, ChopinError> {
    let now = Utc::now();
    let expires = now + Duration::hours(expiry_hours as i64);

    let claims = Claims {
        sub: user_id.to_string(),
        exp: expires.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ChopinError::Internal(format!("Failed to create token: {}", e)))
}

/// Validate a JWT token and return the claims.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, ChopinError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| ChopinError::Unauthorized(format!("Invalid token: {}", e)))?;

    Ok(token_data.claims)
}
