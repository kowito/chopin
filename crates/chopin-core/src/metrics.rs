// src/metrics.rs
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

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

// ─── Global Metrics Registry ──────────────────────────────────────────────────

/// Global registry for all per-worker metric sets.
/// Populated by `Server::serve()` before spawning worker threads.
static GLOBAL_METRICS: OnceLock<Arc<Vec<Arc<WorkerMetrics>>>> = OnceLock::new();

/// Global server start time for uptime reporting.
static START_TIME: OnceLock<Instant> = OnceLock::new();

/// Register all worker metric handles globally so that the `/metrics` and
/// `/health` handlers can read them from any thread.
///
/// Must be called once, before workers start — subsequent calls are no-ops.
pub fn register_global_metrics(metrics: Arc<Vec<Arc<WorkerMetrics>>>) {
    let _ = GLOBAL_METRICS.set(metrics);
    let _ = START_TIME.set(Instant::now());
}

/// Returns total request count across all workers.
fn total_requests() -> usize {
    GLOBAL_METRICS
        .get()
        .map(|m| m.iter().map(|w| w.req_count.load(Ordering::Relaxed)).sum())
        .unwrap_or(0)
}

/// Returns total active connections across all workers.
fn total_active_conns() -> usize {
    GLOBAL_METRICS
        .get()
        .map(|m| {
            m.iter()
                .map(|w| w.active_conns.load(Ordering::Relaxed))
                .sum()
        })
        .unwrap_or(0)
}

/// Returns total bytes sent across all workers.
fn total_bytes_sent() -> usize {
    GLOBAL_METRICS
        .get()
        .map(|m| m.iter().map(|w| w.bytes_sent.load(Ordering::Relaxed)).sum())
        .unwrap_or(0)
}

/// Returns uptime in seconds (0 before server start).
pub fn uptime_secs() -> u64 {
    START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}

/// Prometheus text format handler — mount at `/metrics` via `Chopin::with_metrics()`.
///
/// Renders per-worker counters/gauges plus aggregate totals in the
/// [Prometheus exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/).
pub fn prometheus_metrics_handler(_ctx: crate::http::Context) -> crate::http::Response {
    let Some(metrics) = GLOBAL_METRICS.get() else {
        return crate::http::Response::server_error();
    };

    let mut buf = String::with_capacity(1024);

    // ── chopin_requests_total ─────────────────────────────────────────────────
    buf.push_str("# HELP chopin_requests_total Total HTTP requests processed.\n");
    buf.push_str("# TYPE chopin_requests_total counter\n");
    let mut agg_req: usize = 0;
    for (i, w) in metrics.iter().enumerate() {
        let v = w.req_count.load(Ordering::Relaxed);
        agg_req += v;
        buf.push_str(&format!("chopin_requests_total{{worker=\"{i}\"}} {v}\n"));
    }
    buf.push_str(&format!(
        "chopin_requests_total{{worker=\"all\"}} {agg_req}\n"
    ));

    // ── chopin_active_connections ─────────────────────────────────────────────
    buf.push_str("\n# HELP chopin_active_connections Currently open connections.\n");
    buf.push_str("# TYPE chopin_active_connections gauge\n");
    let mut agg_conns: usize = 0;
    for (i, w) in metrics.iter().enumerate() {
        let v = w.active_conns.load(Ordering::Relaxed);
        agg_conns += v;
        buf.push_str(&format!(
            "chopin_active_connections{{worker=\"{i}\"}} {v}\n"
        ));
    }
    buf.push_str(&format!(
        "chopin_active_connections{{worker=\"all\"}} {agg_conns}\n"
    ));

    // ── chopin_bytes_sent_total ───────────────────────────────────────────────
    buf.push_str("\n# HELP chopin_bytes_sent_total Total bytes sent to clients.\n");
    buf.push_str("# TYPE chopin_bytes_sent_total counter\n");
    let mut agg_bytes: usize = 0;
    for (i, w) in metrics.iter().enumerate() {
        let v = w.bytes_sent.load(Ordering::Relaxed);
        agg_bytes += v;
        buf.push_str(&format!("chopin_bytes_sent_total{{worker=\"{i}\"}} {v}\n"));
    }
    buf.push_str(&format!(
        "chopin_bytes_sent_total{{worker=\"all\"}} {agg_bytes}\n"
    ));

    // ── chopin_workers_total ──────────────────────────────────────────────────
    buf.push_str("\n# HELP chopin_workers_total Number of worker threads.\n");
    buf.push_str("# TYPE chopin_workers_total gauge\n");
    buf.push_str(&format!("chopin_workers_total {}\n", metrics.len()));

    // ── chopin_uptime_seconds ─────────────────────────────────────────────────
    buf.push_str("\n# HELP chopin_uptime_seconds Server uptime in seconds.\n");
    buf.push_str("# TYPE chopin_uptime_seconds counter\n");
    buf.push_str(&format!("chopin_uptime_seconds {}\n", uptime_secs()));

    let mut resp = crate::http::Response::new(200);
    // Prometheus text format content-type
    resp.content_type = "text/plain; version=0.0.4; charset=utf-8";
    resp.body = crate::http::Body::Bytes(buf.into_bytes());
    resp
}

/// Built-in health check handler — mount at `/health` via `Chopin::with_health()`.
///
/// Returns `200 OK` with a JSON body:
/// ```json
/// {"status":"ok","uptime_secs":123,"workers":4,"requests":9876,"connections":12}
/// ```
pub fn health_handler(_ctx: crate::http::Context) -> crate::http::Response {
    let uptime = uptime_secs();
    let workers = GLOBAL_METRICS.get().map(|m| m.len()).unwrap_or(0);
    let requests = total_requests();
    let connections = total_active_conns();
    let bytes = total_bytes_sent();

    let body = format!(
        r#"{{"status":"ok","uptime_secs":{uptime},"workers":{workers},"requests":{requests},"active_connections":{connections},"bytes_sent":{bytes}}}"#,
    );
    crate::http::Response::json_bytes(body.into_bytes())
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
