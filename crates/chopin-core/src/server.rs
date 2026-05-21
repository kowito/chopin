// src/server.rs
use crate::error::ChopinError;
use crate::router::Router;
use crate::syscalls::{self};
use crate::worker::Worker;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// High-level application builder for Chopin.
///
/// Collects routes registered via `#[get]`/`#[post]`/… macros, optionally
/// mounts OpenAPI documentation, and starts the multi-threaded server.
///
/// # Example
///
/// ```rust,no_run
/// use chopin_core::{get, Context, Response, Chopin};
///
/// #[get("/")]
/// fn index(_ctx: Context) -> Response {
///     Response::text("Hello!")
/// }
///
/// fn main() {
///     Chopin::new()
///         .mount_all_routes()
///         .serve("0.0.0.0:8080")
///         .unwrap();
/// }
/// ```
pub struct Chopin {
    router: Router,
    max_request_size: Option<usize>,
    worker_init: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for Chopin {
    fn default() -> Self {
        Self::new()
    }
}

impl Chopin {
    /// Create a new Chopin application with an empty router.
    pub fn new() -> Self {
        Self {
            router: Router::new(),
            max_request_size: None,
            worker_init: None,
        }
    }

    /// Register a callback that runs once on every worker thread at startup.
    ///
    /// Use this to initialise per-thread resources such as database pools:
    ///
    /// ```rust,ignore
    /// Chopin::new()
    ///     .with_worker_init(|| {
    ///         chopin_pg::init_pool("postgres://user:pass@localhost/mydb", 4)
    ///             .expect("pool init failed");
    ///     })
    ///     .mount_all_routes()
    ///     .serve("0.0.0.0:8080")
    ///     .unwrap();
    /// ```
    pub fn with_worker_init(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.worker_init = Some(Arc::new(f));
        self
    }

    /// Discover and register all routes annotated with `#[get]`, `#[post]`, etc.
    pub fn mount_all_routes(mut self) -> Self {
        for route in inventory::iter::<crate::router::RouteDef> {
            self.router.add(route.method, route.path, route.handler);
        }
        self.router.finalize();
        self
    }

    /// Enable the built-in OpenAPI documentation at `/openapi.json` and `/docs`.
    pub fn with_openapi(mut self) -> Self {
        self.router
            .get("/openapi.json", crate::openapi::openapi_json_handler);
        self.router
            .get("/docs", crate::openapi::scalar_docs_handler);
        self
    }

    /// Mount the built-in Prometheus metrics endpoint at the given path.
    ///
    /// The metrics page is available immediately after the server starts.
    /// Aggregate counters across all worker threads are rendered in the
    /// [Prometheus exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Chopin::new().mount_all_routes().with_metrics("/metrics").serve("0.0.0.0:8080").unwrap();
    /// ```
    pub fn with_metrics(mut self, path: &'static str) -> Self {
        self.router
            .get(path, crate::metrics::prometheus_metrics_handler);
        self
    }

    /// Mount a built-in health check endpoint at the given path.
    ///
    /// Returns `200 OK` with a JSON body containing server uptime, worker
    /// count, total request count, and active connection count. Suitable for
    /// Kubernetes liveness/readiness probes and AWS ALB health checks.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Chopin::new().mount_all_routes().with_health("/health").serve("0.0.0.0:8080").unwrap();
    /// ```
    pub fn with_health(mut self, path: &'static str) -> Self {
        self.router.get(path, crate::metrics::health_handler);
        self
    }

    /// Initialise structured JSON logging to stderr using `tracing-subscriber`.
    ///
    /// Log level is controlled by the `RUST_LOG` environment variable
    /// (default: `info`). Requires the `logging` feature flag.
    ///
    /// # Example
    ///
    /// ```bash
    /// RUST_LOG=debug cargo run
    /// ```
    #[cfg(feature = "logging")]
    pub fn with_logging(self) -> Self {
        use tracing_subscriber::{EnvFilter, fmt};
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        fmt().json().with_env_filter(filter).init();
        self
    }

    /// Override the maximum allowed request size (headers + body) in bytes.
    ///
    /// Requests exceeding this are rejected with `413 Content Too Large`.
    /// Defaults to 1 MiB (overridable via `CHOPIN_MAX_REQUEST_SIZE` env var).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Chopin::new()
    ///     .mount_all_routes()
    ///     .with_max_request_size(10 * 1024 * 1024) // 10 MiB
    ///     .serve("0.0.0.0:8080")
    ///     .unwrap();
    /// ```
    pub fn with_max_request_size(mut self, bytes: usize) -> Self {
        self.max_request_size = Some(bytes);
        self
    }

    /// Start the server, binding to `host_port` (e.g. `"0.0.0.0:8080"`).
    pub fn serve(self, host_port: &str) -> crate::error::ChopinResult<()> {
        let mut server = Server::bind(host_port);
        if let Some(size) = self.max_request_size {
            server = server.with_max_request_size(size);
        }
        if let Some(init) = self.worker_init {
            server = server.with_worker_init_arc(init);
        }
        server.serve(self.router)
    }

    /// Start the server with TLS, binding to `host_port`.
    ///
    /// Requires the `tls` feature flag.
    #[cfg(feature = "tls")]
    pub fn serve_tls(
        self,
        host_port: &str,
        cert_path: &str,
        key_path: &str,
    ) -> crate::error::ChopinResult<()> {
        let mut server = Server::bind(host_port).with_tls(cert_path, key_path)?;
        if let Some(size) = self.max_request_size {
            server = server.with_max_request_size(size);
        }
        if let Some(init) = self.worker_init {
            server = server.with_worker_init_arc(init);
        }
        server.serve(self.router)
    }
}

/// Low-level multi-threaded server.
///
/// Use this when you want full control over the [`Router`] (e.g. adding
/// middleware, merging sub-routers) instead of the macro-driven [`Chopin`]
/// builder.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::{Router, Server, Context, Response};
///
/// fn ping(_ctx: Context) -> Response { Response::text("pong") }
///
/// let mut router = Router::new();
/// router.get("/ping", ping);
///
/// Server::bind("0.0.0.0:8080")
///     .workers(4)
///     .serve(router)
///     .unwrap();
/// ```
pub struct Server {
    host_port: String,
    workers: usize,
    /// Phase 3.2 Option C: cap on pipelined requests batched per event-loop
    /// iteration.  0 = unlimited (default).
    max_pipeline_depth: u32,
    /// Maximum allowed request size (headers + body) in bytes.
    /// `None` means use the worker default (1 MiB / env override).
    max_request_size: Option<usize>,
    /// Optional callback invoked once per worker thread at startup.
    /// Used to initialise per-thread resources (e.g. database pools).
    worker_init: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(feature = "tls")]
    tls_config: Option<crate::tls::TlsServerConfig>,
}

impl Server {
    /// Bind to the given address. Defaults to one worker per logical CPU.
    pub fn bind(host_port: &str) -> Self {
        Self {
            host_port: host_port.to_string(),
            workers: num_cpus::get(),
            max_pipeline_depth: 0,
            max_request_size: None,
            worker_init: None,
            #[cfg(feature = "tls")]
            tls_config: None,
        }
    }

    /// Register a callback that runs once on every worker thread at startup.
    ///
    /// Ideal for initialising per-thread resources such as database pools:
    ///
    /// ```rust,ignore
    /// Server::bind("0.0.0.0:8080")
    ///     .with_worker_init(|| {
    ///         chopin_pg::init_pool("postgres://user:pass@localhost/mydb", 4)
    ///             .expect("pool init failed");
    ///     })
    ///     .serve(router)
    ///     .unwrap();
    /// ```
    pub fn with_worker_init(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.worker_init = Some(Arc::new(f));
        self
    }

    /// Internal: accept a pre-boxed init fn (used by `Chopin::serve`).
    pub(crate) fn with_worker_init_arc(mut self, f: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.worker_init = Some(f);
        self
    }

    /// Set the number of worker threads (defaults to `num_cpus::get()`).
    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    /// Limit the number of pipelined HTTP requests processed per event-loop
    /// iteration before forcing a write flush.
    ///
    /// Setting this improves fairness when clients send many back-to-back
    /// pipelined requests: after `depth` requests are batched and serialised,
    /// their responses are flushed and other connections get a turn.
    ///
    /// `0` (the default) means unlimited — all buffered pipelined requests are
    /// handled before yielding, which maximises throughput for pipeline-heavy
    /// clients.
    pub fn with_max_pipeline_depth(mut self, depth: u32) -> Self {
        self.max_pipeline_depth = depth;
        self
    }

    /// Override the maximum allowed request size (headers + body combined) in bytes.
    ///
    /// Requests that exceed this limit are rejected with `413 Content Too Large`.
    /// Defaults to 1 MiB (overridable via the `CHOPIN_MAX_REQUEST_SIZE` env var).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Server::bind("0.0.0.0:8080")
    ///     .with_max_request_size(10 * 1024 * 1024) // 10 MiB
    ///     .serve(router)
    ///     .unwrap();
    /// ```
    pub fn with_max_request_size(mut self, bytes: usize) -> Self {
        self.max_request_size = Some(bytes);
        self
    }

    /// Enable TLS using PEM certificate and key files.
    ///
    /// Requires the `tls` feature flag.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Server::bind("0.0.0.0:443")
    ///     .with_tls("cert.pem", "key.pem")
    ///     .unwrap()
    ///     .serve(router)
    ///     .unwrap();
    /// ```
    #[cfg(feature = "tls")]
    pub fn with_tls(mut self, cert_path: &str, key_path: &str) -> crate::error::ChopinResult<Self> {
        let cfg = crate::tls::TlsServerConfig::from_pem_files(cert_path, key_path)
            .map_err(|e| crate::error::ChopinError::Other(e))?;
        self.tls_config = Some(cfg);
        Ok(self)
    }

    /// Start the server with the provided router. Spawns one thread per worker,
    /// each pinned to a CPU core, and blocks until shutdown.
    pub fn serve(self, mut router: Router) -> crate::error::ChopinResult<()> {
        // Sort children at every trie level for binary-search matching.
        router.finalize();

        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let shutdown_signal = shutdown_flag.clone();
        ctrlc::set_handler(move || {
            tracing::info!("Shutdown signal received — draining connections");
            shutdown_signal.store(true, Ordering::Release);
        })
        .map_err(|e| ChopinError::Other(format!("Failed to set Ctrl-C handler: {e}")))?;

        let mut worker_metrics: Vec<Arc<crate::metrics::WorkerMetrics>> =
            Vec::with_capacity(self.workers);
        for _ in 0..self.workers {
            worker_metrics.push(Arc::new(crate::metrics::WorkerMetrics::new()));
        }

        // Register metrics globally so /metrics and /health handlers can read them.
        crate::metrics::register_global_metrics(Arc::new(worker_metrics.clone()));

        let Parts { host, port } = parse_host_port(&self.host_port)?;

        tracing::info!(
            address = %self.host_port,
            workers = self.workers,
            "Chopin server starting"
        );

        let mut handles: Vec<thread::JoinHandle<()>> = Vec::with_capacity(self.workers);
        for (i, metrics_worker) in worker_metrics.iter().enumerate().take(self.workers) {
            let core_id = core_ids.get(i % core_ids.len()).copied();
            let router_clone = router.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = metrics_worker.clone();

            let host_clone = host.clone();
            let port_clone = port;

            #[cfg(feature = "tls")]
            let tls_clone = self.tls_config.clone();

            let max_pipeline_depth_clone = self.max_pipeline_depth;
            let max_request_size_clone = self.max_request_size;
            let worker_init_clone = self.worker_init.clone();

            let handle = thread::Builder::new()
                .name(format!("chopin-worker-{}", i))
                .spawn(move || {
                    if let Some(id) = core_id {
                        core_affinity::set_for_current(id);
                    }

                    // Invoke the user-supplied per-thread init callback (e.g. db pool init).
                    if let Some(ref init_fn) = worker_init_clone {
                        init_fn();
                    }

                    tracing::debug!(worker_id = i, "Worker thread started");

                    // Create dedicated SO_REUSEPORT listener for this worker
                    match syscalls::create_listen_socket_reuseport(&host_clone, port_clone) {
                        Ok(listen_fd) => {
                            let mut worker =
                                Worker::new(i, router_clone, metrics_worker, listen_fd);
                            #[cfg(feature = "tls")]
                            if let Some(cfg) = tls_clone {
                                worker.set_tls_config(cfg);
                            }
                            if max_pipeline_depth_clone > 0 {
                                worker.set_max_pipeline_depth(max_pipeline_depth_clone);
                            }
                            if let Some(max_req) = max_request_size_clone {
                                worker.set_max_request_size(max_req);
                            }
                            if let Err(e) = worker.run(shutdown) {
                                tracing::error!(worker_id = i, error = %e, "Worker exited with error");
                            }
                            unsafe {
                                libc::close(listen_fd);
                            }
                        }
                        Err(e) => {
                            tracing::error!(worker_id = i, error = %e, "Failed to create listen socket");
                        }
                    }
                })
                .map_err(ChopinError::from)?;

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

        tracing::info!("All workers stopped — server shutdown complete");
        Ok(())
    }
}

struct Parts {
    host: String,
    port: u16,
}

fn parse_host_port(hp: &str) -> crate::error::ChopinResult<Parts> {
    // D.6: Support IPv6 bracket notation, e.g. "[::1]:8080"
    if let Some(rest) = hp.strip_prefix('[') {
        // IPv6 bracketed form: [host]:port
        let bracket_end = rest.find(']').ok_or_else(|| {
            crate::error::ChopinError::Other("Missing closing ']' in IPv6 address".to_string())
        })?;
        let host = rest[..bracket_end].to_string();
        let after = &rest[bracket_end + 1..];
        let port_str = after.strip_prefix(':').ok_or_else(|| {
            crate::error::ChopinError::Other("Missing port after IPv6 address".to_string())
        })?;
        let port = port_str
            .parse::<u16>()
            .map_err(|_| crate::error::ChopinError::Other("Invalid port number".to_string()))?;
        Ok(Parts { host, port })
    } else {
        // IPv4 / hostname: split on last colon
        let colon = hp.rfind(':').ok_or_else(|| {
            crate::error::ChopinError::Other("Missing port in address".to_string())
        })?;
        let host = hp[..colon].to_string();
        let port = hp[colon + 1..]
            .parse::<u16>()
            .map_err(|_| crate::error::ChopinError::Other("Invalid port number".to_string()))?;
        Ok(Parts { host, port })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ipv4() {
        let p = parse_host_port("0.0.0.0:8080").unwrap();
        assert_eq!(p.host, "0.0.0.0");
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn test_parse_ipv6_bracket() {
        let p = parse_host_port("[::1]:9090").unwrap();
        assert_eq!(p.host, "::1");
        assert_eq!(p.port, 9090);
    }

    #[test]
    fn test_parse_ipv6_full() {
        let p = parse_host_port("[::]:3000").unwrap();
        assert_eq!(p.host, "::");
        assert_eq!(p.port, 3000);
    }

    #[test]
    fn test_parse_localhost() {
        let p = parse_host_port("localhost:4000").unwrap();
        assert_eq!(p.host, "localhost");
        assert_eq!(p.port, 4000);
    }

    #[test]
    fn test_parse_missing_port() {
        assert!(parse_host_port("0.0.0.0").is_err());
    }
}
