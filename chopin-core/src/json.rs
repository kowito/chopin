//! Unified JSON abstraction for Chopin.
//!
//! When the `perf` feature is enabled, all serialization and deserialization
//! routes through **sonic-rs** — a SIMD-accelerated JSON library that is
//! measurably faster than serde_json for both parsing and writing.
//!
//! Without `perf`, standard serde_json is used so the framework compiles
//! everywhere without architecture-specific requirements.
//!
//! # Thread-local buffer reuse
//!
//! [`to_bytes`] serializes into a **thread-local `BytesMut`** and returns
//! zero-copy [`Bytes`]. The buffer is reused across requests, eliminating
//! heap allocation on the hot path. This is the same pattern used by
//! ntex (#8 in TFB Round 22, 1.2M req/s).

#[cfg(feature = "perf")]
use sonic_rs as engine;

#[cfg(not(feature = "perf"))]
use serde_json as engine;

use bytes::{BufMut, Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use std::cell::RefCell;

/// Re-export the underlying `Error` type so callers can pattern-match.
pub use engine::Error;

/// High-water mark for the thread-local serialization buffer.
/// 4 KB covers the vast majority of JSON API responses without
/// fragmentation. The buffer grows if needed and retains capacity.
const BUFFER_HW: usize = 4096;

thread_local! {
    /// Per-thread reusable serialization buffer.
    ///
    /// Retains capacity across requests so the hot path is zero-alloc.
    /// After [`to_bytes`] calls `split().freeze()`, the `BytesMut` keeps
    /// the remaining capacity for the next request. When the previous
    /// `Bytes` is dropped (response sent), the allocation can be reclaimed.
    static JSON_BUF: RefCell<BytesMut> = RefCell::new(BytesMut::with_capacity(BUFFER_HW));
}

// ── Serialization ──────────────────────────────────────────────

/// Serialize `value` to [`Bytes`] using a thread-local reusable buffer.
///
/// **Hot path** (>99% of calls after warmup): zero heap allocation.
/// The thread-local `BytesMut` is reused, and `split().freeze()` returns
/// a zero-copy `Bytes` view.
///
/// **Cold path** (first call per thread, or buffer exhausted): one
/// allocation to (re)grow the thread-local buffer.
///
/// This is the recommended serialization path for HTTP responses.
/// Used by [`crate::response::ApiResponse`] and [`crate::extractors::Json`].
#[inline]
pub fn to_bytes(value: &impl Serialize) -> Result<Bytes, Error> {
    JSON_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        buf.clear();
        if buf.capacity() < 128 {
            buf.reserve(BUFFER_HW);
        }
        // BufMut::writer() adapts BytesMut to std::io::Write.
        // Both sonic-rs and serde_json accept impl Write.
        engine::to_writer((&mut *buf).writer(), value)?;
        // split() extracts the data as a new BytesMut; the original keeps
        // the remaining capacity for the next request. freeze() converts
        // to Bytes (zero-copy, just a vtable swap).
        Ok(buf.split().freeze())
    })
}

/// Serialize `value` directly into a `Vec<u8>` buffer.
///
/// Both sonic-rs and serde_json implement `to_writer` for `&mut Vec<u8>`,
/// so we use a concrete type instead of a generic `Write` bound.
///
/// Prefer [`to_bytes`] for HTTP responses (avoids per-request allocation).
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
