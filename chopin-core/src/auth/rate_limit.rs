use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// In-memory sliding-window rate limiter.
///
/// Tracks login attempts per key (e.g. IP or email) and rejects when
/// the count exceeds `max_attempts` within `window`.
pub struct RateLimiter {
    max_attempts: u32,
    window: Duration,
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            max_attempts,
            window: Duration::from_secs(window_secs),
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Check if a key is rate-limited. Returns `Ok(())` if allowed,
    /// or `Err(seconds_until_retry)` if limited.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();
        let cutoff = now - self.window;

        let entries = map.entry(key.to_string()).or_default();
        entries.retain(|t| *t > cutoff);

        if entries.len() >= self.max_attempts as usize {
            // Find oldest entry to compute retry-after
            let oldest = entries.first().unwrap();
            let retry_after = self.window.as_secs() - now.duration_since(*oldest).as_secs();
            return Err(retry_after.max(1));
        }

        entries.push(now);
        Ok(())
    }

    /// Record an attempt for a key without checking the limit.
    pub fn record(&self, key: &str) {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();
        map.entry(key.to_string()).or_default().push(now);
    }

    /// Reset attempts for a key (e.g. after successful login).
    pub fn reset(&self, key: &str) {
        let mut map = self.attempts.lock().unwrap();
        map.remove(key);
    }

    /// Remove expired entries to prevent memory growth.
    /// Call this periodically (e.g. every 5 minutes).
    pub fn cleanup(&self) {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();
        let cutoff = now - self.window;
        map.retain(|_, entries| {
            entries.retain(|t| *t > cutoff);
            !entries.is_empty()
        });
    }
}
