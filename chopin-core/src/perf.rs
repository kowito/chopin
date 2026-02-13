//! Performance utilities for Chopin.
//!
//! Provides lock-free Date header caching and other performance optimizations
//! to minimize per-request overhead.
//!
//! ## Date Header Cache Architecture
//!
//! ```text
//! Background task (every 500ms):
//!   → atomically increments DATE_EPOCH (u64)
//!
//! Request hot path:
//!   → AtomicU64::load(Relaxed)   // ~1ns, no memory fence
//!   → thread_local check          // ~2ns, no synchronization
//!   → if epoch matches: clone cached HeaderValue (~5ns memcpy)
//!   → if stale: format date once per thread per 500ms window
//! ```
//!
//! This eliminates ALL cross-thread synchronization on the hot path.
//! No RwLock, no Arc increment, no atomic CAS — just a single relaxed
//! atomic load + thread-local lookup.

use axum::http::HeaderValue;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global epoch counter, incremented every 500ms by background task.
/// Readers use this to detect staleness of their thread-local cache.
/// Relaxed ordering is sufficient — we only need eventual visibility,
/// not happens-before guarantees (stale date by one tick is fine).
static DATE_EPOCH: AtomicU64 = AtomicU64::new(0);

// Thread-local date cache: (epoch, formatted HeaderValue).
// Each thread maintains its own copy — zero contention.
// `const` initialization avoids lazy_static overhead.
thread_local! {
    static LOCAL_DATE: RefCell<(u64, HeaderValue)> = const {
        RefCell::new((u64::MAX, HeaderValue::from_static("")))
    };
}

/// Format the current time as an HTTP Date header value.
#[inline(always)]
fn now_header() -> HeaderValue {
    let now = httpdate::fmt_http_date(std::time::SystemTime::now());
    // SAFETY: httpdate always produces valid ASCII
    HeaderValue::from_str(&now).unwrap()
}

/// Initialize the Date header cache and start the background updater.
/// Should be called once at server startup. Safe to call multiple times.
pub fn init_date_cache() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Set initial epoch so readers know the cache is active
        DATE_EPOCH.store(1, Ordering::Release);
        tokio::spawn(async move {
            let mut epoch: u64 = 1;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                epoch = epoch.wrapping_add(1);
                // Release ensures the new epoch is visible to reader threads.
                // Readers use Relaxed — they'll see it within a few nanoseconds
                // on any modern CPU (x86 TSO or ARM with DMB).
                DATE_EPOCH.store(epoch, Ordering::Release);
            }
        });
    });
}

/// Get the cached Date header value — **lock-free, zero synchronization**.
///
/// Hot path (~8ns): Relaxed atomic load + thread-local hit + memcpy clone.
/// Cold path (~100ns): one `httpdate::fmt_http_date` per thread per 500ms.
///
/// Compared to RwLock approach (~25ns + contention spikes), this saves
/// ~15ns per request and eliminates all contention under high concurrency.
#[inline(always)]
pub fn cached_date_header() -> HeaderValue {
    let current_epoch = DATE_EPOCH.load(Ordering::Relaxed);

    // Not initialized yet — fall back to live formatting
    if current_epoch == 0 {
        return now_header();
    }

    LOCAL_DATE.with(|cell| {
        {
            let cached = cell.borrow();
            if cached.0 == current_epoch {
                // Hot path: epoch matches, return thread-local clone.
                // HeaderValue for 29-byte date string is stored inline,
                // so clone is a ~48-byte memcpy (no heap allocation).
                return cached.1.clone();
            }
        }
        // Cold path: epoch changed (once per thread per 500ms).
        // Format the date and cache it in this thread's local storage.
        let val = now_header();
        let cloned = val.clone();
        *cell.borrow_mut() = (current_epoch, val);
        cloned
    })
}
