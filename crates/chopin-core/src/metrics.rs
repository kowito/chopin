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
