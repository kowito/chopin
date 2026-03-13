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
        }
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

    /// Start the server, binding to `host_port` (e.g. `"0.0.0.0:8080"`).
    pub fn serve(self, host_port: &str) -> crate::error::ChopinResult<()> {
        let server = Server::bind(host_port);
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
}

impl Server {
    /// Bind to the given address. Defaults to one worker per logical CPU.
    pub fn bind(host_port: &str) -> Self {
        Self {
            host_port: host_port.to_string(),
            workers: num_cpus::get(),
        }
    }

    /// Set the number of worker threads (defaults to `num_cpus::get()`).
    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
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
            shutdown_signal.store(true, Ordering::Release);
        })
        .map_err(|e| ChopinError::Other(format!("Failed to set Ctrl-C handler: {e}")))?;

        let mut worker_metrics = Vec::with_capacity(self.workers);
        for _ in 0..self.workers {
            worker_metrics.push(Arc::new(crate::metrics::WorkerMetrics::new()));
        }

        let _metrics_clones = worker_metrics.clone();
        let _shutdown_metrics = shutdown_flag.clone();

        let Parts { host, port } = parse_host_port(&self.host_port)?;

        let mut handles: Vec<thread::JoinHandle<()>> = Vec::with_capacity(self.workers);
        for (i, metrics_worker) in worker_metrics.iter().enumerate().take(self.workers) {
            let core_id = core_ids.get(i % core_ids.len()).copied();
            let router_clone = router.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = metrics_worker.clone();

            let host_clone = host.clone();
            let port_clone = port;

            let handle = thread::Builder::new()
                .name(format!("chopin-worker-{}", i))
                .spawn(move || {
                    if let Some(id) = core_id {
                        core_affinity::set_for_current(id);
                    }

                    // Create dedicated SO_REUSEPORT listener for this worker
                    match syscalls::create_listen_socket_reuseport(&host_clone, port_clone) {
                        Ok(listen_fd) => {
                            let mut worker =
                                Worker::new(i, router_clone, metrics_worker, listen_fd);
                            if let Err(_e) = worker.run(shutdown) {
                                // Error suppressed in production
                            }
                            unsafe {
                                libc::close(listen_fd);
                            }
                        }
                        Err(_e) => {
                            // Error suppressed in production
                        }
                    }
                })
                .map_err(ChopinError::from)?;

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

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
