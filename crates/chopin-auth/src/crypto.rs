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

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: Argon2 is intentionally slow (~100–300 ms per call).
    // These tests exercise correctness, not speed.

    #[test]
    fn test_hash_password_returns_ok() {
        let result = hash_password(b"mypassword");
        assert!(result.is_ok(), "hash_password should return Ok");
        let hash = result.unwrap();
        // Argon2id PHC string format starts with $argon2
        assert!(hash.starts_with("$argon2"), "unexpected hash format: {}", hash);
    }

    #[test]
    fn test_verify_correct_password_returns_true() {
        let hash = hash_password(b"correct-horse").unwrap();
        let ok = verify_password(b"correct-horse", &hash).unwrap();
        assert!(ok, "correct password should verify true");
    }

    #[test]
    fn test_verify_wrong_password_returns_false() {
        let hash = hash_password(b"correct-horse").unwrap();
        let ok = verify_password(b"wrong-battery", &hash).unwrap();
        assert!(!ok, "wrong password should verify false");
    }

    #[test]
    fn test_invalid_hash_format_returns_err() {
        let result = verify_password(b"password", "not-a-valid-hash");
        assert!(result.is_err(), "invalid hash format should return Err");
    }

    #[test]
    fn test_hash_is_unique_per_call() {
        // Each call uses a fresh random salt
        let h1 = hash_password(b"same-pass").unwrap();
        let h2 = hash_password(b"same-pass").unwrap();
        assert_ne!(h1, h2, "two hashes of the same password must differ (random salt)");
    }

    #[test]
    fn test_empty_password_hashes_and_verifies() {
        let hash = hash_password(b"").unwrap();
        assert!(verify_password(b"", &hash).unwrap());
        assert!(!verify_password(b"notempty", &hash).unwrap());
    }
}
