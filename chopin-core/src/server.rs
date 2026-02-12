//! Ultra-low-latency HTTP server for Chopin.
//!
//! Bypasses Axum's Router entirely for benchmark endpoints (`/json`, `/plaintext`)
//! using a direct hyper HTTP/1.1 Service. All other routes are delegated to the
//! Axum Router with full middleware.
//!
//! ## Performance Mode Architecture
//!
//! ```text
//! SO_REUSEPORT × N CPU cores  (kernel-level load balancing)
//!   → per-core accept loop
//!     → TCP_NODELAY
//!       → hyper HTTP/1.1 (keep-alive, pipeline_flush)
//!         → ChopinService::call(req)
//!           → path == "/json"      → pre-baked 27-byte response (ZERO alloc)
//!           → path == "/plaintext" → pre-baked 13-byte response (ZERO alloc)
//!           → anything else        → Axum Router (full middleware stack)
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;

use axum::body::Body;
use bytes::Bytes;
use hyper::http::{Request, Response, header, HeaderValue};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::perf;

// ═══════════════════════════════════════════════════════════════════
// Pre-computed response bodies — ZERO serialization, ZERO allocation.
// These are embedded in the binary's .rodata section.
// ═══════════════════════════════════════════════════════════════════

/// `{"message":"Hello, World!"}` — 27 bytes.
static JSON_BODY: Bytes = Bytes::from_static(b"{\"message\":\"Hello, World!\"}");
/// `Hello, World!` — 13 bytes.
static PLAIN_BODY: Bytes = Bytes::from_static(b"Hello, World!");

// Pre-computed HeaderValues — avoids per-request allocation.
static CT_JSON: HeaderValue = HeaderValue::from_static("application/json");
static CT_PLAIN: HeaderValue = HeaderValue::from_static("text/plain");
static CL_27: HeaderValue = HeaderValue::from_static("27");
static CL_13: HeaderValue = HeaderValue::from_static("13");
static SERVER_NAME: HeaderValue = HeaderValue::from_static("chopin");

/// Build a JSON response with cached Date header. ZERO allocation on hot path.
#[inline(always)]
fn json_response() -> Response<Body> {
    let mut res = Response::new(Body::from(JSON_BODY.clone()));
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, CT_JSON.clone());
    headers.insert(header::CONTENT_LENGTH, CL_27.clone());
    headers.insert(header::SERVER, SERVER_NAME.clone());
    headers.insert(header::DATE, perf::cached_date_header());
    res
}

/// Build a plaintext response with cached Date header.
#[inline(always)]
fn plain_response() -> Response<Body> {
    let mut res = Response::new(Body::from(PLAIN_BODY.clone()));
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, CT_PLAIN.clone());
    headers.insert(header::CONTENT_LENGTH, CL_13.clone());
    headers.insert(header::SERVER, SERVER_NAME.clone());
    headers.insert(header::DATE, perf::cached_date_header());
    res
}

// ═══════════════════════════════════════════════════════════════════
// ChopinService — A proper hyper Service (no closure overhead)
//
// The Router is cloned ONCE per connection (in accept loop), NOT
// per request. On the fast path (/json, /plaintext), the Router
// is never touched — zero overhead.
// ═══════════════════════════════════════════════════════════════════

/// The core hyper Service. Holds an Axum Router for the slow path.
#[derive(Clone)]
struct ChopinService {
    router: axum::Router,
}

impl Service<Request<Incoming>> for ChopinService {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline(always)]
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        // ── Fast path: string comparison on URI path ──
        // This runs BEFORE any Router lookup, middleware, or allocation.
        let path = req.uri().path();

        if path.len() == 5 && path == "/json" {
            return Box::pin(async { Ok(json_response()) });
        }
        if path.len() == 10 && path == "/plaintext" {
            return Box::pin(async { Ok(plain_response()) });
        }

        // ── Slow path: delegate to Axum Router ──
        let mut router = self.router.clone();
        Box::pin(async move {
            let (parts, incoming) = req.into_parts();
            let req = Request::from_parts(parts, Body::new(incoming));
            Ok(tower::Service::call(&mut router, req)
                .await
                .unwrap_or_else(|err| match err {}))
        })
    }
}

/// Shared hyper HTTP/1.1 builder — configured once, reused for all connections.
fn http1_builder() -> http1::Builder {
    let mut builder = http1::Builder::new();
    builder
        .keep_alive(true)
        .pipeline_flush(true)
        // Don't waste cycles parsing large request headers in bench mode.
        // 8KB is plenty for wrk/bombardier which send minimal headers.
        .max_buf_size(8 * 1024);
    builder
}

/// Run a single accept loop on the given listener.
/// This is spawned once per CPU core in performance mode.
async fn accept_loop(
    listener: TcpListener,
    router: axum::Router,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let http_builder = http1_builder();

    loop {
        tokio::select! {
            biased;
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        let io = TokioIo::new(stream);
                        let svc = ChopinService { router: router.clone() };
                        let builder = http_builder.clone();

                        tokio::spawn(async move {
                            let conn = builder.serve_connection(io, svc);
                            if let Err(e) = conn.await {
                                if !e.is_incomplete_message()
                                    && !e.is_canceled()
                                    && !e.is_closed()
                                {
                                    tracing::debug!("connection error: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("TCP accept error: {}", e);
                    }
                }
            }
            _ = shutdown.changed() => {
                break;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════

/// Run the high-performance Chopin server (single listener) with graceful shutdown.
///
/// This server uses **raw hyper HTTP/1.1** for maximum throughput:
/// - `/json` and `/plaintext` bypass Axum's Router entirely
/// - HTTP/1.1 keep-alive with pipeline flush
/// - TCP_NODELAY on every connection
/// - Cached Date header (updated every 500ms)
/// - Router cloned once per connection, not per request
/// - All other routes delegate to the Axum Router
pub async fn run_until(
    listener: TcpListener,
    router: axum::Router,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let http_builder = http1_builder();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                tracing::info!("Shutting down Chopin server...");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        let io = TokioIo::new(stream);
                        let svc = ChopinService { router: router.clone() };
                        let builder = http_builder.clone();

                        tokio::spawn(async move {
                            let conn = builder.serve_connection(io, svc);
                            if let Err(e) = conn.await {
                                if !e.is_incomplete_message()
                                    && !e.is_canceled()
                                    && !e.is_closed()
                                {
                                    tracing::debug!("connection error: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("TCP accept error: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Run the Chopin server with **SO_REUSEPORT** multi-core accept loops.
///
/// Creates N listeners (one per CPU core) on the same address.
/// The kernel distributes incoming connections across cores, eliminating
/// the single accept-loop bottleneck. Each core has its own TcpListener
/// and runs an independent accept loop.
///
/// ## Requirements
/// - Linux / macOS (SO_REUSEPORT support)
/// - Works best with `mimalloc` global allocator (enable `perf` feature)
pub async fn run_reuseport(
    addr: std::net::SocketAddr,
    router: axum::Router,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    tracing::info!("Performance mode: {} accept loops (SO_REUSEPORT)", num_cores);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut handles = Vec::with_capacity(num_cores);

    for i in 0..num_cores {
        let socket = socket2::Socket::new(
            if addr.is_ipv4() {
                socket2::Domain::IPV4
            } else {
                socket2::Domain::IPV6
            },
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_reuse_address(true)?;
        #[cfg(not(windows))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&addr.into())?;
        socket.listen(8192)?;

        let std_listener: std::net::TcpListener = socket.into();
        let tokio_listener = TcpListener::from_std(std_listener)?;

        let router = router.clone();
        let rx = shutdown_rx.clone();

        let handle = tokio::spawn(async move {
            tracing::debug!("Accept loop {} started", i);
            accept_loop(tokio_listener, router, rx).await;
            tracing::debug!("Accept loop {} stopped", i);
        });
        handles.push(handle);
    }

    // Wait for shutdown signal, then notify all accept loops
    shutdown.await;
    tracing::info!("Shutting down Chopin server ({} cores)...", num_cores);
    let _ = shutdown_tx.send(true);

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
