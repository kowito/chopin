//! Unified JSON abstraction for Chopin.
//!
//! When the `perf` feature is enabled, all serialization and deserialization
//! routes through **sonic-rs** — a SIMD-accelerated JSON library that is
//! measurably faster than serde_json for both parsing and writing.
//!
//! Without `perf`, standard serde_json is used so the framework compiles
//! everywhere without architecture-specific requirements.

#[cfg(feature = "perf")]
use sonic_rs as engine;

#[cfg(not(feature = "perf"))]
use serde_json as engine;

use serde::{de::DeserializeOwned, Serialize};

/// Re-export the underlying `Error` type so callers can pattern-match.
pub use engine::Error;

// ── Serialization ──────────────────────────────────────────────

/// Serialize `value` directly into a `Vec<u8>` buffer.
///
/// Both sonic-rs and serde_json implement `to_writer` for `&mut Vec<u8>`,
/// so we use a concrete type instead of a generic `Write` bound.
#[inline]
pub fn to_writer(buf: &mut Vec<u8>, value: &impl Serialize) -> Result<(), Error> {
    engine::to_writer(buf, value)
}

/// Serialize `value` to a `String`.
#[inline]
pub fn to_string(value: &impl Serialize) -> Result<String, Error> {
    engine::to_string(value)
}

// ── Deserialization ────────────────────────────────────────────

/// Deserialize `T` from a byte slice.
#[inline]
pub fn from_slice<T: DeserializeOwned>(slice: &[u8]) -> Result<T, Error> {
    engine::from_slice(slice)
}

/// Deserialize `T` from a string slice.
#[inline]
pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, Error> {
    engine::from_str(s)
}
