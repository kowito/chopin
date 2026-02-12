//! Performance utilities for Chopin.
//!
//! Provides Date header caching and other performance optimizations
//! to minimize per-request overhead.

use axum::http::HeaderValue;
use std::sync::{Arc, OnceLock};

/// We piggyback on tokio's parking_lot feature (already enabled) for
/// a faster RwLock — no poisoning overhead, smaller memory footprint.
use tokio::sync::RwLock;

/// Cached HTTP Date header, updated every 500ms by a background task.
/// Using a tokio::sync::RwLock inside an Arc avoids std::sync::RwLock
/// poisoning and is designed for async contexts.
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
                *val_clone.write().await = hv;
            }
        });
        val
    });
}

/// Get the cached Date header value.
/// Falls back to computing it live if the cache isn't initialized.
#[inline]
pub fn cached_date_header() -> HeaderValue {
    CACHED_DATE
        .get()
        .and_then(|v| v.try_read().ok().map(|h| h.clone()))
        .unwrap_or_else(now_header)
}
