// src/metrics.rs
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(C, align(64))]
pub struct WorkerMetrics {
    pub req_count: AtomicUsize,
    pub active_conns: AtomicUsize,
    pub bytes_sent: AtomicUsize,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self {
            req_count: AtomicUsize::new(0),
            active_conns: AtomicUsize::new(0),
            bytes_sent: AtomicUsize::new(0),
        }
    }

    pub fn inc_req(&self) {
        self.req_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_conn(&self) {
        self.active_conns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_conn(&self) {
        self.active_conns.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, bytes: usize) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Initial state ────────────────────────────────────────────────────────

    #[test]
    fn test_new_all_counters_zero() {
        let m = WorkerMetrics::new();
        assert_eq!(m.req_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.active_conns.load(Ordering::Relaxed), 0);
        assert_eq!(m.bytes_sent.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_default_equals_new() {
        let m: WorkerMetrics = Default::default();
        assert_eq!(m.req_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.active_conns.load(Ordering::Relaxed), 0);
    }

    // ─── inc_req ──────────────────────────────────────────────────────────────

    #[test]
    fn test_inc_req_increments() {
        let m = WorkerMetrics::new();
        m.inc_req();
        assert_eq!(m.req_count.load(Ordering::Relaxed), 1);
        m.inc_req();
        m.inc_req();
        assert_eq!(m.req_count.load(Ordering::Relaxed), 3);
    }

    // ─── inc_conn / dec_conn ──────────────────────────────────────────────────

    #[test]
    fn test_inc_dec_conn() {
        let m = WorkerMetrics::new();
        m.inc_conn();
        m.inc_conn();
        assert_eq!(m.active_conns.load(Ordering::Relaxed), 2);
        m.dec_conn();
        assert_eq!(m.active_conns.load(Ordering::Relaxed), 1);
        m.dec_conn();
        assert_eq!(m.active_conns.load(Ordering::Relaxed), 0);
    }

    // ─── add_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn test_add_bytes_accumulates() {
        let m = WorkerMetrics::new();
        m.add_bytes(100);
        m.add_bytes(256);
        m.add_bytes(1024);
        assert_eq!(m.bytes_sent.load(Ordering::Relaxed), 1380);
    }

    #[test]
    fn test_add_bytes_zero_noop() {
        let m = WorkerMetrics::new();
        m.add_bytes(0);
        assert_eq!(m.bytes_sent.load(Ordering::Relaxed), 0);
    }

    // ─── alignment (cache-line isolation) ─────────────────────────────────────

    #[test]
    fn test_struct_align_is_64() {
        assert_eq!(
            std::mem::align_of::<WorkerMetrics>(),
            64,
            "WorkerMetrics must be 64-byte aligned (one full cache line)"
        );
    }

    // ─── multi-threaded correctness ───────────────────────────────────────────

    #[test]
    fn test_concurrent_inc_req() {
        use std::sync::Arc;
        let m = Arc::new(WorkerMetrics::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let mc = Arc::clone(&m);
            handles.push(std::thread::spawn(move || {
                for _ in 0..1_000 {
                    mc.inc_req();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(m.req_count.load(Ordering::Relaxed), 8_000);
    }
}
