use rand::Rng;
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::error::ChopinError;

/// Generate a new TOTP secret for a user.
///
/// Returns `(secret_base32, otpauth_uri)` — store `secret_base32` in the DB,
/// display the URI (or QR code) to the user.
pub fn generate_totp_secret(
    issuer: &str,
    account_name: &str,
) -> Result<(String, String), ChopinError> {
    let secret = Secret::generate_secret();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(|e| {
            ChopinError::Internal(format!("Failed to generate TOTP secret bytes: {}", e))
        })?,
        Some(issuer.to_string()),
        account_name.to_string(),
    )
    .map_err(|e| ChopinError::Internal(format!("Failed to create TOTP: {}", e)))?;

    let secret_b32 = secret.to_encoded().to_string();
    let uri = totp.get_url();

    Ok((secret_b32, uri))
}

/// Verify a TOTP code against a stored secret.
pub fn verify_totp(secret_base32: &str, code: &str) -> Result<bool, ChopinError> {
    let secret = Secret::Encoded(secret_base32.to_string());
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret
            .to_bytes()
            .map_err(|e| ChopinError::Internal(format!("Failed to decode TOTP secret: {}", e)))?,
        None,
        String::new(),
    )
    .map_err(|e| ChopinError::Internal(format!("Failed to create TOTP: {}", e)))?;

    let valid = totp
        .check_current(code)
        .map_err(|e| ChopinError::Internal(format!("TOTP system time error: {}", e)))?;
    Ok(valid)
}

/// Generate a cryptographically secure random token (hex-encoded).
pub fn generate_secure_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

/// SHA-256 hash a token for safe database storage.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
