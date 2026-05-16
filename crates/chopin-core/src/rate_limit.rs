// src/rate_limit.rs
//
// Per-IP token-bucket rate limiter implemented as a standard Chopin middleware.
//
// Design
// ------
// - **No shared state**: each worker thread has its own `thread_local!` bucket
//   map.  Zero mutexes, zero atomics in the hot path.
// - **Token bucket algorithm**: allows short bursts up to `capacity` while
//   sustaining `rate` requests/second long-term.
// - **Client IP**: read from `X-Forwarded-For` → `X-Real-IP` → `"unknown"`.
//   Works transparently behind nginx, AWS ALB, or any standard reverse proxy.
// - **LRU-style eviction**: when the per-thread map exceeds `MAX_BUCKETS`
//   entries, stale buckets are cleared to prevent unbounded memory growth.
// - **Configurable**: call [`configure`] once at startup (before spawning
//   worker threads) or per-worker; the values are stored in static atomics so
//   every worker thread reads the same configuration.
//
// Usage
// -----
// ```rust
// use chopin_core::rate_limit;
//
// // 100 requests per 60 seconds (burst up to 100)
// rate_limit::configure(100, 60);
//
// let mut router = Router::new();
// router.layer(rate_limit::per_ip);   // register as global middleware
// ```

use crate::http::{Context, Response};
use crate::router::BoxedHandler;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Global configuration ──────────────────────────────────────────────────────

/// Maximum requests allowed in a single window (token bucket capacity).
static RATE_LIMIT_CAPACITY: AtomicU64 = AtomicU64::new(100);
/// Window duration in seconds over which tokens refill to capacity.
static RATE_LIMIT_WINDOW_SECS: AtomicU64 = AtomicU64::new(60);

/// Set the global rate-limit parameters.
///
/// Call this once before `serve()` or `Server::bind()`.  The values are
/// stored in static atomics and are visible to every worker thread.
///
/// * `capacity`     – maximum requests allowed per `window_secs` (burst cap)
/// * `window_secs`  – refill window in seconds
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::rate_limit;
/// rate_limit::configure(200, 60); // 200 req/min per client IP
/// ```
pub fn configure(capacity: u64, window_secs: u64) {
    RATE_LIMIT_CAPACITY.store(capacity.max(1), Ordering::Relaxed);
    RATE_LIMIT_WINDOW_SECS.store(window_secs.max(1), Ordering::Relaxed);
}

// ── Per-thread bucket map ─────────────────────────────────────────────────────

/// Maximum number of IP buckets to keep per worker thread.
/// When this limit is hit, buckets that have been fully refilled (idle)
/// are evicted to reclaim memory.
const MAX_BUCKETS: usize = 8_192;

#[derive(Clone)]
struct Bucket {
    /// Current token count (fractional to support sub-second precision).
    tokens: f64,
    /// Instant of the last token refill.
    last_refill: Instant,
}

impl Bucket {
    fn new(capacity: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens proportional to elapsed time, then try to consume one.
    /// Returns `true` if the request is allowed, `false` if throttled.
    fn try_consume(&mut self, capacity: f64, window_secs: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        // Tokens refill linearly: capacity tokens per window_secs seconds.
        let refill = elapsed * (capacity / window_secs);
        self.tokens = (self.tokens + refill).min(capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// True if this bucket has been fully refilled and can be safely evicted.
    fn is_idle(&self, capacity: f64, window_secs: f64) -> bool {
        let elapsed = Instant::now()
            .duration_since(self.last_refill)
            .as_secs_f64();
        self.tokens + elapsed * (capacity / window_secs) >= capacity
    }
}

thread_local! {
    static BUCKETS: RefCell<HashMap<[u8; 16], Bucket>> = RefCell::new(HashMap::new());
}

// ── IP extraction ─────────────────────────────────────────────────────────────

/// Extract the first IP address from `X-Forwarded-For`, falling back to
/// `X-Real-IP`, and finally returning `[0u8; 16]` (the "unknown" key) if
/// neither header is present.
///
/// The returned value is a 16-byte array used as the HashMap key:
/// - IPv4 addresses are encoded as IPv4-mapped IPv6 (`::ffff:a.b.c.d`)
/// - IPv6 addresses fill all 16 bytes
/// - "unknown" returns all zeros
fn extract_ip_key(ctx: &Context) -> [u8; 16] {
    // 1. X-Forwarded-For: pick the *first* (leftmost) address — that is the
    //    original client IP as set by the outermost trusted proxy.
    let raw = ctx
        .header("x-forwarded-for")
        .or_else(|| ctx.header("x-real-ip"));

    let ip_str = match raw {
        None => return [0u8; 16],
        Some(s) => {
            // X-Forwarded-For may be "ip1, ip2, ip3" — take the first token.
            s.split(',').next().unwrap_or("").trim()
        }
    };

    parse_ip_to_key(ip_str)
}

/// Parse an IP string (v4 or v6) into a 16-byte key. Returns `[0u8; 16]` on
/// failure so unknown/malformed IPs share the same (very permissive) bucket.
fn parse_ip_to_key(s: &str) -> [u8; 16] {
    // Try IPv4 first (fast path — most common case)
    if let Ok(addr) = s.parse::<std::net::Ipv4Addr>() {
        // Map to IPv4-in-IPv6: [0,0,0,0,0,0,0,0,0,0,0xff,0xff, a,b,c,d]
        let octs = addr.octets();
        let mut key = [0u8; 16];
        key[10] = 0xff;
        key[11] = 0xff;
        key[12..16].copy_from_slice(&octs);
        return key;
    }
    // Try IPv6
    if let Ok(addr) = s.parse::<std::net::Ipv6Addr>() {
        return addr.octets();
    }
    [0u8; 16]
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Chopin middleware function that enforces per-IP rate limiting.
///
/// Register it with `router.layer(rate_limit::per_ip)`.  Must call
/// [`configure`] beforehand (or rely on the defaults: 100 req/60 s).
///
/// Returns `429 Too Many Requests` with a `Retry-After` header when the
/// client's token bucket is exhausted.
pub fn per_ip(ctx: Context, next: BoxedHandler) -> Response {
    let capacity = RATE_LIMIT_CAPACITY.load(Ordering::Relaxed) as f64;
    let window_secs = RATE_LIMIT_WINDOW_SECS.load(Ordering::Relaxed) as f64;

    let ip_key = extract_ip_key(&ctx);

    let allowed = BUCKETS.with(|cell| {
        let mut map = cell.borrow_mut();

        // Evict idle buckets when map is too large to prevent unbounded growth.
        if map.len() >= MAX_BUCKETS {
            map.retain(|_, b| !b.is_idle(capacity, window_secs));
        }

        let bucket = map
            .entry(ip_key)
            .or_insert_with(|| Bucket::new(capacity));
        bucket.try_consume(capacity, window_secs)
    });

    if allowed {
        next(ctx)
    } else {
        // Compute approximate wait time until one token is available.
        let retry_after = (window_secs / capacity).ceil() as u64;
        let retry_secs = retry_after.max(1);
        Response::new(429)
            .with_header("Retry-After", retry_secs)
            .with_header("Content-Type", "text/plain")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        use std::time::Duration;
        let capacity = 10.0;
        let window = 1.0; // 1 second window for fast test
        let mut b = Bucket { tokens: 0.0, last_refill: Instant::now() };

        // Simulate 0.5s elapsed by manipulating last_refill
        b.last_refill = Instant::now() - Duration::from_millis(500);
        // After 0.5s with 10 tokens/second rate: 5 tokens refilled
        assert!(b.try_consume(capacity, window)); // consumes 1
        // Now ~4 tokens remain — allow 4 more
        for _ in 0..4 {
            assert!(b.try_consume(capacity, window));
        }
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
        // Reset to defaults for other tests
        configure(100, 60);
    }

    #[test]
    fn test_bucket_is_idle_when_full() {
        let b = Bucket::new(10.0); // starts full
        assert!(b.is_idle(10.0, 60.0));
    }

    #[test]
    fn test_bucket_not_idle_when_consumed() {
        let mut b = Bucket::new(10.0);
        b.try_consume(10.0, 60.0);
        assert!(!b.is_idle(10.0, 60.0));
    }
}
