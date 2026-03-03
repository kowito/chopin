// src/crypto.rs
use argon2::{
    Argon2, Params,
    password_hash::{
        PasswordHash, PasswordHasher as Argon2PasswordHasher, PasswordVerifier, SaltString,
        rand_core::OsRng,
    },
};
use chopin_core::error::{ChopinError, ChopinResult};

// ─── PasswordHasher ──────────────────────────────────────────────────────────

/// A configurable Argon2id password hasher.
///
/// Use the preset constructors for common configurations or [`PasswordHasher::custom`]
/// for full control:
///
/// | Preset | Memory | Iterations | Threads | Use case |
/// |---|---|---|---|---|
/// | [`interactive`](PasswordHasher::interactive) | 19 MiB | 2 | 1 | Default / login |
/// | [`sensitive`](PasswordHasher::sensitive) | 64 MiB | 4 | 2 | High-value secrets |
///
/// # Example
/// ```rust,ignore
/// use chopin_auth::PasswordHasher;
/// let hasher = PasswordHasher::interactive();
/// let hash = hasher.hash(b"my-password")?;
/// assert!(hasher.verify(b"my-password", &hash)?);
/// ```
#[derive(Clone)]
pub struct PasswordHasher {
    params: Params,
}

impl PasswordHasher {
    /// Interactive preset (Argon2id defaults: 19 MiB, 2 iterations, 1 thread).
    /// Balanced between speed and security; suitable for most login flows.
    pub fn interactive() -> Self {
        Self {
            params: Params::default(),
        }
    }

    /// Sensitive preset (64 MiB, 4 iterations, 2 threads).
    /// Slower and more memory-intensive; suitable for high-value secrets.
    pub fn sensitive() -> Self {
        Self::custom(65_536, 4, 2).expect("sensitive params are valid")
    }

    /// Custom Argon2id parameters.
    ///
    /// - `memory_kib`: memory cost in kibibytes (minimum 8).
    /// - `iterations`: time cost (minimum 1).
    /// - `parallelism`: degree of parallelism (minimum 1).
    pub fn custom(memory_kib: u32, iterations: u32, parallelism: u32) -> ChopinResult<Self> {
        let params = Params::new(memory_kib, iterations, parallelism, None)
            .map_err(|e| ChopinError::Other(format!("invalid Argon2 params: {e}")))?;
        Ok(Self { params })
    }

    /// Hash a password using Argon2id. Returns a PHC-format string.
    pub fn hash(&self, password: &[u8]) -> ChopinResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            self.params.clone(),
        );
        argon2
            .hash_password(password, &salt)
            .map(|h| h.to_string())
            .map_err(|e| ChopinError::Other(format!("failed to hash password: {e}")))
    }

    /// Verify a password against a PHC-format hash string.
    ///
    /// Returns `Ok(true)` on match, `Ok(false)` on mismatch, `Err` on invalid hash.
    pub fn verify(&self, password: &[u8], hash: &str) -> ChopinResult<bool> {
        let parsed = PasswordHash::new(hash)
            .map_err(|e| ChopinError::Other(format!("invalid hash format: {e}")))?;
        Ok(Argon2::default().verify_password(password, &parsed).is_ok())
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::interactive()
    }
}

// ─── Convenience free functions ──────────────────────────────────────────────

/// Hash a password using the interactive Argon2id preset.
///
/// This is a convenience wrapper around [`PasswordHasher::interactive`].
/// For configurable parameters, use [`PasswordHasher`] directly.
pub fn hash_password(password: &[u8]) -> ChopinResult<String> {
    PasswordHasher::interactive().hash(password)
}

/// Verify a password against a PHC-format hash string.
///
/// This is a convenience wrapper around [`PasswordHasher::interactive`].
pub fn verify_password(password: &[u8], hash: &str) -> ChopinResult<bool> {
    PasswordHasher::interactive().verify(password, hash)
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
        assert!(
            hash.starts_with("$argon2"),
            "unexpected hash format: {hash}"
        );
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
        let h1 = hash_password(b"same-pass").unwrap();
        let h2 = hash_password(b"same-pass").unwrap();
        assert_ne!(
            h1, h2,
            "two hashes of the same password must differ (random salt)"
        );
    }

    #[test]
    fn test_empty_password_hashes_and_verifies() {
        let hash = hash_password(b"").unwrap();
        assert!(verify_password(b"", &hash).unwrap());
        assert!(!verify_password(b"notempty", &hash).unwrap());
    }

    #[test]
    fn test_password_hasher_struct() {
        let hasher = PasswordHasher::interactive();
        let hash = hasher.hash(b"structpass").unwrap();
        assert!(hasher.verify(b"structpass", &hash).unwrap());
        assert!(!hasher.verify(b"wrong", &hash).unwrap());
    }

    #[test]
    fn test_sensitive_preset_produces_valid_hash() {
        let hasher = PasswordHasher::sensitive();
        let hash = hasher.hash(b"sensitive").unwrap();
        assert!(hasher.verify(b"sensitive", &hash).unwrap());
    }

    #[test]
    fn test_custom_params() {
        // Use very low params so the test is fast.
        let hasher = PasswordHasher::custom(8, 1, 1).unwrap();
        let hash = hasher.hash(b"custom").unwrap();
        assert!(hasher.verify(b"custom", &hash).unwrap());
    }
}
