// src/crypto.rs
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chopin_core::error::{ChopinError, ChopinResult};

pub fn hash_password(password: &[u8]) -> ChopinResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password, &salt)
        .map_err(|e| ChopinError::Other(format!("Failed to hash password: {}", e)))?
        .to_string();

    Ok(password_hash)
}

pub fn verify_password(password: &[u8], hash: &str) -> ChopinResult<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| ChopinError::Other(format!("Invalid password hash format: {}", e)))?;
    let is_valid = Argon2::default()
        .verify_password(password, &parsed_hash)
        .is_ok();
    Ok(is_valid)
}
