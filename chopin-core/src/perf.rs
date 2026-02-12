//! Performance utilities for Chopin.
//!
//! Provides Date header caching and other performance optimizations
//! to minimize per-request overhead.

use axum::http::HeaderValue;
use std::sync::{Arc, OnceLock, RwLock};

/// Cached HTTP Date header, updated every 500ms by a background task.
///
/// Uses `std::sync::RwLock` (NOT `tokio::sync::RwLock`) because:
/// - Reads are synchronous and take nanoseconds (no async overhead)
/// - Multiple readers never block each other (read-heavy pattern)
/// - The write lock is held for ~50ns every 500ms (negligible contention)
/// - `tokio::sync::RwLock::try_read()` can FAIL under contention,
///   causing expensive fallback to `SystemTime::now()` + format
static CACHED_DATE: OnceLock<Arc<RwLock<HeaderValue>>> = OnceLock::new();

fn now_header() -> HeaderValue {
    let now = httpdate::fmt_http_date(std::time::SystemTime::now());
    HeaderValue::from_str(&now).unwrap()
}

/// Initialize the Date header cache and start the background updater.
/// Should be called once at server startup. Safe to call multiple times.
pub fn init_date_cache() {
    let _ = CACHED_DATE.get_or_init(|| {
        let val = Arc::new(RwLock::new(now_header()));
        let val_clone = val.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                let hv = now_header();
                // write() blocks readers for ~nanoseconds — negligible
                *val_clone.write().unwrap_or_else(|e| e.into_inner()) = hv;
            }
        });
        val
    });
}

/// Get the cached Date header value.
/// Falls back to computing it live if the cache isn't initialized.
///
/// This is a synchronous read — no async runtime interaction, no task
/// scheduling, just an atomic compare on the RwLock. Multiple readers
/// proceed concurrently (no blocking).
#[inline(always)]
pub fn cached_date_header() -> HeaderValue {
    match CACHED_DATE.get() {
        Some(lock) => lock.read().unwrap_or_else(|e| e.into_inner()).clone(),
        None => now_header(),
    }
}
