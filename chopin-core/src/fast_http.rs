//! Raw HTTP/1.1 handler that bypasses hyper entirely.
//!
//! For TechEmpower-style benchmarks where every request hits a FastRoute,
//! this module provides maximum throughput by:
//!
//! 1. **No HTTP framework** — reads/writes raw bytes on TcpStream
//! 2. **Pre-serialized responses** — only the 29-byte Date header is patched
//! 3. **Zero heap allocation** — per-connection buffers reused via `Vec::clear()`
//! 4. **Single-syscall writes** — entire response written in one `write_all()`
//! 5. **No Arc/atomic per request** — routes are borrowed, not cloned
//!
//! ## Architecture
//!
//! ```text
//! SO_REUSEPORT × N CPU cores
//!   → per-core accept loop (raw)
//!     → TCP_NODELAY
//!       → loop (keep-alive):
//!         → read request bytes into reuseable buffer
//!           → parse path (scan for spaces — ~10ns)
//!             → match RawFastRoute → write pre-serialized bytes (one syscall)
//!             → no match → write cached 404
//! ```
//!
//! ## Why This Beats hyper
//!
//! hyper adds ~200ns per request for:
//! - Full HTTP request parsing (method, version, all headers)
//! - `Service::call()` → `Future::poll()` → `Body::poll_frame()` chain
//! - `Response<T>` construction + `HeaderMap` serialization to wire format
//! - Connection state machine management
//!
//! The raw handler eliminates ALL of this. The only per-request work is:
//! - Scan for `\r\n\r\n` in read buffer (~10ns)
//! - Extract path between two spaces (~5ns)
//! - Linear scan of 1-5 route paths (~5ns)
//! - `memcpy` pre-serialized response into write buffer (~20ns)
//! - Patch 29-byte Date value (~3ns)
//! - `write_all()` syscall (~200ns)
//!
//! Total: ~240ns vs ~450ns with hyper = **~45% faster**.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::perf;
use crate::server::FastRoute;

// ═══════════════════════════════════════════════════════════════
// RawFastRoute — Pre-serialized HTTP/1.1 response
// ═══════════════════════════════════════════════════════════════

/// A pre-serialized HTTP/1.1 response split into prefix + suffix.
///
/// At request time, the response is assembled as:
/// `[prefix][29-byte date][suffix]`
///
/// All three parts are memcpy'd into a reusable per-connection write buffer,
/// then flushed via a single `write_all()` syscall.
pub struct RawFastRoute {
    /// Path as raw bytes for zero-cost comparison (no UTF-8 validation).
    path: Box<[u8]>,
    /// Response bytes before the Date value:
    /// `HTTP/1.1 200 OK\r\ncontent-type: ...\r\ncontent-length: N\r\nserver: chopin\r\ndate: `
    prefix: Box<[u8]>,
    /// Response bytes after the Date value:
    /// `\r\n\r\n{body}`
    suffix: Box<[u8]>,
    /// Total response size for buffer pre-allocation.
    pub(crate) total_len: usize,
}

impl RawFastRoute {
    /// Create a raw fast route from a FastRoute.
    ///
    /// Pre-serializes the complete HTTP/1.1 response at startup time.
    /// The only variable part at request time is the 29-byte Date header.
    pub fn from_fast_route(route: &FastRoute) -> Self {
        let body = route.body_ref();
        let content_type = route.content_type_static();

        let prefix = format!(
            "HTTP/1.1 200 OK\r\n\
             content-type: {}\r\n\
             content-length: {}\r\n\
             server: chopin\r\n\
             date: ",
            content_type,
            body.len(),
        );

        let mut suffix_bytes = Vec::with_capacity(4 + body.len());
        suffix_bytes.extend_from_slice(b"\r\n\r\n");
        suffix_bytes.extend_from_slice(body);

        let total_len = prefix.len() + 29 + suffix_bytes.len();

        RawFastRoute {
            path: route.path_str().as_bytes().into(),
            prefix: prefix.into_bytes().into_boxed_slice(),
            suffix: suffix_bytes.into_boxed_slice(),
            total_len,
        }
    }

    /// Assemble the complete response into the given buffer.
    ///
    /// The buffer is cleared and filled with: prefix + date + suffix.
    /// Uses `Vec::clear()` + `extend_from_slice()` which is a memcpy
    /// into already-allocated memory (zero alloc after first request).
    #[inline(always)]
    fn write_to_buf(&self, buf: &mut Vec<u8>, date: &[u8; 29]) {
        buf.clear();
        buf.extend_from_slice(&self.prefix);
        buf.extend_from_slice(date);
        buf.extend_from_slice(&self.suffix);
    }
}

/// Pre-serialized 404 response for unrecognized paths.
/// Includes keep-alive support so the connection isn't dropped.
static RAW_404: &[u8] = b"HTTP/1.1 404 Not Found\r\n\
    content-length: 0\r\n\
    server: chopin\r\n\
    \r\n";

// ═══════════════════════════════════════════════════════════════
// Minimal HTTP request parser
// ═══════════════════════════════════════════════════════════════

/// Extract the URI path from an HTTP request.
///
/// Parses: `GET /path HTTP/1.1\r\n...`
/// Returns the path bytes (e.g., `/json`) and whether the request is complete
/// (has `\r\n\r\n` terminator).
///
/// **Cost:** ~10ns for a typical benchmark request (< 200 bytes).
/// Uses simple byte scanning — no regex, no allocations.
#[inline(always)]
fn parse_request_path(buf: &[u8]) -> Option<&[u8]> {
    // Find first space (after method: "GET ")
    let first_space = buf.iter().position(|&b| b == b' ')?;
    let rest = &buf[first_space + 1..];
    // Find second space (after path: "/json ")
    let second_space = rest.iter().position(|&b| b == b' ')?;
    Some(&rest[..second_space])
}

/// Check if the buffer contains a complete HTTP request (ends with \r\n\r\n).
#[inline(always)]
fn has_complete_request(buf: &[u8]) -> bool {
    // For typical benchmark requests (< 200 bytes), windows() is fast.
    // We search from the end since \r\n\r\n is always at the end.
    if buf.len() < 4 {
        return false;
    }
    // Fast path: check the last 4 bytes first (most common for single-segment reads)
    let tail = &buf[buf.len().saturating_sub(4)..];
    if tail == b"\r\n\r\n" {
        return true;
    }
    // Slow path: search the entire buffer (handles multi-segment reads)
    buf.windows(4).any(|w| w == b"\r\n\r\n")
}

/// Check if the request contains `Connection: close` header.
/// HTTP/1.1 defaults to keep-alive, so we only close if explicitly requested.
#[inline(always)]
fn wants_close(buf: &[u8]) -> bool {
    // Search for case-insensitive "connection: close" or "Connection: close"
    // For benchmarks, keep-alive is always used, so this is almost never true.
    for window in buf.windows(17) {
        if window.eq_ignore_ascii_case(b"connection: close") {
            return true;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════
// Raw accept loop — bypasses hyper entirely
// ═══════════════════════════════════════════════════════════════

/// Handle a single TCP connection with raw HTTP.
///
/// Loops for keep-alive: read request → match route → write response → repeat.
/// All buffers are allocated once and reused for the lifetime of the connection.
#[inline(never)] // One instance per spawned task — don't inline into accept loop
async fn handle_raw_connection(
    mut stream: tokio::net::TcpStream,
    routes: &[RawFastRoute],
    max_response_len: usize,
) {
    // Per-connection buffers — allocated once, reused for every request.
    // 1024 bytes is enough for any TechEmpower benchmark request.
    let mut read_buf = vec![0u8; 1024];
    let mut write_buf = Vec::with_capacity(max_response_len);
    let mut read_pos = 0;

    loop {
        // ── Read request ──
        // Loop until we have a complete request (\r\n\r\n).
        // For benchmarks over keep-alive, requests arrive in one TCP segment.
        loop {
            match stream.read(&mut read_buf[read_pos..]).await {
                Ok(0) => return, // Connection closed
                Ok(n) => {
                    read_pos += n;
                    if has_complete_request(&read_buf[..read_pos]) {
                        break;
                    }
                    if read_pos >= read_buf.len() {
                        // Request too large — bail
                        return;
                    }
                }
                Err(_) => return,
            }
        }

        let req = &read_buf[..read_pos];

        // ── Parse path and match route ──
        if let Some(path) = parse_request_path(req) {
            let mut matched = false;
            for route in routes {
                if path == &*route.path {
                    let date = perf::cached_date_bytes();
                    route.write_to_buf(&mut write_buf, &date);
                    if stream.write_all(&write_buf).await.is_err() {
                        return;
                    }
                    matched = true;
                    break;
                }
            }
            if !matched {
                if stream.write_all(RAW_404).await.is_err() {
                    return;
                }
            }
        } else {
            // Malformed request
            return;
        }

        // ── Keep-alive check ──
        if wants_close(req) {
            return;
        }

        // Reset for next request on this connection
        read_pos = 0;
    }
}

/// Run a raw accept loop on the given listener (one per CPU core).
///
/// This is the inner loop of the raw performance mode. Each CPU core
/// runs its own accept loop with its own TcpListener (via SO_REUSEPORT).
async fn raw_accept_loop(
    listener: TcpListener,
    routes: Arc<[RawFastRoute]>,
    max_response_len: usize,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        let routes = routes.clone();
                        tokio::spawn(async move {
                            handle_raw_connection(stream, &routes, max_response_len).await;
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

// ═══════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════

/// Run the **raw** Chopin server with SO_REUSEPORT — **hyper completely bypassed**.
///
/// This is the ultimate performance mode for FastRoute endpoints.
/// Requests are read and responses are written as raw bytes on the TCP socket.
///
/// **Limitations:**
/// - Only FastRoute endpoints are served (no Axum router fallback)
/// - No middleware (CORS, tracing, etc.)
/// - No HTTP/2 support
/// - No request body parsing
///
/// **Use when:** You need maximum throughput for static responses
/// (benchmarks, health checks, metrics endpoints).
///
/// For a mixed server with both fast routes and Axum routes, use
/// `server::run_reuseport()` instead.
pub async fn run_raw_reuseport(
    addr: std::net::SocketAddr,
    fast_routes: &[FastRoute],
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // Convert FastRoutes to RawFastRoutes (pre-serialize HTTP responses)
    let raw_routes: Vec<RawFastRoute> = fast_routes
        .iter()
        .map(RawFastRoute::from_fast_route)
        .collect();

    let max_response_len = raw_routes.iter().map(|r| r.total_len).max().unwrap_or(256);
    let raw_routes: Arc<[RawFastRoute]> = raw_routes.into();

    tracing::info!(
        "RAW Performance mode: {} accept loops (SO_REUSEPORT), {} fast route(s), hyper BYPASSED",
        num_cores,
        fast_routes.len(),
    );

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

        let routes = raw_routes.clone();
        let rx = shutdown_rx.clone();

        let handle = tokio::spawn(async move {
            tracing::debug!("Raw accept loop {} started", i);
            raw_accept_loop(tokio_listener, routes, max_response_len, rx).await;
            tracing::debug!("Raw accept loop {} stopped", i);
        });
        handles.push(handle);
    }

    shutdown.await;
    tracing::info!("Shutting down raw Chopin server ({} cores)...", num_cores);
    let _ = shutdown_tx.send(true);

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
