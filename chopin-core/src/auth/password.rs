use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use crate::error::ChopinError;

/// Hash a plaintext password using Argon2.
pub fn hash_password(password: &str) -> Result<String, ChopinError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| ChopinError::Internal(format!("Failed to hash password: {}", e)))
}

/// Verify a plaintext password against a stored hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, ChopinError> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| ChopinError::Internal(format!("Invalid password hash: {}", e)))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
