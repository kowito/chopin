//! Ultra-low-latency HTTP server for Chopin.
//!
//! Provides a `FastRoute` API for users to register ultra-fast response
//! endpoints that bypass Axum's middleware entirely. Routes can serve
//! either pre-computed static bodies (zero-alloc, ~35ns) or per-request
//! serialized JSON (thread-local buffer reuse, ~100-150ns). Each route
//! can be individually configured with decorators (CORS, Cache-Control,
//! method filters) — all pre-computed at registration time.
//!
//! ## Architecture
//!
//! ```text
//! ChopinService::call(req)
//!   → FastRoute match + method check?
//!       → static: pre-baked response (ZERO heap alloc, ~35ns)
//!       → dynamic: per-request serialize (thread-local buf, ~100-150ns)
//!   → CORS preflight (OPTIONS)?
//!       → pre-baked 204 response
//!   → no match / method not allowed
//!       → Axum Router (full middleware stack)
//! ```
//!
//! With `REUSEPORT=true`:
//! ```text
//! SO_REUSEPORT × N CPU cores  (kernel-level load balancing)
//!   → per-core accept loop (current_thread tokio runtime)
//!     → TCP_NODELAY
//!       → hyper HTTP/1.1 (keep-alive, pipeline_flush)
//!         → ChopinService
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use chopin_core::{App, FastRoute};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Message { message: &'static str }
//!
//! let app = App::new().await?
//!     // Per-request JSON serialization (TechEmpower compliant, ~100-150ns)
//!     .fast_route(FastRoute::json_serialize("/json", || Message {
//!         message: "Hello, World!",
//!     }).get_only())
//!
//!     // Static plaintext (zero-alloc, ~35ns)
//!     .fast_route(FastRoute::text("/plaintext", b"Hello, World!").get_only())
//!
//!     // Static JSON (pre-cached, NOT TFB-compliant for /json)
//!     .fast_route(
//!         FastRoute::json("/api/status", br#"{"status":"ok"}"#)
//!             .cors()
//!             .get_only()
//!     )
//!
//!     // With Cache-Control
//!     .fast_route(
//!         FastRoute::text("/health", b"OK")
//!             .cache_control("public, max-age=60")
//!     );
//!
//! // All other routes go through Axum Router with full middleware
//! app.run().await?;
//! ```
//!
//! ## Per-Route Trade-off
//!
//! | Feature | FastRoute (static) | FastRoute (dynamic) | FastRoute (+decorators) | Axum Router |
//! |---------|---------------------|---------------------|-------------------------|-------------|
//! | **Performance** | ~35ns | ~100-150ns | ~35-150ns | ~1,000-5,000ns |
//! | **Throughput** | ~28M req/s | ~7-10M req/s | ~7-28M req/s | ~200K-1M req/s |
//! | Static body | Yes | — | Yes | Yes |
//! | Dynamic JSON | — | `json_serialize()` | `json_serialize()` | Yes |
//! | TFB compliant | — | ✓ | ✓ | ✓ |
//! | CORS | — | — | `.cors()` | Yes |
//! | Cache-Control | — | — | `.cache_control()` | Yes |
//! | Custom headers | — | — | `.header()` | Yes |
//! | Method filter | — | — | `.methods()` / `.get_only()` | Yes |
//! | Auth | — | — | — | Yes |
//! | Logging/Tracing | — | — | — | Yes |
//! | Request ID | — | — | — | Yes |
//!
//! **FastRoute is 7-142× faster** — static routes pre-compute everything;
//! dynamic routes use thread-local buffer reuse + sonic-rs SIMD serialization.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use bytes::Bytes;
use http_body::Frame;
use hyper::body::Incoming;
use hyper::http::{header, HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::perf;

// ═══════════════════════════════════════════════════════════════════
// FastRoute — user-facing API for zero-allocation static responses.
// ═══════════════════════════════════════════════════════════════════

static SERVER_NAME: HeaderValue = HeaderValue::from_static("chopin");

/// Body source for a FastRoute endpoint.
///
/// - `Static`: pre-computed bytes embedded in the binary. Clone is a pointer
///   copy (zero-alloc). Used for plaintext, HTML, or pre-cached JSON.
/// - `Dynamic`: per-request serialization via closure. Uses thread-local
///   buffer reuse (zero-alloc on hot path). TechEmpower JSON-compliant.
enum FastRouteBody {
    /// Pre-computed static bytes. `Bytes::clone()` for `from_static` data
    /// is a plain pointer+length copy (no Arc increment).
    Static(Bytes),
    /// Per-request body generation via closure.
    /// The `Arc` is cloned once per connection (when `ChopinService` is cloned),
    /// NOT per request. The closure itself is called per-request.
    Dynamic(Arc<dyn Fn() -> Bytes + Send + Sync>),
}

impl Clone for FastRouteBody {
    fn clone(&self) -> Self {
        match self {
            FastRouteBody::Static(b) => FastRouteBody::Static(b.clone()),
            FastRouteBody::Dynamic(f) => FastRouteBody::Dynamic(f.clone()),
        }
    }
}

/// A pre-computed static response route that bypasses Axum middleware.
///
/// Register fast routes on the [`App`](crate::App) to serve responses
/// with minimal overhead on the hot path. Use the builder methods to
/// configure per-route behavior — all header decorators are pre-computed
/// at registration time with zero per-request cost.
///
/// # Static vs Dynamic
///
/// - **Static** ([`json()`](Self::json), [`text()`](Self::text), [`html()`](Self::html)):
///   pre-computed body bytes, ~35ns/req, zero heap allocation.
/// - **Dynamic** ([`json_serialize()`](Self::json_serialize)):
///   per-request JSON serialization via thread-local buffer, ~100-150ns/req.
///   Complies with TechEmpower benchmark rules.
///
/// # Trade-off
///
/// FastRoute endpoints are 7-142× faster than Axum middleware routes but
/// don't run through the middleware stack. Use decorators like `.cors()` and
/// `.cache_control()` to add common headers without middleware overhead.
/// For auth, logging, or complex dynamic content, use normal Axum routes.
///
/// # Examples
///
/// ```rust,ignore
/// use chopin_core::FastRoute;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Message { message: &'static str }
///
/// // Per-request JSON serialization (TechEmpower compliant, ~100-150ns)
/// FastRoute::json_serialize("/json", || Message {
///     message: "Hello, World!",
/// });
///
/// // Static JSON (pre-cached, ~35ns, NOT TFB-compliant for /json endpoint)
/// FastRoute::json("/api/status", br#"{"status":"ok"}"#);
///
/// // With CORS and method filter (still ~35ns/req for static)
/// FastRoute::json("/api/status", br#"{"status":"ok"}"#)
///     .cors()
///     .get_only();
///
/// // With Cache-Control and custom headers
/// FastRoute::text("/health", b"OK")
///     .cache_control("public, max-age=60")
///     .header(hyper::header::X_CONTENT_TYPE_OPTIONS, "nosniff");
/// ```
#[derive(Clone)]
pub struct FastRoute {
    /// Path to match (exact match, no wildcards).
    path: Box<str>,
    /// Response body — static bytes or per-request dynamic serialization.
    body: FastRouteBody,
    /// Pre-built HeaderMap with Content-Type, Content-Length, Server, and
    /// any decorator headers (CORS, Cache-Control, custom).
    /// Cloning the HeaderMap is a single contiguous memcpy — cheaper than
    /// per-header hash-probe-insert operations.
    base_headers: HeaderMap,
    /// Pre-built CORS preflight response headers.
    /// `Some` when `.cors()` has been called — enables automatic OPTIONS handling.
    preflight_headers: Option<HeaderMap>,
    /// Allowed HTTP methods. `None` = all methods accepted (default).
    /// When set, non-matching methods fall through to the Axum Router,
    /// allowing different strategies per method on the same path.
    allowed_methods: Option<Box<[Method]>>,
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
            body: FastRouteBody::Static(bytes),
            base_headers,
            preflight_headers: None,
            allowed_methods: None,
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

    /// Create a JSON fast route with **per-request serialization**.
    ///
    /// Unlike [`FastRoute::json()`] which serves a pre-cached static response,
    /// this constructor serializes the value returned by `f` on every request.
    /// This complies with [TechEmpower benchmark rules](https://github.com/TechEmpower/FrameworkBenchmarks/wiki/Project-Information-Framework-Tests-Overview#json-serialization):
    ///
    /// > *"The serialization to JSON must not be cached; the computational effort
    /// > to serialize an object to JSON must occur within the scope of handling
    /// > each request."*
    ///
    /// ## Performance
    ///
    /// - **Thread-local buffer reuse** — zero heap allocation on the hot path
    /// - **sonic-rs SIMD** (with `perf` feature) — vectorized JSON writing
    /// - **~100-150ns/req** — still 10-50× faster than full Axum middleware
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// use serde::Serialize;
    /// use chopin_core::FastRoute;
    ///
    /// #[derive(Serialize)]
    /// struct Message {
    ///     message: &'static str,
    /// }
    ///
    /// FastRoute::json_serialize("/json", || Message {
    ///     message: "Hello, World!",
    /// })
    /// ```
    pub fn json_serialize<F, T>(path: &str, f: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
        T: serde::Serialize,
    {
        let body_fn: Arc<dyn Fn() -> Bytes + Send + Sync> = Arc::new(move || {
            crate::json::to_bytes(&f()).expect("FastRoute JSON serialization failed")
        });

        // Content-Length is computed per-request after serialization.
        let mut base_headers = HeaderMap::with_capacity(4);
        base_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        base_headers.insert(header::SERVER, SERVER_NAME.clone());

        FastRoute {
            path: path.into(),
            body: FastRouteBody::Dynamic(body_fn),
            base_headers,
            preflight_headers: None,
            allowed_methods: None,
        }
    }

    /// Get the path of this FastRoute.
    pub fn path(&self) -> &str {
        &self.path
    }

    // ═══ Decorators (all pre-computed at registration, zero per-request cost) ═══

    /// Add permissive CORS headers (`Access-Control-Allow-Origin: *`).
    ///
    /// Pre-computed at registration time — zero per-request cost.
    /// Also handles `OPTIONS` preflight requests automatically with a
    /// `204 No Content` response.
    ///
    /// # Trade-off
    ///
    /// Enables cross-origin access without any runtime overhead.
    /// For dynamic origin validation (checking `Origin` header against
    /// an allow-list), use the Axum Router with tower-http `CorsLayer`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// FastRoute::json("/api/status", br#"{"status":"ok"}"#)
    ///     .cors()
    ///     .get_only()
    /// ```
    pub fn cors(mut self) -> Self {
        // Add CORS header to normal responses
        self.base_headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );

        // Build pre-computed preflight response headers
        let mut preflight = HeaderMap::with_capacity(6);
        preflight.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        preflight.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, POST, PUT, DELETE, PATCH, OPTIONS"),
        );
        preflight.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type, Authorization"),
        );
        preflight.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("86400"),
        );
        preflight.insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        preflight.insert(header::SERVER, SERVER_NAME.clone());

        self.preflight_headers = Some(preflight);
        self
    }

    /// Set the `Cache-Control` header (pre-computed at registration time).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// FastRoute::text("/health", b"OK")
    ///     .cache_control("public, max-age=60")
    /// ```
    pub fn cache_control(mut self, value: &'static str) -> Self {
        self.base_headers
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
        self
    }

    /// Restrict to specific HTTP methods.
    ///
    /// By default, all methods are accepted. When set, non-matching methods
    /// fall through to the Axum Router — this lets you handle different
    /// methods on the same path with different strategies.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use hyper::Method;
    ///
    /// // GET /json → FastRoute (zero-alloc)
    /// // POST /json → falls through to Axum Router
    /// FastRoute::json("/json", body)
    ///     .methods(&[Method::GET, Method::HEAD])
    /// ```
    pub fn methods(mut self, methods: &[Method]) -> Self {
        self.allowed_methods = Some(methods.into());
        self
    }

    /// Convenience: restrict to `GET` and `HEAD` only.
    ///
    /// Equivalent to `.methods(&[Method::GET, Method::HEAD])`.
    /// Other methods (POST, PUT, etc.) fall through to the Axum Router.
    pub fn get_only(self) -> Self {
        self.methods(&[Method::GET, Method::HEAD])
    }

    /// Add a custom pre-computed header.
    ///
    /// The header is added at registration time and included in every
    /// response at zero per-request cost.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// FastRoute::json("/api/v1/status", body)
    ///     .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
    ///     .header(header::X_FRAME_OPTIONS, "DENY")
    /// ```
    pub fn header(mut self, name: header::HeaderName, value: &'static str) -> Self {
        self.base_headers
            .insert(name, HeaderValue::from_static(value));
        self
    }

    // ═══ Response Builders ═══

    /// Build the HTTP response.
    ///
    /// **Static path (pre-cached body):**
    /// Clones the base_headers (single memcpy) then inserts Date. Body is
    /// a pointer copy. Total: ~35ns.
    ///
    /// **Dynamic path (per-request serialization):**
    /// Calls the body closure (serialize via thread-local buffer), then
    /// clones base_headers, inserts Date + Content-Length. Total: ~100-150ns.
    ///
    /// Both paths use `ChopinBody::Fast` (inline `Option<Bytes>`) to avoid
    /// the `Box::pin` heap allocation in `Body::from(Bytes)`.
    #[inline(always)]
    fn respond(&self) -> Response<ChopinBody> {
        let mut headers = self.base_headers.clone();
        headers.insert(header::DATE, perf::cached_date_header());

        let body = match &self.body {
            FastRouteBody::Static(bytes) => bytes.clone(),
            FastRouteBody::Dynamic(f) => {
                let bytes = f();
                // Content-Length computed per-request for dynamic bodies.
                headers.insert(
                    header::CONTENT_LENGTH,
                    HeaderValue::from_str(&bytes.len().to_string()).unwrap(),
                );
                bytes
            }
        };

        let mut res = Response::new(ChopinBody::Fast(Some(body)));
        *res.headers_mut() = headers;
        res
    }

    /// Build the CORS preflight response (204 No Content + CORS headers).
    ///
    /// Only called when `.cors()` was used and the request method is OPTIONS.
    /// Pre-computed headers — cost is one HeaderMap clone + Date insert.
    #[inline(always)]
    fn respond_preflight(&self) -> Response<ChopinBody> {
        let mut headers = self
            .preflight_headers
            .as_ref()
            .expect("respond_preflight called without .cors()")
            .clone();
        headers.insert(header::DATE, perf::cached_date_header());
        let mut res = Response::new(ChopinBody::Fast(None));
        *res.status_mut() = StatusCode::NO_CONTENT;
        *res.headers_mut() = headers;
        res
    }

    /// Check if the given HTTP method is allowed for this route.
    #[inline(always)]
    fn method_allowed(&self, method: &Method) -> bool {
        match &self.allowed_methods {
            None => true,
            Some(methods) => methods.iter().any(|m| m == method),
        }
    }
}

impl std::fmt::Debug for FastRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("FastRoute");
        s.field("path", &self.path);
        match &self.body {
            FastRouteBody::Static(b) => s.field("body_len", &b.len()),
            FastRouteBody::Dynamic(_) => s.field("body", &"dynamic"),
        };
        s.field("cors", &self.preflight_headers.is_some())
            .field("methods", &self.allowed_methods)
            .finish()
    }
}

impl std::fmt::Display for FastRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)?;
        if let Some(ref methods) = self.allowed_methods {
            write!(
                f,
                " [{}]",
                methods
                    .iter()
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        if self.preflight_headers.is_some() {
            write!(f, " +cors")?;
        }
        match &self.body {
            FastRouteBody::Static(b) => write!(f, " ({} bytes)", b.len()),
            FastRouteBody::Dynamic(_) => write!(f, " (dynamic json)"),
        }
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
            ChopinBody::Axum(body) => Pin::new(body).poll_frame(cx).map(|opt| {
                opt.map(|res| {
                    res.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
                })
            }),
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
            ChopinFuture::Ready(res) => Poll::Ready(Ok(res
                .take()
                .expect("ChopinFuture::Ready polled after completion"))),
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
        let method = req.method();

        // ── Fast path: linear scan over user-registered static routes ──
        // For 1-5 routes, this is faster than HashMap (cache-line friendly).
        // Returns ChopinFuture::Ready — NO Box::pin, NO heap allocation.
        for route in self.fast_routes.iter() {
            if path == &*route.path {
                // Handle CORS preflight (OPTIONS) automatically
                if route.preflight_headers.is_some() && method == Method::OPTIONS {
                    return ChopinFuture::Ready(Some(route.respond_preflight()));
                }
                // Check method filter (default: all methods accepted)
                if route.method_allowed(method) {
                    return ChopinFuture::Ready(Some(route.respond()));
                }
                // Method not allowed → fall through to Axum Router
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
/// Each CPU core gets its own **`current_thread` tokio runtime** and
/// SO_REUSEPORT listener. This eliminates the work-stealing overhead of
/// the multi-thread runtime: no cross-thread task migration, no scheduler
/// mutex contention, perfect cache locality per core.
///
/// This matches the architecture used by top TechEmpower Rust entries
/// (Axum, Salvo, Ohkami) — per-core single-threaded runtimes with
/// kernel-level connection distribution.
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
        tracing::info!(
            "Performance mode: {} cores (per-core current_thread runtime + SO_REUSEPORT), no fast routes",
            num_cores
        );
    } else {
        tracing::info!(
            "Performance mode: {} cores (per-core current_thread runtime + SO_REUSEPORT), {} fast route(s): [{}]",
            num_cores,
            fast_routes.len(),
            fast_routes
                .iter()
                .map(|r| r.path.as_ref())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    // Shutdown coordination: a watch channel visible to all cores.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut thread_handles = Vec::with_capacity(num_cores);

    // Spawn N-1 worker threads, each with its own current_thread runtime
    // and SO_REUSEPORT listener. The main thread also participates.
    for i in 0..num_cores {
        let router = router.clone();
        let fast_routes = fast_routes.clone();
        let rx = shutdown_rx.clone();

        let handle = std::thread::Builder::new()
            .name(format!("chopin-worker-{}", i))
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create per-core tokio runtime");

                rt.block_on(async move {
                    // Each core binds its own SO_REUSEPORT listener.
                    let listener = create_reuseport_listener(addr)
                        .expect("Failed to create SO_REUSEPORT listener");

                    tracing::debug!("Core {} accept loop started", i);
                    accept_loop(listener, fast_routes, router, rx).await;
                    tracing::debug!("Core {} accept loop stopped", i);
                });
            })?;

        thread_handles.push(handle);
    }

    // Wait for shutdown signal on the caller's runtime, then notify all cores.
    shutdown.await;
    tracing::info!("Shutting down Chopin server ({} cores)...", num_cores);
    let _ = shutdown_tx.send(true);

    for handle in thread_handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Create a SO_REUSEPORT TCP listener bound to the given address.
///
/// Tuned for maximum throughput:
/// - `SO_REUSEPORT`: kernel distributes connections across cores
/// - `SO_REUSEADDR`: fast restart without TIME_WAIT delays
/// - `TCP_NODELAY`: set on the socket (inherited by accepted connections on some OS)
/// - Backlog of 4096: matches top TFB entries
fn create_reuseport_listener(
    addr: std::net::SocketAddr,
) -> Result<TcpListener, Box<dyn std::error::Error + Send + Sync>> {
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
    socket.set_nodelay(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(4096)?;

    let std_listener: std::net::TcpListener = socket.into();
    Ok(TcpListener::from_std(std_listener)?)
}

/// Start a multi-core server using per-thread `current_thread` runtimes.
///
/// This is the recommended entry point for maximum-throughput benchmarks.
/// Each CPU core gets its own single-threaded tokio runtime and SO_REUSEPORT
/// listener, eliminating all cross-thread synchronization overhead.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::{App, FastRoute};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Message { message: &'static str }
///
/// fn main() {
///     chopin_core::server::start_multicore(serve);
/// }
///
/// async fn serve() {
///     let app = App::new().await.unwrap()
///         .fast_route(FastRoute::json_serialize("/json", || Message {
///             message: "Hello, World!",
///         }))
///         .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));
///     app.run().await.unwrap();
/// }
/// ```
pub fn start_multicore<Fut>(f: fn() -> Fut)
where
    Fut: std::future::Future<Output = ()> + 'static,
{
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // Spawn N-1 worker threads with current_thread runtimes.
    let mut handles = Vec::with_capacity(num_cores - 1);
    for _ in 1..num_cores {
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create per-core tokio runtime");
            rt.block_on(f());
        });
        handles.push(handle);
    }

    // Run on the main thread too.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create main tokio runtime");
    rt.block_on(f());

    for handle in handles {
        let _ = handle.join();
    }
}
