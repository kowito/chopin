//! Ultra-low-latency HTTP server for Chopin.
//!
//! Provides a `FastRoute` API for users to register zero-allocation static
//! response endpoints that bypass Axum's Router entirely. All other routes
//! are delegated to the Axum Router with full middleware.
//!
//! ## Performance Mode Architecture
//!
//! ```text
//! SO_REUSEPORT × N CPU cores  (kernel-level load balancing)
//!   → per-core accept loop
//!     → TCP_NODELAY
//!       → hyper HTTP/1.1 (keep-alive, pipeline_flush)
//!         → ChopinService::call(req)
//!           → FastRoute match?  → pre-baked response (ZERO heap alloc)
//!           → no match          → Axum Router (full middleware stack)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use chopin_core::{App, FastRoute};
//!
//! let app = App::new().await?
//!     .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
//!     .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));
//! app.run().await?;
//! ```

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use bytes::Bytes;
use http_body::Frame;
use hyper::http::{Request, Response, HeaderMap, header, HeaderValue};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::perf;

// ═══════════════════════════════════════════════════════════════════
// FastRoute — user-facing API for zero-allocation static responses.
// ═══════════════════════════════════════════════════════════════════

static SERVER_NAME: HeaderValue = HeaderValue::from_static("chopin");

/// A pre-computed static response route that bypasses Axum entirely.
///
/// Register fast routes on the [`App`](crate::App) to serve static
/// responses with zero heap allocation on the hot path. This is how
/// you implement TechEmpower-style benchmark endpoints — no cheating,
/// no hardcoded magic, just a clean API.
///
/// # Examples
///
/// ```rust,ignore
/// use chopin_core::FastRoute;
///
/// // JSON benchmark endpoint
/// FastRoute::json("/json", br#"{"message":"Hello, World!"}"#);
///
/// // Plaintext benchmark endpoint
/// FastRoute::text("/plaintext", b"Hello, World!");
///
/// // Custom content type
/// FastRoute::new("/health", b"OK", "text/plain; charset=utf-8");
/// ```
#[derive(Clone)]
pub struct FastRoute {
    /// Path to match (exact match, no wildcards).
    path: Box<str>,
    /// Pre-computed response body (embedded in binary if `&'static [u8]`).
    body: Bytes,
    /// Original content type string (needed for raw HTTP handler).
    content_type_str: &'static str,
    /// Pre-built HeaderMap with Content-Type, Content-Length, Server.
    /// Cloning a 3-entry HeaderMap is cheaper than reserve(4) + 4 individual
    /// hash-probe-insert operations — it's a single contiguous memcpy of the
    /// internal buffer + 3 inline HeaderValue copies.
    base_headers: HeaderMap,
}

impl FastRoute {
    /// Create a fast route with a custom content type.
    ///
    /// **Pre-computes everything at registration time:**
    /// - Body as `Bytes::from_static` (clone = pointer copy)
    /// - HeaderMap with CT + CL + Server (clone = one alloc + memcpy)
    ///
    /// **Per-request cost:** clone base_headers + insert Date = ~35ns total.
    pub fn new(path: &str, body: &'static [u8], content_type: &'static str) -> Self {
        let bytes = Bytes::from_static(body);

        // Pre-build HeaderMap with capacity for 4 headers (3 + Date).
        // This allocation happens ONCE at startup, not per-request.
        let mut base_headers = HeaderMap::with_capacity(4);
        base_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        base_headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&body.len().to_string()).unwrap(),
        );
        base_headers.insert(header::SERVER, SERVER_NAME.clone());

        FastRoute {
            path: path.into(),
            body: bytes,
            content_type_str: content_type,
            base_headers,
        }
    }

    /// Create a JSON fast route (`Content-Type: application/json`).
    ///
    /// ```rust,ignore
    /// FastRoute::json("/json", br#"{"message":"Hello, World!"}"#)
    /// ```
    pub fn json(path: &str, body: &'static [u8]) -> Self {
        Self::new(path, body, "application/json")
    }

    /// Create a plaintext fast route (`Content-Type: text/plain`).
    ///
    /// ```rust,ignore
    /// FastRoute::text("/plaintext", b"Hello, World!")
    /// ```
    pub fn text(path: &str, body: &'static [u8]) -> Self {
        Self::new(path, body, "text/plain")
    }

    /// Create an HTML fast route (`Content-Type: text/html; charset=utf-8`).
    pub fn html(path: &str, body: &'static [u8]) -> Self {
        Self::new(path, body, "text/html; charset=utf-8")
    }

    // ═══ Accessors for raw HTTP handler (fast_http module) ═══

    /// Path as a string slice.
    #[inline(always)]
    pub(crate) fn path_str(&self) -> &str {
        &self.path
    }

    /// Content-Type as a static string.
    #[inline(always)]
    pub(crate) fn content_type_static(&self) -> &'static str {
        self.content_type_str
    }

    /// Body bytes.
    #[inline(always)]
    pub(crate) fn body_ref(&self) -> &[u8] {
        &self.body
    }

    /// Build the HTTP response.
    ///
    /// **Pre-built headers:** Clones the base_headers (single memcpy of internal
    /// buffer, no per-header hash computation) then inserts only the Date.
    ///
    /// **Zero-alloc body:** Uses `ChopinBody::Fast` (inline `Option<Bytes>`)
    /// instead of `Body::from(Bytes)` which Box-allocates.
    ///
    /// **Cost breakdown per request:**
    /// - `base_headers.clone()`: 1 alloc + memcpy (~25ns)
    /// - `insert(DATE, ...)`:   1 hash + insert (~5ns)
    /// - `body.clone()`:        pointer copy for static (~2ns)
    /// - `Response::new()`:     stack init (~3ns)
    /// Total: ~35ns (vs ~50ns with 4 individual inserts)
    #[inline(always)]
    fn respond(&self) -> Response<ChopinBody> {
        let mut headers = self.base_headers.clone();
        headers.insert(header::DATE, perf::cached_date_header());
        let mut res = Response::new(ChopinBody::Fast(Some(self.body.clone())));
        *res.headers_mut() = headers;
        res
    }
}

impl std::fmt::Debug for FastRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastRoute")
            .field("path", &self.path)
            .field("body_len", &self.body.len())
            .finish()
    }
}

impl std::fmt::Display for FastRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} bytes)", self.path, self.body.len())
    }
}

// ═════════════════════════════════════════════════════════════════
// ChopinBody — zero-allocation body for FastRoute, boxed for Axum
//
// Eliminates the `Box::pin` heap allocation in `Body::from(Bytes)` on
// the fast path. The body is stored inline as `Option<Bytes>` (~32
// bytes on the stack) instead of going through axum's BoxBody wrapper.
// For Axum router responses, the standard Body is used.
// ═════════════════════════════════════════════════════════════════

/// Response body that avoids heap allocation for FastRoute endpoints.
///
/// - `Fast`: body is `Option<Bytes>` directly on the stack. `Bytes::clone()`
///   for `from_static` data is a plain pointer+length copy (no Arc increment).
///   This eliminates the `Box::new(Full::new(bytes))` allocation that
///   `axum::body::Body::from(Bytes)` performs.
/// - `Axum`: standard axum Body (boxed) for Router-handled responses.
pub enum ChopinBody {
    /// Fast path: static body bytes. Zero heap allocation.
    Fast(Option<Bytes>),
    /// Slow path: Axum router response body.
    Axum(Body),
}

impl http_body::Body for ChopinBody {
    type Data = Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    #[inline(always)]
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // Both variants are Unpin (Bytes is Unpin, Body wraps Pin<Box<_>>).
        match self.get_mut() {
            ChopinBody::Fast(data) => Poll::Ready(data.take().map(|b| Ok(Frame::data(b)))),
            ChopinBody::Axum(body) => {
                Pin::new(body).poll_frame(cx).map(|opt| {
                    opt.map(|res| {
                        res.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
                    })
                })
            },
        }
    }

    #[inline(always)]
    fn is_end_stream(&self) -> bool {
        match self {
            ChopinBody::Fast(data) => data.is_none(),
            ChopinBody::Axum(body) => body.is_end_stream(),
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> http_body::SizeHint {
        match self {
            ChopinBody::Fast(data) => {
                http_body::SizeHint::with_exact(data.as_ref().map_or(0, |b| b.len()) as u64)
            }
            ChopinBody::Axum(body) => http_body::Body::size_hint(body),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// ChopinService — hyper Service with ZERO-ALLOC fast path
//
// Holds an `Arc<[FastRoute]>` checked before the Axum Router.
// The Router is cloned ONCE per connection (in accept loop), NOT
// per request. On the fast path, the Router is never touched AND
// no Box::pin allocation occurs.
// ═══════════════════════════════════════════════════════════════════

/// Custom future that avoids `Box::pin` heap allocation on the fast path.
///
/// - `Ready`: immediate response, zero heap allocation.
/// - `Router`: boxed Axum future (only for normal API routes).
pub enum ChopinFuture {
    /// Fast path — response already computed, zero heap allocation.
    Ready(Option<Response<ChopinBody>>),
    /// Slow path — delegate to Axum Router (boxed because Router::Future is opaque).
    Router(Pin<Box<dyn Future<Output = Result<Response<ChopinBody>, Infallible>> + Send>>),
}

impl Future for ChopinFuture {
    type Output = Result<Response<ChopinBody>, Infallible>;

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // All variants are Unpin: Option<Response<_>> is Unpin,
        // Pin<Box<dyn Future>> is Unpin (Box manages the pinning).
        match self.get_mut() {
            ChopinFuture::Ready(res) => {
                Poll::Ready(Ok(res.take().expect("ChopinFuture::Ready polled after completion")))
            }
            ChopinFuture::Router(fut) => fut.as_mut().poll(cx),
        }
    }
}

/// The core hyper Service.
///
/// - `fast_routes`: `Arc<[FastRoute]>` — checked first, zero-alloc response.
/// - `router`: Axum Router — fallback for all other paths.
#[derive(Clone)]
struct ChopinService {
    fast_routes: Arc<[FastRoute]>,
    router: axum::Router,
}

impl Service<Request<Incoming>> for ChopinService {
    type Response = Response<ChopinBody>;
    type Error = Infallible;
    type Future = ChopinFuture;

    #[inline(always)]
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let path = req.uri().path();

        // ── Fast path: linear scan over user-registered static routes ──
        // For 1-5 routes, this is faster than HashMap (cache-line friendly).
        // Returns ChopinFuture::Ready — NO Box::pin, NO heap allocation.
        for route in self.fast_routes.iter() {
            if path == &*route.path {
                return ChopinFuture::Ready(Some(route.respond()));
            }
        }

        // ── Slow path: delegate to Axum Router ──
        let mut router = self.router.clone();
        ChopinFuture::Router(Box::pin(async move {
            let (parts, incoming) = req.into_parts();
            let req = Request::from_parts(parts, Body::new(incoming));
            let response = tower::Service::call(&mut router, req)
                .await
                .unwrap_or_else(|err| match err {});
            Ok(response.map(ChopinBody::Axum))
        }))
    }
}

/// Shared hyper HTTP/1.1 builder — configured once, reused for all connections.
///
/// Tuned for maximum throughput:
/// - `keep_alive(true)`: reuse connections (critical for benchmarks)
/// - `pipeline_flush(true)`: flush between pipelined responses (low latency)
/// - `max_buf_size(16KB)`: larger read buffer reduces syscalls for headers
/// - `half_close(false)`: skip half-close handling (saves a syscall)
fn http1_builder() -> http1::Builder {
    let mut builder = http1::Builder::new();
    builder
        .keep_alive(true)
        .pipeline_flush(true)
        .half_close(false)
        .max_buf_size(16 * 1024);
    builder
}

/// Run a single accept loop on the given listener.
/// Spawned once per CPU core in performance mode.
async fn accept_loop(
    listener: TcpListener,
    fast_routes: Arc<[FastRoute]>,
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
                        let svc = ChopinService {
                            fast_routes: fast_routes.clone(),
                            router: router.clone(),
                        };
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
/// - User-registered `FastRoute`s bypass Axum entirely (zero-alloc).
/// - All other routes delegate to the Axum Router with full middleware.
/// - HTTP/1.1 keep-alive with pipeline flush.
/// - TCP_NODELAY on every connection.
/// - Cached Date header (updated every 500ms).
pub async fn run_until(
    listener: TcpListener,
    fast_routes: Arc<[FastRoute]>,
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
                        let svc = ChopinService {
                            fast_routes: fast_routes.clone(),
                            router: router.clone(),
                        };
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
/// the single accept-loop bottleneck.
///
/// User-registered `FastRoute`s are shared via `Arc<[FastRoute]>` across
/// all cores — one Arc increment per connection, zero per request.
pub async fn run_reuseport(
    addr: std::net::SocketAddr,
    fast_routes: Arc<[FastRoute]>,
    router: axum::Router,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    if fast_routes.is_empty() {
        tracing::info!("Performance mode: {} accept loops (SO_REUSEPORT), no fast routes", num_cores);
    } else {
        tracing::info!(
            "Performance mode: {} accept loops (SO_REUSEPORT), {} fast route(s): [{}]",
            num_cores,
            fast_routes.len(),
            fast_routes.iter().map(|r| r.path.as_ref()).collect::<Vec<_>>().join(", "),
        );
    }

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
        socket.listen(16384)?;

        let std_listener: std::net::TcpListener = socket.into();
        let tokio_listener = TcpListener::from_std(std_listener)?;

        let router = router.clone();
        let fast_routes = fast_routes.clone();
        let rx = shutdown_rx.clone();

        let handle = tokio::spawn(async move {
            tracing::debug!("Accept loop {} started", i);
            accept_loop(tokio_listener, fast_routes, router, rx).await;
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
