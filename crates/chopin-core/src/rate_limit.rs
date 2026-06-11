//! Per-IP token-bucket rate limiter implemented as a standard Chopin middleware.
//!
//! Each worker thread maintains its own bucket map — zero mutexes, zero atomics
//! in the hot path.  IP resolution priority: `X-Real-IP` → `X-Forwarded-For`
//! (with configurable trusted-proxy depth stripping) → socket peer address.
//!
//! # ⚠️ Important: Rate limiting is per-worker-thread
//!
//! Because each worker thread has an independent bucket map, the **effective
//! global rate limit is N × `capacity`**, where N is the number of worker
//! threads.  For example, `configure(100, 60)` with 4 workers allows up to
//! **400 requests/second per IP** across the entire server, not 100.
//!
//! To enforce a consistent global limit, divide your desired capacity by the
//! number of workers:
//!
//! ```rust,no_run
//! # use chopin_core::rate_limit;
//! let desired_global_rps = 100;
//! let num_workers = 4;
//! rate_limit::configure(desired_global_rps / num_workers, 60); // 25 per worker = 100 global
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! use chopin_core::{Router, rate_limit};
//!
//! // 100 requests per 60 s per IP (burst up to 100)
//! rate_limit::configure(100, 60);
//!
//! // Trust one reverse proxy (e.g. nginx) in front of the server
//! rate_limit::set_trusted_depth(1);
//!
//! let mut router = Router::new();
//! router.layer(rate_limit::per_ip);
//! ```
// src/rate_limit.rs
//
// Per-IP token-bucket rate limiter implemented as a standard Chopin middleware.
//
// Design
// ------
// - **No shared state**: each worker thread has its own `thread_local!` bucket
//   map.  Zero mutexes, zero atomics in the hot path.
// - **Token bucket algorithm**: allows short bursts up to `capacity` while
//   sustaining `capacity/window_secs` requests/second long-term.
// - **IP source** (in priority order):
//   1. `X-Real-IP` — set by nginx/caddy/ALB to `$remote_addr`; clients cannot
//      override it because the proxy replaces any client-supplied value.
//   2. `X-Forwarded-For` with `trusted_depth` stripping — when `trusted_depth`
//      is configured > 0, the Nth-from-right IP is used (N = trusted_depth).
//      Handles multi-hop proxy chains (CDN → nginx → app).
//   3. Socket peer address — captured via `getpeername(2)` at accept time and
//      stored in `Conn.peer_addr`. Always available, cannot be forged.
// - **Truly bounded map**: two-pass eviction (idle then LRU) guarantees the
//   per-thread bucket map never grows past `MAX_BUCKETS`.
// - **Configurable**: call [`configure`] at startup; values live in static
//   atomics so every worker thread shares the same configuration.
//
// Usage
// -----
// ```rust
// use chopin_core::rate_limit;
//
// // 100 requests per 60 seconds per IP (burst up to 100)
// rate_limit::configure(100, 60);
//
// // Optional: trust one reverse proxy (nginx/ALB) in front of the server
// rate_limit::set_trusted_depth(1);
//
// let mut router = Router::new();
// router.layer(rate_limit::per_ip);
// ```

use crate::http::{Context, Response};
use crate::router::BoxedHandler;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

// ── Global configuration ──────────────────────────────────────────────────────

static RATE_LIMIT_CAPACITY: AtomicU64 = AtomicU64::new(100);
static RATE_LIMIT_WINDOW_SECS: AtomicU64 = AtomicU64::new(60);
static TRUSTED_DEPTH: AtomicU8 = AtomicU8::new(0);

/// Set the global rate-limit parameters.
///
/// * `capacity`    – maximum requests allowed per `window_secs` (burst cap)
/// * `window_secs` – refill window in seconds
///
/// # ⚠️ Per-worker multiplier
///
/// These parameters are enforced **per worker thread**, not globally.  Each
/// worker maintains an independent token-bucket map.  With N workers, the
/// effective global rate limit is **N × `capacity`** requests per IP per
/// window.
///
/// For a consistent global limit, divide your desired capacity by the number
/// of workers:
///
/// ```rust,no_run
/// # use chopin_core::rate_limit;
/// // 4 workers, want 100 rps global → 25 per worker
/// rate_limit::configure(25, 60);
/// ```
///
/// This design avoids all mutex and atomic contention on the request hot path.
pub fn configure(capacity: u64, window_secs: u64) {
    RATE_LIMIT_CAPACITY.store(capacity.max(1), Ordering::Relaxed);
    RATE_LIMIT_WINDOW_SECS.store(window_secs.max(1), Ordering::Relaxed);
}

/// Set the number of trusted reverse proxies in front of this server.
///
/// - `0` (default): ignore `X-Forwarded-For`; use `X-Real-IP` or the socket
///   peer address.  Safe against XFF header injection by clients.
/// - `1`: one trusted proxy (nginx/ALB); strip the rightmost XFF entry and
///   use the next as the client IP.
/// - `N`: strip N rightmost XFF entries.
///
/// **Security**: set `depth > 0` only when you control the proxy layer.
/// Clients behind a trusted proxy can still forge additional XFF entries;
/// the stripping only removes entries added by your own proxies.
pub fn set_trusted_depth(depth: u8) {
    TRUSTED_DEPTH.store(depth, Ordering::Relaxed);
}

// ── Per-thread bucket map ─────────────────────────────────────────────────────

const MAX_BUCKETS: usize = 8_192;

#[derive(Clone)]
struct Bucket {
    tokens: f64,
    /// Instant of last token refill — doubles as the LRU access timestamp.
    last_refill: Instant,
}

impl Bucket {
    fn new(capacity: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self, capacity: f64, window_secs: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * (capacity / window_secs)).min(capacity);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Seconds until the next token becomes available.
    fn retry_after_secs(&self, capacity: f64, window_secs: f64) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        ((1.0 - self.tokens) * window_secs / capacity).ceil() as u64
    }

    fn is_idle(&self, capacity: f64, window_secs: f64) -> bool {
        let elapsed = Instant::now()
            .duration_since(self.last_refill)
            .as_secs_f64();
        self.tokens + elapsed * (capacity / window_secs) >= capacity
    }
}

thread_local! {
    static BUCKETS: RefCell<HashMap<[u8; 16], Bucket>> =
        RefCell::new(HashMap::with_capacity(256));
}

// ── Eviction ──────────────────────────────────────────────────────────────────

/// Keep the per-thread bucket map within `MAX_BUCKETS`.
///
/// Pass 1: remove fully-refilled (idle) buckets.
/// Pass 2 (fallback): evict the oldest 25% by last_refill (LRU).
fn evict_if_full(map: &mut HashMap<[u8; 16], Bucket>, capacity: f64, window_secs: f64) {
    if map.len() < MAX_BUCKETS {
        return;
    }
    map.retain(|_, b| !b.is_idle(capacity, window_secs));
    if map.len() < MAX_BUCKETS {
        return;
    }
    // LRU eviction: drop the oldest 25% of buckets by last_refill.
    let mut times: Vec<Instant> = map.values().map(|b| b.last_refill).collect();
    times.sort_unstable();
    let cutoff = times[times.len() / 4];
    map.retain(|_, b| b.last_refill > cutoff);
}

// ── IP extraction ─────────────────────────────────────────────────────────────

fn extract_ip_key(ctx: &Context) -> [u8; 16] {
    // 1. X-Real-IP: set by proxy to $remote_addr — cannot be forged by clients.
    if let Some(ip) = ctx.header("x-real-ip") {
        let key = parse_ip_to_key(ip.trim());
        if key != [0u8; 16] {
            return key;
        }
    }

    // 2. X-Forwarded-For with trusted-depth stripping (only when depth > 0).
    let depth = TRUSTED_DEPTH.load(Ordering::Relaxed) as usize;
    if depth > 0
        && let Some(xff) = ctx.header("x-forwarded-for")
    {
        let ips: Vec<&str> = xff.split(',').map(str::trim).collect();
        // Strip `depth` rightmost entries (added by our trusted proxies),
        // then take the next entry as the originating client IP.
        if ips.len() > depth {
            let key = parse_ip_to_key(ips[ips.len() - 1 - depth]);
            if key != [0u8; 16] {
                return key;
            }
        }
    }

    // 3. Socket peer address — always available, set at accept time.
    ctx.peer_addr
}

fn parse_ip_to_key(s: &str) -> [u8; 16] {
    if let Ok(addr) = s.parse::<std::net::Ipv4Addr>() {
        let mut key = [0u8; 16];
        key[10] = 0xff;
        key[11] = 0xff;
        key[12..16].copy_from_slice(&addr.octets());
        return key;
    }
    if let Ok(addr) = s.parse::<std::net::Ipv6Addr>() {
        return addr.octets();
    }
    [0u8; 16]
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Per-IP token-bucket rate limiting middleware.
///
/// Register with `router.layer(rate_limit::per_ip)`.  Configure with
/// [`configure`] and optionally [`set_trusted_depth`].
///
/// Returns `429 Too Many Requests` with `Retry-After`, `X-RateLimit-Limit`,
/// and `X-RateLimit-Reset` headers when the token bucket is exhausted.
///
/// # ⚠️ Per-worker enforcement
///
/// This middleware runs independently on each worker thread using a
/// `thread_local!` bucket map.  A client may hit **up to N × `capacity`**
/// requests before being rate-limited, where N is the number of workers.
/// See the [module-level documentation](self) for details on configuring
/// a consistent global limit.
pub fn per_ip(ctx: Context, next: BoxedHandler) -> Response {
    let capacity = RATE_LIMIT_CAPACITY.load(Ordering::Relaxed) as f64;
    let window_secs = RATE_LIMIT_WINDOW_SECS.load(Ordering::Relaxed) as f64;
    let ip_key = extract_ip_key(&ctx);

    let (allowed, retry_secs) = BUCKETS.with(|cell| {
        let mut map = cell.borrow_mut();
        evict_if_full(&mut map, capacity, window_secs);
        let bucket = map.entry(ip_key).or_insert_with(|| Bucket::new(capacity));
        let allowed = bucket.try_consume(capacity, window_secs);
        let retry = if allowed {
            0
        } else {
            bucket.retry_after_secs(capacity, window_secs)
        };
        (allowed, retry)
    });

    if allowed {
        next(ctx)
    } else {
        Response::new(429)
            .with_header("Retry-After", retry_secs)
            .with_header("Content-Type", "text/plain; charset=utf-8")
            .with_header("X-RateLimit-Limit", capacity as u64)
            .with_header("X-RateLimit-Reset", retry_secs)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_bucket_allows_up_to_capacity() {
        let mut b = Bucket::new(5.0);
        for _ in 0..5 {
            assert!(b.try_consume(5.0, 60.0));
        }
        assert!(!b.try_consume(5.0, 60.0));
    }

    #[test]
    fn test_bucket_refills_over_time() {
        let capacity = 10.0;
        let window = 1.0;
        let mut b = Bucket {
            tokens: 0.0,
            last_refill: Instant::now(),
        };
        b.last_refill = Instant::now() - Duration::from_millis(500);
        // 0.5s × 10 tokens/s = 5 tokens refilled; consume all 5
        for _ in 0..5 {
            assert!(b.try_consume(capacity, window));
        }
        // 6th should fail (all tokens consumed)
        assert!(!b.try_consume(capacity, window));
    }

    #[test]
    fn test_retry_after_secs() {
        let mut b = Bucket::new(10.0);
        for _ in 0..10 {
            b.try_consume(10.0, 60.0);
        }
        // 0 tokens left; each token refills in 60/10 = 6 s
        let secs = b.retry_after_secs(10.0, 60.0);
        assert!(secs >= 6, "expected >=6, got {secs}");
    }

    #[test]
    fn test_parse_ipv4_key() {
        let key = parse_ip_to_key("192.168.1.1");
        assert_eq!(&key[10..12], &[0xff, 0xff]);
        assert_eq!(&key[12..16], &[192, 168, 1, 1]);
    }

    #[test]
    fn test_parse_ipv6_key() {
        let key = parse_ip_to_key("::1");
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(key, expected);
    }

    #[test]
    fn test_parse_unknown_returns_zeros() {
        assert_eq!(parse_ip_to_key("not-an-ip"), [0u8; 16]);
    }

    #[test]
    fn test_configure_updates_globals() {
        configure(50, 30);
        assert_eq!(RATE_LIMIT_CAPACITY.load(Ordering::Relaxed), 50);
        assert_eq!(RATE_LIMIT_WINDOW_SECS.load(Ordering::Relaxed), 30);
        configure(100, 60);
    }

    #[test]
    fn test_set_trusted_depth() {
        set_trusted_depth(2);
        assert_eq!(TRUSTED_DEPTH.load(Ordering::Relaxed), 2);
        set_trusted_depth(0);
    }

    #[test]
    fn test_bucket_is_idle_when_full() {
        let b = Bucket::new(10.0);
        assert!(b.is_idle(10.0, 60.0));
    }

    #[test]
    fn test_bucket_not_idle_when_consumed() {
        let mut b = Bucket::new(10.0);
        b.try_consume(10.0, 60.0);
        assert!(!b.is_idle(10.0, 60.0));
    }

    #[test]
    fn test_evict_removes_idle_buckets() {
        let capacity = 5.0;
        let window = 60.0;
        let mut map: HashMap<[u8; 16], Bucket> = HashMap::new();
        for i in 0..MAX_BUCKETS as u32 {
            let mut key = [0u8; 16];
            key[12..16].copy_from_slice(&i.to_be_bytes());
            map.insert(key, Bucket::new(capacity)); // full = idle
        }
        evict_if_full(&mut map, capacity, window);
        assert!(map.len() < MAX_BUCKETS);
    }

    #[test]
    fn test_evict_lru_fallback_when_all_active() {
        let capacity = 5.0;
        let window = 60.0;
        let mut map: HashMap<[u8; 16], Bucket> = HashMap::new();
        for i in 0..MAX_BUCKETS as u32 {
            let mut key = [0u8; 16];
            key[12..16].copy_from_slice(&i.to_be_bytes());
            let mut b = Bucket::new(capacity);
            for _ in 0..capacity as usize {
                b.try_consume(capacity, window);
            }
            map.insert(key, b); // all tokens consumed = not idle
        }
        let before = map.len();
        evict_if_full(&mut map, capacity, window);
        assert!(map.len() < before, "LRU eviction should have freed buckets");
        assert!(map.len() <= MAX_BUCKETS * 3 / 4 + 1);
    }
}
