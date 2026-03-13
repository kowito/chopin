// src/worker.rs — unified worker: epoll/kqueue (default) + io_uring (Linux, feature = "io-uring")

#[cfg(all(target_os = "linux", feature = "io-uring"))]
use crate::conn;
use crate::conn::ConnState;
use crate::error::{ChopinError, ChopinResult};
use crate::http_date::format_http_date;
use crate::slab::ConnectionSlab;
use crate::syscalls;
#[cfg(all(target_os = "linux", feature = "io-uring"))]
#[allow(unused_imports)]
use crate::syscalls::uring::{
    ACCEPT_CONN_IDX, IORING_CQE_F_MORE, IORING_SETUP_COOP_TASKRUN, IORING_SETUP_SINGLE_ISSUER,
    IORING_SETUP_SQPOLL, OP_TYPE_ACCEPT, OP_TYPE_CLOSE, OP_TYPE_READ, OP_TYPE_SPLICE,
    OP_TYPE_WRITE, OP_TYPE_WRITEV, UringRing, decode_user_data, encode_user_data, io_uring_cqe,
    prep_accept_multishot, prep_close, prep_read, prep_splice, prep_write, prep_writev,
};
#[cfg(not(all(target_os = "linux", feature = "io-uring")))]
use crate::syscalls::{EPOLLIN, EPOLLOUT, Epoll, epoll_event};
use crate::timer::TimerWheel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::metrics::WorkerMetrics;
use crate::router::Router;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pre-baked Content-Type header lines for the two most common types.
const CT_TEXT_PLAIN: &[u8] = b"Content-Type: text/plain\r\n";
const CT_APP_JSON: &[u8] = b"Content-Type: application/json\r\n";

/// Pre-baked 200 OK response prefix: status line + Server header.
const STATUS_200_PREFIX: &[u8] = b"HTTP/1.1 200 OK\r\nServer: chopin\r\n";

/// Pre-baked 200 OK fast-paths: status + server + content-type in one memcpy.
const FAST_200_JSON: &[u8] =
    b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: application/json\r\n";
const FAST_200_TEXT: &[u8] = b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: text/plain\r\n";
const FAST_200_HTML: &[u8] =
    b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: text/html; charset=utf-8\r\n";

/// Format an HTTP status line into a fixed 40-byte buffer. Returns the slice length.
#[inline(always)]
fn status_line(status: u16, out: &mut [u8; 40]) -> usize {
    let (phrase, code_bytes): (&[u8], &[u8]) = match status {
        100 => (b"Continue", b"100"),
        101 => (b"Switching Protocols", b"101"),
        200 => (b"OK", b"200"),
        201 => (b"Created", b"201"),
        202 => (b"Accepted", b"202"),
        204 => (b"No Content", b"204"),
        206 => (b"Partial Content", b"206"),
        301 => (b"Moved Permanently", b"301"),
        302 => (b"Found", b"302"),
        304 => (b"Not Modified", b"304"),
        400 => (b"Bad Request", b"400"),
        401 => (b"Unauthorized", b"401"),
        403 => (b"Forbidden", b"403"),
        404 => (b"Not Found", b"404"),
        405 => (b"Method Not Allowed", b"405"),
        408 => (b"Request Timeout", b"408"),
        409 => (b"Conflict", b"409"),
        410 => (b"Gone", b"410"),
        413 => (b"Content Too Large", b"413"),
        415 => (b"Unsupported Media Type", b"415"),
        422 => (b"Unprocessable Entity", b"422"),
        429 => (b"Too Many Requests", b"429"),
        500 => (b"Internal Server Error", b"500"),
        501 => (b"Not Implemented", b"501"),
        502 => (b"Bad Gateway", b"502"),
        503 => (b"Service Unavailable", b"503"),
        504 => (b"Gateway Timeout", b"504"),
        _ => (b"Unknown", b"000"),
    };

    let prefix = b"HTTP/1.1 ";
    let mut i = 0;
    out[i..i + prefix.len()].copy_from_slice(prefix);
    i += prefix.len();
    out[i..i + 3].copy_from_slice(code_bytes);
    i += 3;
    out[i] = b' ';
    i += 1;
    out[i..i + phrase.len()].copy_from_slice(phrase);
    i += phrase.len();
    out[i] = b'\r';
    i += 1;
    out[i] = b'\n';
    i += 1;
    i
}

/// Maximum number of requests on a single keep-alive connection before closing.
/// Prevents a single long-lived connection from monopolising a slab slot forever.
const KEEPALIVE_MAX_REQUESTS: u32 = 10_000;

pub struct Worker {
    #[allow(dead_code)]
    id: usize,
    router: Router,
    metrics: Arc<WorkerMetrics>,
    listen_fd: i32, // Dedicated SO_REUSEPORT listener
    slab_capacity: usize,
    epoll_timeout_ms: i32,
}

impl Worker {
    pub fn new(id: usize, router: Router, metrics: Arc<WorkerMetrics>, listen_fd: i32) -> Self {
        let slab_capacity = std::env::var("CHOPIN_SLAB_CAPACITY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10_000);

        let epoll_timeout_ms = std::env::var("CHOPIN_EPOLL_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1000);

        Self {
            id,
            router,
            metrics,
            listen_fd,
            slab_capacity,
            epoll_timeout_ms,
        }
    }

    /// Dispatches to the io_uring event loop on Linux with the `io-uring` feature,
    /// or falls back to the epoll/kqueue event loop on all other platforms.
    pub fn run(&mut self, shutdown: Arc<AtomicBool>) -> ChopinResult<()> {
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            eprintln!("[chopin] worker-{} using io_uring event loop", self.id);
            return self.run_uring(shutdown);
        }
        #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
        self.run_epoll(shutdown)
    }

    #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
    #[allow(clippy::collapsible_if)]
    fn run_epoll(&mut self, shutdown: Arc<AtomicBool>) -> ChopinResult<()> {
        // 1. Setup epoll/kqueue instance
        let epoll = Epoll::new()?;

        // Register the listen fd
        let listen_token = u64::MAX;
        if let Err(_e) = epoll.add(self.listen_fd, listen_token, EPOLLIN) {
            return Ok(());
        }

        // 2. Initialize Slab Allocator
        // Default 10k = ~80 MB per worker (Conn is ~8 KB each).
        // Override via CHOPIN_SLAB_CAPACITY env var for heavy load.
        let mut slab = ConnectionSlab::new(self.slab_capacity);

        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024]; // Process up to 1024 events at once

        // Wait timeout in ms (0 = spin-poll mode for lowest latency, trades CPU).
        let mut timeout = self.epoll_timeout_ms;

        let mut now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ChopinError::ClockError)?
            .as_secs() as u32;
        let mut last_prune = now;
        let mut timer_wheel = TimerWheel::new(now);
        let mut iter_count: u32 = 0;
        let mut drain_started: u32 = 0; // timestamp when shutdown was first observed

        loop {
            let is_shutting_down = shutdown.load(Ordering::Acquire);
            if is_shutting_down && slab.is_empty() {
                break;
            }
            // D.3: Hard drain deadline — exit after 30s even if connections remain
            if is_shutting_down && drain_started > 0 && now.wrapping_sub(drain_started) > 30 {
                break;
            }
            iter_count = iter_count.wrapping_add(1);

            // Update time and prune every 1024 iterations
            #[allow(clippy::manual_is_multiple_of)]
            if iter_count % 1024 == 0 {
                now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::ClockError)?
                    .as_secs() as u32;

                if now - last_prune >= 1 {
                    self.prune_connections_wheel(&mut slab, &epoll, &mut timer_wheel, now);
                    last_prune = now;
                }
            }

            let n = match epoll.wait(&mut events, timeout) {
                Ok(n) => n,
                Err(_) => continue, // Interrupted likely
            };

            for event in &events[..n] {
                let token = event.u64;
                let is_read = (event.events & EPOLLIN as u32) != 0;
                let is_write = (event.events & EPOLLOUT as u32) != 0;

                if token == listen_token {
                    // Direct accept (SO_REUSEPORT)
                    if is_shutting_down {
                        continue;
                    }

                    loop {
                        match syscalls::accept_connection(self.listen_fd) {
                            Ok(Some(client_fd)) => {
                                // Explicitly set TCP_NODELAY on every accepted socket.
                                // Inheritance from the listener is not guaranteed on all
                                // platforms (notably macOS), so we set it unconditionally.
                                unsafe {
                                    let one: libc::c_int = 1;
                                    libc::setsockopt(
                                        client_fd,
                                        libc::IPPROTO_TCP,
                                        libc::TCP_NODELAY,
                                        &one as *const _ as *const libc::c_void,
                                        std::mem::size_of_val(&one) as libc::socklen_t,
                                    );
                                    // Disable delayed ACK on Linux for lower latency.
                                    #[cfg(target_os = "linux")]
                                    libc::setsockopt(
                                        client_fd,
                                        libc::IPPROTO_TCP,
                                        libc::TCP_QUICKACK,
                                        &one as *const _ as *const libc::c_void,
                                        std::mem::size_of_val(&one) as libc::socklen_t,
                                    );
                                }
                                // Add to slab
                                if let Ok(idx) = slab.allocate(client_fd) {
                                    // Register with epoll
                                    if let Err(_e) = epoll.add(client_fd, idx as u64, EPOLLIN) {
                                        slab.free(idx);
                                        unsafe {
                                            libc::close(client_fd);
                                        }
                                    } else {
                                        if let Some(conn) = slab.get_mut(idx) {
                                            conn.state = ConnState::Reading;
                                            conn.flags = crate::conn::CONN_KEEP_ALIVE;
                                            conn.last_active = now;
                                            conn.requests_served = 0;
                                            self.metrics.inc_conn();
                                            timer_wheel.insert(idx, now);
                                        }
                                    }
                                } else {
                                    // Out of capacity - backpressure
                                    unsafe {
                                        libc::close(client_fd);
                                    }
                                }
                            }
                            Ok(None) => break, // WouldBlock
                            Err(_) => break,
                        }
                    }
                } else {
                    // Regular connection event
                    let idx = token as usize;
                    if let Some(conn_ref) = slab.get(idx) {
                        let fd = conn_ref.fd;
                        let mut next_state = conn_ref.state;

                        if is_read {
                            if let Some(conn) = slab.get_mut(idx) {
                                let read_start = conn.read_len as usize;
                                if read_start < conn.read_buf.len() {
                                    match syscalls::read_nonblocking(
                                        fd,
                                        &mut conn.read_buf[read_start..],
                                    ) {
                                        Ok(0) => {
                                            // EOF - client closed connection (if no data read)
                                            if read_start == 0 {
                                                next_state = ConnState::Closing;
                                            } else {
                                                next_state = ConnState::Parsing;
                                            }
                                        }
                                        Ok(n) => {
                                            conn.read_len += n as u16;
                                            next_state = ConnState::Parsing;
                                        }
                                        Err(ChopinError::Io(ref e))
                                            if e.kind() == std::io::ErrorKind::WouldBlock =>
                                        {
                                            // Not ready, keep waiting
                                        }
                                        Err(_) => {
                                            next_state = ConnState::Closing;
                                        }
                                    }
                                } else {
                                    // Buffer too full, can't read more without blowing up
                                    next_state = ConnState::Closing;
                                }
                            }
                        }

                        // ── Pipeline: Parse → Handle → Serialize → Write ──
                        // Outer loop allows re-entry from write-complete when
                        // pipelined request data remains in read_buf.
                        'pipeline: loop {
                            // Inner loop: drain all complete requests from read_buf,
                            // serialising each response into write_buf.
                            // Track read_offset for deferred compaction (single memcpy at end).
                            let mut read_offset: usize = 0;
                            while next_state == ConnState::Parsing {
                                if let Some(conn) = slab.get_mut(idx) {
                                    let rl = conn.read_len as usize;
                                    if rl == 0 {
                                        next_state = ConnState::Reading;
                                        break;
                                    }

                                    // Headroom: stop pipelining if write_buf nearly full
                                    let wl = conn.write_len as usize;
                                    if wl + 512 > crate::conn::WRITE_BUF_SIZE {
                                        next_state = ConnState::Writing;
                                        break;
                                    }

                                    let buf = &mut conn.read_buf[read_offset..read_offset + rl];
                                    match crate::parser::parse_request(buf) {
                                        Ok((req, consumed)) => {
                                            let mut ctx = crate::http::Context {
                                                req,
                                                params: [("", ""); crate::http::MAX_PARAMS],
                                                param_count: 0,
                                            };

                                            let mut keep_alive =
                                                (conn.flags & crate::conn::CONN_KEEP_ALIVE) != 0;
                                            if is_shutting_down {
                                                keep_alive = false;
                                            } else if keep_alive {
                                                // Check for Connection: close header
                                                for i in 0..ctx.req.header_count as usize {
                                                    let (k, v) = ctx.req.headers[i];
                                                    if k.len() == 10
                                                        && k.as_bytes()[0] | 0x20 == b'c'
                                                        && k.eq_ignore_ascii_case("Connection")
                                                        && v.eq_ignore_ascii_case("close")
                                                    {
                                                        keep_alive = false;
                                                        break;
                                                    }
                                                }
                                                // Keep-alive enabled by default indefinitely;
                                                // clients or proxies can close if needed
                                            }

                                            self.metrics.inc_req();
                                            conn.requests_served += 1;

                                            // D.2: Cap keep-alive requests per connection
                                            if conn.requests_served >= KEEPALIVE_MAX_REQUESTS {
                                                keep_alive = false;
                                            }

                                            let response = match self
                                                .router
                                                .match_route(ctx.req.method, ctx.req.path)
                                            {
                                                Some((handler, params, param_count, composed)) => {
                                                    ctx.params = params;
                                                    ctx.param_count = param_count;
                                                    let handler_ptr = *handler;

                                                    #[cfg(feature = "catch-panic")]
                                                    let result = std::panic::catch_unwind(
                                                        std::panic::AssertUnwindSafe(|| {
                                                            if let Some(c) = composed {
                                                                (**c)(ctx)
                                                            } else {
                                                                handler_ptr(ctx)
                                                            }
                                                        }),
                                                    );
                                                    #[cfg(feature = "catch-panic")]
                                                    let response = match result {
                                                        Ok(r) => r,
                                                        Err(_) => {
                                                            crate::http::Response::server_error()
                                                        }
                                                    };

                                                    #[cfg(not(feature = "catch-panic"))]
                                                    let response = if let Some(c) = composed {
                                                        (**c)(ctx)
                                                    } else {
                                                        handler_ptr(ctx)
                                                    };

                                                    response
                                                }
                                                None => crate::http::Response::not_found(),
                                            };

                                            // ── Serialize response APPENDING to write_buf ──
                                            // ctx consumed → read_buf borrow released
                                            let wstart = conn.write_len as usize;
                                            let wbuf = &mut conn.write_buf[wstart..];
                                            let mut pos: usize = 0;
                                            let mut overflow = false;

                                            macro_rules! w {
                                                ($src:expr) => {
                                                    if !overflow {
                                                        let c = $src;
                                                        let end = pos + c.len();
                                                        if let Some(slice) = wbuf.get_mut(pos..end)
                                                        {
                                                            slice.copy_from_slice(c);
                                                            pos = end;
                                                        } else {
                                                            overflow = true;
                                                        }
                                                    }
                                                };
                                            }

                                            // Fast-path: 200 OK + known content-type → single memcpy
                                            // (status + server + content-type pre-baked together).
                                            let ct_written = if response.status == 200 {
                                                match response.content_type {
                                                    "application/json" => {
                                                        w!(FAST_200_JSON);
                                                        true
                                                    }
                                                    "text/plain" => {
                                                        w!(FAST_200_TEXT);
                                                        true
                                                    }
                                                    "text/html; charset=utf-8" => {
                                                        w!(FAST_200_HTML);
                                                        true
                                                    }
                                                    _ => {
                                                        w!(STATUS_200_PREFIX);
                                                        false
                                                    }
                                                }
                                            } else {
                                                let mut sl_buf = [0u8; 40];
                                                let sl_len =
                                                    status_line(response.status, &mut sl_buf);
                                                w!(&sl_buf[..sl_len]);
                                                w!(b"Server: chopin\r\n");
                                                false
                                            };

                                            // Date header: fresh timestamp per response — no caching.
                                            let mut date_buf = [0u8; 37];
                                            let response_now = SystemTime::now()
                                                .duration_since(UNIX_EPOCH)
                                                .map_err(|_| ChopinError::ClockError)?
                                                .as_secs()
                                                as u32;
                                            format_http_date(response_now, &mut date_buf);
                                            w!(&date_buf[..]);

                                            // Content-Type: skip if already baked into fast-path prefix
                                            if !ct_written {
                                                match response.content_type {
                                                    "text/plain" => w!(CT_TEXT_PLAIN),
                                                    "application/json" => w!(CT_APP_JSON),
                                                    ct => {
                                                        w!(b"Content-Type: ");
                                                        w!(ct.as_bytes());
                                                        w!(b"\r\n");
                                                    }
                                                }
                                            }

                                            let is_chunked = matches!(
                                                response.body,
                                                crate::http::Body::Stream(_)
                                            );

                                            if is_chunked {
                                                w!(b"Transfer-Encoding: chunked\r\n");
                                            } else {
                                                w!(b"Content-Length: ");
                                                let body_len = response.body.len();
                                                let mut itoa_buf = [0u8; 10];
                                                let itoa_len = {
                                                    let mut n = body_len;
                                                    if n == 0 {
                                                        itoa_buf[0] = b'0';
                                                        1
                                                    } else {
                                                        let mut i = 0;
                                                        while n > 0 {
                                                            itoa_buf[i] = b'0' + (n % 10) as u8;
                                                            n /= 10;
                                                            i += 1;
                                                        }
                                                        itoa_buf[..i].reverse();
                                                        i
                                                    }
                                                };
                                                w!(&itoa_buf[..itoa_len]);
                                                w!(b"\r\n");
                                            }

                                            if keep_alive {
                                                w!(b"Connection: keep-alive\r\n");
                                            } else {
                                                w!(b"Connection: close\r\n");
                                            }

                                            for header in response.headers.iter() {
                                                w!(header.name.as_bytes());
                                                w!(b": ");
                                                w!(header.value.as_str().as_bytes());
                                                w!(b"\r\n");
                                            }

                                            w!(b"\r\n");

                                            // Body (only when headers didn't overflow)
                                            if !overflow {
                                                match response.body {
                                                    crate::http::Body::Empty => {}
                                                    crate::http::Body::Static(b) => {
                                                        if wstart == 0
                                                            && pos + b.len()
                                                                <= crate::conn::WRITE_BUF_SIZE
                                                        {
                                                            // Zero-copy: store ptr for writev
                                                            conn.body_ptr = b.as_ptr() as usize;
                                                            conn.body_total = b.len() as u32;
                                                        } else {
                                                            // Body too large or pipelining:
                                                            // copy into write_buf (triggers overflow→500)
                                                            w!(b);
                                                        }
                                                    }
                                                    crate::http::Body::Bytes(b) => {
                                                        if wstart == 0
                                                            && pos + b.len()
                                                                <= crate::conn::WRITE_BUF_SIZE
                                                        {
                                                            // Zero-copy: move into pinned storage
                                                            let boxed = b.into_boxed_slice();
                                                            conn.body_ptr = boxed.as_ptr() as usize;
                                                            conn.body_total = boxed.len() as u32;
                                                            conn.body_owned = Some(boxed);
                                                        } else {
                                                            // Body too large or pipelining:
                                                            // copy into write_buf (triggers overflow→500)
                                                            w!(b.as_slice());
                                                        }
                                                    }
                                                    crate::http::Body::Stream(mut iter) => {
                                                        for chunk in iter.by_ref() {
                                                            let hex_len = {
                                                                let mut n = chunk.len();
                                                                let mut hex_buf = [0u8; 8];
                                                                let mut i = 0;
                                                                if n == 0 {
                                                                    hex_buf[0] = b'0';
                                                                    i = 1;
                                                                } else {
                                                                    while n > 0 {
                                                                        let d = (n % 16) as u8;
                                                                        hex_buf[i] = if d < 10 {
                                                                            b'0' + d
                                                                        } else {
                                                                            b'A' + d - 10
                                                                        };
                                                                        n /= 16;
                                                                        i += 1;
                                                                    }
                                                                    hex_buf[..i].reverse();
                                                                }
                                                                (hex_buf, i)
                                                            };
                                                            w!(&hex_len.0[..hex_len.1]);
                                                            w!(b"\r\n");
                                                            w!(chunk.as_slice());
                                                            w!(b"\r\n");
                                                        }
                                                        w!(b"0\r\n\r\n");
                                                    }
                                                    crate::http::Body::File {
                                                        mut fd,
                                                        offset,
                                                        len,
                                                    } => {
                                                        // Zero-copy path: don't write body to
                                                        // write_buf. Instead, store sendfile state
                                                        // on the connection. The file body will be
                                                        // sent via sendfile() after headers flush.
                                                        conn.sendfile_fd = fd.take();
                                                        conn.sendfile_offset = offset;
                                                        conn.sendfile_remaining = len;
                                                    }
                                                }
                                            }

                                            if overflow {
                                                if wstart > 0 {
                                                    // Previous responses queued — flush
                                                    // them first, re-parse this request after.
                                                    next_state = ConnState::Writing;
                                                    break;
                                                }
                                                // wstart==0 ⇒ wbuf aliases full write_buf
                                                // Format error response with dynamic Date header
                                                let mut pos_err = 0;
                                                let err_prefix =
                                                    b"HTTP/1.1 500 Internal Server Error\r\n";
                                                wbuf[pos_err..pos_err + err_prefix.len()]
                                                    .copy_from_slice(err_prefix);
                                                pos_err += err_prefix.len();

                                                let error_now = SystemTime::now()
                                                    .duration_since(UNIX_EPOCH)
                                                    .map_err(|_| ChopinError::ClockError)?
                                                    .as_secs()
                                                    as u32;
                                                let mut date_buf = [0u8; 37];
                                                let date_len =
                                                    format_http_date(error_now, &mut date_buf);
                                                wbuf[pos_err..pos_err + date_len]
                                                    .copy_from_slice(&date_buf[..date_len]);
                                                pos_err += date_len;

                                                let err_suffix = b"Content-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
                                                wbuf[pos_err..pos_err + err_suffix.len()]
                                                    .copy_from_slice(err_suffix);
                                                pos = pos_err + err_suffix.len();
                                                keep_alive = false;
                                            }

                                            // Done using wbuf — NLL releases the borrow.
                                            conn.write_len = (wstart + pos) as u16;

                                            // Deferred compaction: track offset, compact once at end
                                            read_offset += consumed;
                                            conn.read_len = (rl - consumed) as u16;

                                            // Sticky keep-alive flag
                                            if !keep_alive {
                                                conn.flags &= !crate::conn::CONN_KEEP_ALIVE;
                                            }

                                            // If Connection: close was seen, stop
                                            // pipelining and flush immediately.
                                            if !keep_alive {
                                                next_state = ConnState::Writing;
                                                break;
                                            }

                                            // If we deferred body for writev zero-copy,
                                            // flush before pipelining more responses to
                                            // preserve HTTP response ordering.
                                            if conn.body_ptr != 0 {
                                                next_state = ConnState::Writing;
                                                break;
                                            }

                                            // Continue inner loop → next pipelined request
                                        }
                                        Err(crate::parser::ParseError::Incomplete) => {
                                            // Not enough data for a complete request.
                                            // If we accumulated responses, flush them;
                                            // otherwise wait for more data.
                                            next_state = if conn.write_len > 0 {
                                                ConnState::Writing
                                            } else {
                                                ConnState::Reading
                                            };
                                            break;
                                        }
                                        Err(crate::parser::ParseError::TooLarge) => {
                                            // Send 413 and close
                                            let err_413 = b"HTTP/1.1 413 Content Too Large\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                                            let wstart = conn.write_len as usize;
                                            let end = wstart + err_413.len();
                                            if end <= crate::conn::WRITE_BUF_SIZE {
                                                conn.write_buf[wstart..end]
                                                    .copy_from_slice(err_413);
                                                conn.write_len = end as u16;
                                            }
                                            conn.flags &= !crate::conn::CONN_KEEP_ALIVE;
                                            next_state = ConnState::Writing;
                                            break;
                                        }
                                        Err(_) => {
                                            next_state = ConnState::Closing;
                                            break;
                                        }
                                    }
                                } else {
                                    break;
                                }
                            } // end inner while (pipeline parse loop)

                            // Deferred compaction: single memcpy after all pipelined parses
                            if read_offset > 0 {
                                if let Some(conn) = slab.get_mut(idx) {
                                    let remaining = conn.read_len as usize;
                                    if remaining > 0 {
                                        conn.read_buf
                                            .copy_within(read_offset..read_offset + remaining, 0);
                                    }
                                }
                            }

                            // If we serialised responses but the inner loop exited in
                            // Reading state (single keep-alive request — not pipelined,
                            // not Connection: close), we still need to flush the write buffer.
                            if next_state == ConnState::Reading {
                                if let Some(conn) = slab.get(idx) {
                                    if conn.write_len > 0 {
                                        next_state = ConnState::Writing;
                                    }
                                }
                            }

                            // ── Write ──
                            if next_state == ConnState::Writing || is_write {
                                if let Some(conn) = slab.get_mut(idx) {
                                    let write_total = conn.write_len as usize;
                                    let ws = conn.write_pos as usize;

                                    // Phase 1: headers + body — attempt writev when both ready
                                    if ws == 0 && conn.body_ptr != 0 && conn.body_sent == 0 {
                                        // First attempt: send headers + body in one writev
                                        let header_slice = &conn.write_buf[0..write_total];
                                        // SAFETY: body_ptr points to either 'static data
                                        // (Body::Static) or a Box<[u8]> stored in
                                        // conn.body_owned, live until body_clear().
                                        let body_slice = unsafe {
                                            std::slice::from_raw_parts(
                                                conn.body_ptr as *const u8,
                                                conn.body_total as usize,
                                            )
                                        };
                                        match syscalls::writev_nonblocking(
                                            fd,
                                            &[header_slice, body_slice],
                                        ) {
                                            Ok(n) if n > 0 => {
                                                self.metrics.add_bytes(n);
                                                if n >= write_total {
                                                    conn.write_pos = write_total as u16;
                                                    conn.body_sent = (n - write_total) as u32;
                                                } else {
                                                    conn.write_pos = n as u16;
                                                }
                                            }
                                            Ok(_) => {} // WouldBlock — wait for EPOLLOUT
                                            Err(_) => {
                                                conn.close_sendfile();
                                                conn.body_clear();
                                                next_state = ConnState::Closing;
                                            }
                                        }
                                    } else if ws < write_total {
                                        // Phase 1a: flush remaining headers only
                                        match syscalls::write_nonblocking(
                                            fd,
                                            &conn.write_buf[ws..write_total],
                                        ) {
                                            Ok(n) if n > 0 => {
                                                self.metrics.add_bytes(n);
                                                conn.write_pos += n as u16;
                                            }
                                            Ok(_) => {} // WouldBlock — wait for EPOLLOUT
                                            Err(_) => {
                                                conn.close_sendfile();
                                                conn.body_clear();
                                                next_state = ConnState::Closing;
                                            }
                                        }
                                    }

                                    // Phase 1b: flush remaining body bytes (after headers sent)
                                    if next_state != ConnState::Closing
                                        && conn.write_pos as usize >= write_total
                                        && conn.body_ptr != 0
                                        && conn.body_sent < conn.body_total
                                    {
                                        let body_remaining =
                                            (conn.body_total - conn.body_sent) as usize;
                                        let body_slice = unsafe {
                                            std::slice::from_raw_parts(
                                                (conn.body_ptr + conn.body_sent as usize)
                                                    as *const u8,
                                                body_remaining,
                                            )
                                        };
                                        match syscalls::write_nonblocking(fd, body_slice) {
                                            Ok(n) if n > 0 => {
                                                self.metrics.add_bytes(n);
                                                conn.body_sent += n as u32;
                                            }
                                            Ok(_) => {} // WouldBlock — wait for EPOLLOUT
                                            Err(_) => {
                                                conn.close_sendfile();
                                                conn.body_clear();
                                                next_state = ConnState::Closing;
                                            }
                                        }
                                    }

                                    // Phase 2: Zero-copy sendfile (after headers + body flushed)
                                    if next_state != ConnState::Closing
                                        && conn.write_pos as usize >= conn.write_len as usize
                                        && (conn.body_ptr == 0 || conn.body_sent >= conn.body_total)
                                        && conn.sendfile_remaining > 0
                                    {
                                        match syscalls::sendfile_nonblocking(
                                            fd,
                                            conn.sendfile_fd,
                                            &mut conn.sendfile_offset,
                                            conn.sendfile_remaining,
                                        ) {
                                            Ok(n) if n > 0 => {
                                                self.metrics.add_bytes(n);
                                                conn.sendfile_remaining -= n as u64;
                                            }
                                            Ok(_) => {} // WouldBlock — wait for EPOLLOUT
                                            Err(_) => {
                                                conn.close_sendfile();
                                                next_state = ConnState::Closing;
                                            }
                                        }
                                    }

                                    // Check if fully done (headers + body + sendfile all flushed)
                                    if next_state != ConnState::Closing
                                        && conn.write_pos as usize >= conn.write_len as usize
                                        && (conn.body_ptr == 0 || conn.body_sent >= conn.body_total)
                                        && conn.sendfile_remaining == 0
                                    {
                                        conn.close_sendfile();
                                        conn.body_clear();
                                        conn.write_len = 0;
                                        conn.write_pos = 0;
                                        let ka = (conn.flags & crate::conn::CONN_KEEP_ALIVE) != 0;
                                        if ka && !is_shutting_down {
                                            if conn.read_len > 0 {
                                                // More pipelined data to parse!
                                                next_state = ConnState::Parsing;
                                                conn.state = ConnState::Parsing;
                                                continue 'pipeline;
                                            } else {
                                                conn.state = ConnState::Reading;
                                                next_state = ConnState::Reading;
                                            }
                                        } else {
                                            conn.state = ConnState::Closing;
                                            next_state = ConnState::Closing;
                                        }
                                    }
                                }
                            }

                            break; // exit outer pipeline loop
                        } // end 'pipeline loop

                        // Only register EPOLLOUT if data remains after the immediate
                        // write attempt (partial write). This eliminates one epoll_ctl
                        // syscall for every request whose response fits in a single write.
                        if next_state == ConnState::Writing {
                            let _ = epoll.modify(fd, idx as u64, EPOLLIN | EPOLLOUT);
                            if let Some(conn) = slab.get_mut(idx) {
                                conn.flags |= crate::conn::CONN_EPOLLOUT;
                            }
                        } else if next_state != ConnState::Closing {
                            // Write completed — remove EPOLLOUT interest if it was
                            // previously registered (avoids spurious writable wakeups
                            // on idle keep-alive connections).
                            if let Some(conn) = slab.get_mut(idx) {
                                if (conn.flags & crate::conn::CONN_EPOLLOUT) != 0 {
                                    conn.flags &= !crate::conn::CONN_EPOLLOUT;
                                    let _ = epoll.modify(fd, idx as u64, EPOLLIN);
                                }
                            }
                        }

                        if next_state == ConnState::Closing {
                            if let Some(conn) = slab.get_mut(idx) {
                                conn.close_sendfile();
                                conn.body_clear();
                            }
                            epoll.delete(fd).ok();
                            unsafe {
                                libc::close(fd);
                            }
                            slab.free(idx);
                            self.metrics.dec_conn();
                        } else {
                            if let Some(conn) = slab.get_mut(idx) {
                                conn.last_active = now;
                            }
                        }
                    }
                }
            }
            if shutdown.load(Ordering::Acquire) {
                timeout = 100;
                // D.3: Record when shutdown started for drain deadline
                if drain_started == 0 {
                    drain_started = now;
                }
            }
        }

        for i in 0..slab.capacity() {
            if let Some(conn) = slab.get_mut(i) {
                if conn.state != ConnState::Free {
                    conn.close_sendfile();
                    conn.body_clear();
                    unsafe {
                        libc::close(conn.fd);
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
    fn prune_connections_wheel(
        &self,
        slab: &mut ConnectionSlab,
        epoll: &Epoll,
        wheel: &mut TimerWheel,
        now: u32,
    ) {
        const TIMEOUT: u32 = 30;
        if let Some(mut drain) = wheel.advance(now, TIMEOUT) {
            while let Some(indices) = drain.next_slot() {
                for idx in indices {
                    let (timed_out, fd, last_active) = {
                        if let Some(conn) = slab.get(idx) {
                            if conn.state == ConnState::Free {
                                continue; // Already freed
                            }
                            (
                                now.wrapping_sub(conn.last_active) > TIMEOUT,
                                conn.fd,
                                conn.last_active,
                            )
                        } else {
                            continue;
                        }
                    };
                    if timed_out {
                        if let Some(conn) = slab.get_mut(idx) {
                            conn.close_sendfile();
                            conn.body_clear();
                        }
                        epoll.delete(fd).ok();
                        unsafe {
                            libc::close(fd);
                        }
                        slab.free(idx);
                        self.metrics.dec_conn();
                    } else {
                        // Connection still alive — re-insert at its current slot
                        drain.reinsert(idx, last_active);
                    }
                }
            }
        }
    }

    // ── io_uring path ────────────────────────────────────────────────────────
    // All methods below are compiled only on Linux with the `io-uring` feature.
    // They implement a fully asynchronous completion-based event loop that
    // replaces the epoll readiness loop above.

    /// Submit a multi-shot accept SQE on the listen fd.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn submit_accept(&self, ring: &mut UringRing) {
        if let Some(sqe) = ring.get_sqe() {
            let ud = encode_user_data(ACCEPT_CONN_IDX as usize, OP_TYPE_ACCEPT);
            prep_accept_multishot(sqe, self.listen_fd, ud);
        }
    }

    /// Submit a read SQE for the given connection.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn submit_read(&self, ring: &mut UringRing, slab: &mut ConnectionSlab, idx: usize) {
        if let Some(conn) = slab.get_mut(idx) {
            if conn.pending_op != 0 {
                return; // Already has an in-flight op
            }
            let read_start = conn.read_len as usize;
            if read_start >= conn.read_buf.len() {
                return; // Buffer full
            }
            let buf_ptr = conn.read_buf[read_start..].as_mut_ptr();
            let buf_len = (conn.read_buf.len() - read_start) as u32;
            let ud = encode_user_data(idx, OP_TYPE_READ);
            if let Some(sqe) = ring.get_sqe() {
                prep_read(sqe, conn.fd, buf_ptr, buf_len, ud);
                conn.pending_op = OP_TYPE_READ;
            }
        }
    }

    /// Submit a write SQE for pending write_buf data.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn submit_write(&self, ring: &mut UringRing, slab: &mut ConnectionSlab, idx: usize) {
        if let Some(conn) = slab.get_mut(idx) {
            if conn.pending_op != 0 {
                return;
            }
            let ws = conn.write_pos as usize;
            let wt = conn.write_len as usize;
            if ws >= wt {
                return; // Nothing to write
            }
            let buf_ptr = conn.write_buf[ws..wt].as_ptr();
            let buf_len = (wt - ws) as u32;
            let ud = encode_user_data(idx, OP_TYPE_WRITE);
            if let Some(sqe) = ring.get_sqe() {
                prep_write(sqe, conn.fd, buf_ptr, buf_len, ud);
                conn.pending_op = OP_TYPE_WRITE;
            }
        }
    }

    /// Submit a writev SQE (headers + body scatter-gather).
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn submit_writev_headers_body(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
        iovecs: &[libc::iovec],
    ) {
        if let Some(conn) = slab.get_mut(idx) {
            if conn.pending_op != 0 {
                return;
            }
            let ud = encode_user_data(idx, OP_TYPE_WRITEV);
            if let Some(sqe) = ring.get_sqe() {
                prep_writev(sqe, conn.fd, iovecs.as_ptr(), iovecs.len() as u32, ud);
                conn.pending_op = OP_TYPE_WRITEV;
            }
        }
    }

    /// Submit a close SQE for the given fd.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn submit_close(&self, ring: &mut UringRing, fd: i32, idx: usize) {
        let ud = encode_user_data(idx, OP_TYPE_CLOSE);
        if let Some(sqe) = ring.get_sqe() {
            prep_close(sqe, fd, ud);
        }
    }

    /// Free slob slot and submit async close SQE.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[inline]
    fn close_connection(&self, ring: &mut UringRing, slab: &mut ConnectionSlab, idx: usize) {
        if let Some(conn) = slab.get_mut(idx) {
            conn.close_sendfile();
            conn.body_clear();
            let fd = conn.fd;
            conn.pending_op = 0;
            slab.free(idx);
            self.metrics.dec_conn();
            self.submit_close(ring, fd, idx);
        }
    }

    /// Dispatch a completed CQE to the appropriate handler.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn process_cqe(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        timer_wheel: &mut TimerWheel,
        cqe: io_uring_cqe,
        now: u32,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        let (conn_idx, op_type) = decode_user_data(cqe.user_data);
        match op_type {
            OP_TYPE_ACCEPT => {
                self.handle_accept(ring, slab, timer_wheel, cqe, now, is_shutting_down)?;
            }
            OP_TYPE_READ => {
                self.handle_read(
                    ring,
                    slab,
                    timer_wheel,
                    conn_idx,
                    cqe,
                    now,
                    is_shutting_down,
                )?;
            }
            OP_TYPE_WRITE | OP_TYPE_WRITEV => {
                self.handle_write(ring, slab, conn_idx, cqe, now, is_shutting_down)?;
            }
            OP_TYPE_SPLICE => {
                self.handle_splice(ring, slab, conn_idx, cqe, is_shutting_down)?;
            }
            OP_TYPE_CLOSE => { /* close completed — slab already freed */ }
            _ => {}
        }
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn handle_accept(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        timer_wheel: &mut TimerWheel,
        cqe: io_uring_cqe,
        now: u32,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if is_shutting_down {
            return Ok(());
        }
        if cqe.res < 0 {
            if (cqe.flags & IORING_CQE_F_MORE) == 0 {
                self.submit_accept(ring);
            }
            return Ok(());
        }
        let client_fd = cqe.res;
        unsafe {
            let flags = libc::fcntl(client_fd, libc::F_GETFL);
            libc::fcntl(client_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            let one: libc::c_int = 1;
            libc::setsockopt(
                client_fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
            libc::setsockopt(
                client_fd,
                libc::IPPROTO_TCP,
                libc::TCP_QUICKACK,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
        }
        if let Ok(idx) = slab.allocate(client_fd) {
            if let Some(c) = slab.get_mut(idx) {
                c.state = ConnState::Reading;
                c.flags = conn::CONN_KEEP_ALIVE;
                c.last_active = now;
                c.requests_served = 0;
                c.pending_op = 0;
                self.metrics.inc_conn();
                timer_wheel.insert(idx, now);
            }
            self.submit_read(ring, slab, idx);
        } else {
            self.submit_close(ring, client_fd, 0);
        }
        if (cqe.flags & IORING_CQE_F_MORE) == 0 {
            self.submit_accept(ring);
        }
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[allow(clippy::collapsible_if)]
    fn handle_read(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        timer_wheel: &mut TimerWheel,
        idx: usize,
        cqe: io_uring_cqe,
        now: u32,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if let Some(c) = slab.get_mut(idx) {
            c.pending_op = 0;
        }
        if cqe.res <= 0 {
            if let Some(c) = slab.get(idx) {
                if cqe.res == 0 && c.read_len > 0 {
                    self.pipeline_and_write(ring, slab, timer_wheel, idx, now, is_shutting_down)?;
                    return Ok(());
                }
            }
            self.close_connection(ring, slab, idx);
            return Ok(());
        }
        let bytes_read = cqe.res as usize;
        if let Some(c) = slab.get_mut(idx) {
            c.read_len += bytes_read as u16;
            c.last_active = now;
        }
        self.pipeline_and_write(ring, slab, timer_wheel, idx, now, is_shutting_down)?;
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[allow(clippy::collapsible_if)]
    fn handle_write(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
        cqe: io_uring_cqe,
        now: u32,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if let Some(c) = slab.get_mut(idx) {
            c.pending_op = 0;
        }
        if cqe.res < 0 {
            self.close_connection(ring, slab, idx);
            return Ok(());
        }
        let bytes_written = cqe.res as usize;
        self.metrics.add_bytes(bytes_written);
        if let Some(c) = slab.get_mut(idx) {
            c.last_active = now;
            let wt = c.write_len as usize;
            let ws = c.write_pos as usize;
            if ws < wt {
                if ws == 0 && c.body_ptr != 0 && c.body_sent == 0 {
                    if bytes_written >= wt {
                        c.write_pos = wt as u16;
                        c.body_sent = (bytes_written - wt) as u32;
                    } else {
                        c.write_pos = bytes_written as u16;
                    }
                } else {
                    c.write_pos += bytes_written as u16;
                }
            } else if c.body_ptr != 0 && c.body_sent < c.body_total {
                c.body_sent += bytes_written as u32;
            } else if c.sendfile_remaining > 0 {
                c.sendfile_remaining -= bytes_written as u64;
                c.sendfile_offset += bytes_written as u64;
            }
        }
        if let Some(c) = slab.get(idx) {
            let ws = c.write_pos as usize;
            let wt = c.write_len as usize;
            if ws < wt {
                self.submit_write(ring, slab, idx);
            } else if c.body_ptr != 0 && c.body_sent < c.body_total {
                if let Some(c) = slab.get_mut(idx) {
                    let remaining = (c.body_total - c.body_sent) as usize;
                    let body_slice_ptr = (c.body_ptr + c.body_sent as usize) as *const u8;
                    let ud = encode_user_data(idx, OP_TYPE_WRITE);
                    c.pending_op = OP_TYPE_WRITE;
                    if let Some(sqe) = ring.get_sqe() {
                        prep_write(sqe, c.fd, body_slice_ptr, remaining as u32, ud);
                    }
                }
            } else if c.sendfile_remaining > 0 {
                // A.5: Use io_uring splice for zero-copy file transfer
                if let Some(c) = slab.get_mut(idx) {
                    let chunk = std::cmp::min(c.sendfile_remaining, 1 << 20) as u32;
                    let ud = encode_user_data(idx, OP_TYPE_SPLICE);
                    c.pending_op = OP_TYPE_SPLICE;
                    if let Some(sqe) = ring.get_sqe() {
                        prep_splice(
                            sqe,
                            c.sendfile_fd,
                            c.sendfile_offset as i64,
                            c.fd,
                            -1,
                            chunk,
                            ud,
                        );
                    }
                }
            } else {
                self.complete_response(ring, slab, idx, is_shutting_down)?;
            }
        }
        Ok(())
    }

    /// Handle a completed splice CQE (A.5: zero-copy file transfer via io_uring).
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn handle_splice(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
        cqe: io_uring_cqe,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if let Some(c) = slab.get_mut(idx) {
            c.pending_op = 0;
        }
        if cqe.res <= 0 {
            // Splice failed or EOF — close sendfile and finish response
            if let Some(c) = slab.get_mut(idx) {
                c.close_sendfile();
            }
            self.complete_response(ring, slab, idx, is_shutting_down)?;
            return Ok(());
        }
        let bytes_spliced = cqe.res as u64;
        self.metrics.add_bytes(bytes_spliced as usize);
        if let Some(c) = slab.get_mut(idx) {
            c.sendfile_offset += bytes_spliced;
            c.sendfile_remaining -= bytes_spliced;
            if c.sendfile_remaining > 0 {
                // Submit next splice chunk
                let chunk = std::cmp::min(c.sendfile_remaining, 1 << 20) as u32;
                let ud = encode_user_data(idx, OP_TYPE_SPLICE);
                c.pending_op = OP_TYPE_SPLICE;
                if let Some(sqe) = ring.get_sqe() {
                    prep_splice(
                        sqe,
                        c.sendfile_fd,
                        c.sendfile_offset as i64,
                        c.fd,
                        -1,
                        chunk,
                        ud,
                    );
                }
                return Ok(());
            }
        }
        self.complete_response(ring, slab, idx, is_shutting_down)
    }

    /// Response fully sent: reset write state and either continue pipeline or wait.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn complete_response(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if let Some(c) = slab.get_mut(idx) {
            c.close_sendfile();
            c.body_clear();
            c.write_len = 0;
            c.write_pos = 0;
            c.pending_op = 0;
            let ka = (c.flags & conn::CONN_KEEP_ALIVE) != 0;
            if ka && !is_shutting_down {
                if c.read_len > 0 {
                    c.state = ConnState::Parsing;
                    drop(c);
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| ChopinError::ClockError)?
                        .as_secs() as u32;
                    self.pipeline_and_write(
                        ring,
                        slab,
                        &mut TimerWheel::new(now),
                        idx,
                        now,
                        is_shutting_down,
                    )?;
                } else {
                    c.state = ConnState::Reading;
                    drop(c);
                    self.submit_read(ring, slab, idx);
                }
            } else {
                drop(c);
                self.close_connection(ring, slab, idx);
            }
        }
        Ok(())
    }

    /// Core pipeline: parse all complete requests, serialize responses, submit write SQEs.
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[allow(clippy::collapsible_if)]
    fn pipeline_and_write(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        _timer_wheel: &mut TimerWheel,
        idx: usize,
        _now: u32,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        let mut read_offset: usize = 0;

        loop {
            // Headroom check
            let should_flush = if let Some(c) = slab.get_mut(idx) {
                let rl = c.read_len as usize;
                if rl == 0 {
                    c.state = ConnState::Reading;
                    drop(c);
                    self.submit_read(ring, slab, idx);
                    return Ok(());
                }
                c.write_len as usize + 512 > conn::WRITE_BUF_SIZE
            } else {
                return Ok(());
            };
            if should_flush {
                break;
            }

            // Parse a request
            let parse_result = if let Some(c) = slab.get_mut(idx) {
                let rl = c.read_len as usize;
                let buf = &mut c.read_buf[read_offset..read_offset + rl];
                match crate::parser::parse_request(buf) {
                    Ok((req, consumed)) => Some((req, consumed)),
                    Err(crate::parser::ParseError::Incomplete) => {
                        if c.write_len > 0 {
                            None // Flush
                        } else {
                            c.state = ConnState::Reading;
                            drop(c);
                            if read_offset > 0 {
                                if let Some(c) = slab.get_mut(idx) {
                                    let remaining = c.read_len as usize;
                                    if remaining > 0 {
                                        c.read_buf
                                            .copy_within(read_offset..read_offset + remaining, 0);
                                    }
                                }
                            }
                            self.submit_read(ring, slab, idx);
                            return Ok(());
                        }
                    }
                    Err(crate::parser::ParseError::TooLarge) => {
                        // Send 413 and close
                        let err_413 = b"HTTP/1.1 413 Content Too Large\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                        if let Some(c) = slab.get_mut(idx) {
                            let wstart = c.write_len as usize;
                            let end = wstart + err_413.len();
                            if end <= conn::WRITE_BUF_SIZE {
                                c.write_buf[wstart..end].copy_from_slice(err_413);
                                c.write_len = end as u16;
                            }
                            c.flags &= !conn::CONN_KEEP_ALIVE;
                        }
                        break; // will submit write below
                    }
                    Err(_) => {
                        self.close_connection(ring, slab, idx);
                        return Ok(());
                    }
                }
            } else {
                return Ok(());
            };

            let Some((req, consumed)) = parse_result else {
                break;
            };

            if let Some(c) = slab.get_mut(idx) {
                let mut ctx = crate::http::Context {
                    req,
                    params: [("", ""); crate::http::MAX_PARAMS],
                    param_count: 0,
                };
                let mut keep_alive = (c.flags & conn::CONN_KEEP_ALIVE) != 0;
                if is_shutting_down {
                    keep_alive = false;
                } else if keep_alive {
                    for i in 0..ctx.req.header_count as usize {
                        let (k, v) = ctx.req.headers[i];
                        if k.len() == 10
                            && k.as_bytes()[0] | 0x20 == b'c'
                            && k.eq_ignore_ascii_case("Connection")
                            && v.eq_ignore_ascii_case("close")
                        {
                            keep_alive = false;
                            break;
                        }
                    }
                }
                self.metrics.inc_req();
                c.requests_served += 1;

                // D.2: Cap keep-alive requests per connection
                if c.requests_served >= KEEPALIVE_MAX_REQUESTS {
                    keep_alive = false;
                }

                let response = match self.router.match_route(ctx.req.method, ctx.req.path) {
                    Some((handler, params, param_count, composed)) => {
                        ctx.params = params;
                        ctx.param_count = param_count;
                        let handler_ptr = *handler;
                        #[cfg(feature = "catch-panic")]
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            if let Some(co) = composed {
                                (**co)(ctx)
                            } else {
                                handler_ptr(ctx)
                            }
                        }));
                        #[cfg(feature = "catch-panic")]
                        let response = match result {
                            Ok(r) => r,
                            Err(_) => crate::http::Response::server_error(),
                        };
                        #[cfg(not(feature = "catch-panic"))]
                        let response = if let Some(co) = composed {
                            (**co)(ctx)
                        } else {
                            handler_ptr(ctx)
                        };
                        response
                    }
                    None => crate::http::Response::not_found(),
                };

                let wstart = c.write_len as usize;
                let wbuf = &mut c.write_buf[wstart..];
                let mut pos: usize = 0;
                let mut overflow = false;

                macro_rules! w {
                    ($src:expr) => {
                        if !overflow {
                            let s = $src;
                            let end = pos + s.len();
                            if let Some(sl) = wbuf.get_mut(pos..end) {
                                sl.copy_from_slice(s);
                                pos = end;
                            } else {
                                overflow = true;
                            }
                        }
                    };
                }

                let ct_written = if response.status == 200 {
                    match response.content_type {
                        "application/json" => {
                            w!(FAST_200_JSON);
                            true
                        }
                        "text/plain" => {
                            w!(FAST_200_TEXT);
                            true
                        }
                        "text/html; charset=utf-8" => {
                            w!(FAST_200_HTML);
                            true
                        }
                        _ => {
                            w!(STATUS_200_PREFIX);
                            false
                        }
                    }
                } else {
                    let mut sl_buf = [0u8; 40];
                    let sl_len = status_line(response.status, &mut sl_buf);
                    w!(&sl_buf[..sl_len]);
                    w!(b"Server: chopin\r\n");
                    false
                };

                let mut date_buf = [0u8; 37];
                let response_now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::ClockError)?
                    .as_secs() as u32;
                format_http_date(response_now, &mut date_buf);
                w!(&date_buf[..]);

                if !ct_written {
                    match response.content_type {
                        "text/plain" => w!(CT_TEXT_PLAIN),
                        "application/json" => w!(CT_APP_JSON),
                        ct => {
                            w!(b"Content-Type: ");
                            w!(ct.as_bytes());
                            w!(b"\r\n");
                        }
                    }
                }

                let is_chunked = matches!(response.body, crate::http::Body::Stream(_));
                if is_chunked {
                    w!(b"Transfer-Encoding: chunked\r\n");
                } else {
                    w!(b"Content-Length: ");
                    let body_len = response.body.len();
                    let mut itoa_buf = [0u8; 10];
                    let itoa_len = {
                        let mut n = body_len;
                        if n == 0 {
                            itoa_buf[0] = b'0';
                            1
                        } else {
                            let mut i = 0;
                            while n > 0 {
                                itoa_buf[i] = b'0' + (n % 10) as u8;
                                n /= 10;
                                i += 1;
                            }
                            itoa_buf[..i].reverse();
                            i
                        }
                    };
                    w!(&itoa_buf[..itoa_len]);
                    w!(b"\r\n");
                }

                if keep_alive {
                    w!(b"Connection: keep-alive\r\n");
                } else {
                    w!(b"Connection: close\r\n");
                }

                for header in response.headers.iter() {
                    w!(header.name.as_bytes());
                    w!(b": ");
                    w!(header.value.as_str().as_bytes());
                    w!(b"\r\n");
                }
                w!(b"\r\n");

                if !overflow {
                    match response.body {
                        crate::http::Body::Empty => {}
                        crate::http::Body::Static(b) => {
                            if wstart == 0 && pos + b.len() <= conn::WRITE_BUF_SIZE {
                                c.body_ptr = b.as_ptr() as usize;
                                c.body_total = b.len() as u32;
                            } else {
                                w!(b);
                            }
                        }
                        crate::http::Body::Bytes(b) => {
                            if wstart == 0 && pos + b.len() <= conn::WRITE_BUF_SIZE {
                                let boxed = b.into_boxed_slice();
                                c.body_ptr = boxed.as_ptr() as usize;
                                c.body_total = boxed.len() as u32;
                                c.body_owned = Some(boxed);
                            } else {
                                w!(b.as_slice());
                            }
                        }
                        crate::http::Body::Stream(mut iter) => {
                            for chunk in iter.by_ref() {
                                let hex_len = {
                                    let mut n = chunk.len();
                                    let mut hex_buf = [0u8; 8];
                                    let mut i = 0;
                                    if n == 0 {
                                        hex_buf[0] = b'0';
                                        i = 1;
                                    } else {
                                        while n > 0 {
                                            let d = (n % 16) as u8;
                                            hex_buf[i] =
                                                if d < 10 { b'0' + d } else { b'A' + d - 10 };
                                            n /= 16;
                                            i += 1;
                                        }
                                        hex_buf[..i].reverse();
                                    }
                                    (hex_buf, i)
                                };
                                w!(&hex_len.0[..hex_len.1]);
                                w!(b"\r\n");
                                w!(chunk.as_slice());
                                w!(b"\r\n");
                            }
                            w!(b"0\r\n\r\n");
                        }
                        crate::http::Body::File {
                            mut fd,
                            offset,
                            len,
                        } => {
                            c.sendfile_fd = fd.take();
                            c.sendfile_offset = offset;
                            c.sendfile_remaining = len;
                        }
                    }
                }

                if overflow {
                    if wstart > 0 {
                        break;
                    } // Flush queued responses first
                    let mut pos_err = 0;
                    let err_prefix = b"HTTP/1.1 500 Internal Server Error\r\n";
                    wbuf[pos_err..pos_err + err_prefix.len()].copy_from_slice(err_prefix);
                    pos_err += err_prefix.len();
                    let error_now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| ChopinError::ClockError)?
                        .as_secs() as u32;
                    let mut date_buf_err = [0u8; 37];
                    let date_len = format_http_date(error_now, &mut date_buf_err);
                    wbuf[pos_err..pos_err + date_len].copy_from_slice(&date_buf_err[..date_len]);
                    pos_err += date_len;
                    let err_suffix =
                        b"Content-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
                    wbuf[pos_err..pos_err + err_suffix.len()].copy_from_slice(err_suffix);
                    pos = pos_err + err_suffix.len();
                    keep_alive = false;
                }

                c.write_len = (wstart + pos) as u16;
                read_offset += consumed;
                c.read_len = (c.read_len as usize - consumed) as u16;

                if !keep_alive {
                    c.flags &= !conn::CONN_KEEP_ALIVE;
                    break;
                }
                if c.body_ptr != 0 {
                    break;
                } // Need writev, stop pipelining
            } else {
                return Ok(());
            }
        } // end loop

        // Deferred compaction
        if read_offset > 0 {
            if let Some(c) = slab.get_mut(idx) {
                let remaining = c.read_len as usize;
                if remaining > 0 {
                    c.read_buf
                        .copy_within(read_offset..read_offset + remaining, 0);
                }
            }
        }

        // Submit write SQE
        if let Some(c) = slab.get(idx) {
            let ws = c.write_pos as usize;
            let wt = c.write_len as usize;
            if ws == 0 && c.body_ptr != 0 && c.body_sent == 0 && wt > 0 {
                let header_slice = &c.write_buf[0..wt];
                let body_slice = unsafe {
                    std::slice::from_raw_parts(c.body_ptr as *const u8, c.body_total as usize)
                };
                let iovecs = [
                    libc::iovec {
                        iov_base: header_slice.as_ptr() as *mut libc::c_void,
                        iov_len: header_slice.len(),
                    },
                    libc::iovec {
                        iov_base: body_slice.as_ptr() as *mut libc::c_void,
                        iov_len: body_slice.len(),
                    },
                ];
                self.submit_writev_headers_body(ring, slab, idx, &iovecs);
            } else if wt > ws {
                self.submit_write(ring, slab, idx);
            } else if c.write_len == 0 {
                drop(c);
                self.submit_read(ring, slab, idx);
            }
        }
        Ok(())
    }

    /// Main io_uring event loop (Linux, feature = "io-uring").
    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[allow(clippy::collapsible_if)]
    fn run_uring(&mut self, shutdown: Arc<AtomicBool>) -> ChopinResult<()> {
        let mut setup_flags = IORING_SETUP_SINGLE_ISSUER | IORING_SETUP_COOP_TASKRUN;
        // A.3: Opt-in to SQPOLL mode via env var (kernel thread polls SQ, zero-syscall submission)
        if std::env::var("CHOPIN_SQPOLL").as_deref() == Ok("1") {
            setup_flags |= IORING_SETUP_SQPOLL;
            eprintln!("[chopin] worker-{} enabling SQPOLL mode", self.id);
        }
        let mut ring = UringRing::new(256, setup_flags)
            .map_err(|e| ChopinError::Other(format!("io_uring_setup failed: {e}")))?;

        let mut slab = ConnectionSlab::new(self.slab_capacity);

        // A.4: Register slab read/write buffers with io_uring for OP_READ_FIXED/OP_WRITE_FIXED.
        // This avoids per-I/O page-table walks by pinning the buffers in kernel memory.
        {
            use crate::conn::{READ_BUF_SIZE, WRITE_BUF_SIZE};
            let cap = slab.capacity();
            let mut iovecs = Vec::with_capacity(cap * 2);
            for i in 0..cap {
                if let Some(c) = slab.get_mut(i) {
                    iovecs.push(libc::iovec {
                        iov_base: c.read_buf.as_mut_ptr() as *mut libc::c_void,
                        iov_len: READ_BUF_SIZE,
                    });
                    iovecs.push(libc::iovec {
                        iov_base: c.write_buf.as_mut_ptr() as *mut libc::c_void,
                        iov_len: WRITE_BUF_SIZE,
                    });
                }
            }
            if let Err(e) = ring.register_buffers(&iovecs) {
                eprintln!(
                    "[chopin] worker-{} register_buffers failed (non-fatal): {e}",
                    self.id
                );
                // Fall back to normal read/write — non-fatal
            }
        }

        let mut now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ChopinError::ClockError)?
            .as_secs() as u32;
        let mut last_prune = now;
        let mut timer_wheel = TimerWheel::new(now);
        let mut iter_count: u32 = 0;

        self.submit_accept(&mut ring);
        ring.submit()?;

        loop {
            let is_shutting_down = shutdown.load(Ordering::Acquire);
            if is_shutting_down && slab.is_empty() {
                break;
            }
            iter_count = iter_count.wrapping_add(1);

            #[allow(clippy::manual_is_multiple_of)]
            if iter_count % 1024 == 0 {
                now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::ClockError)?
                    .as_secs() as u32;
                if now - last_prune >= 1 {
                    self.prune_connections_wheel_uring(&mut ring, &mut slab, &mut timer_wheel, now);
                    last_prune = now;
                }
            }

            ring.submit_and_wait(1)?;

            let mut cqe_count = 0u32;
            while let Some(cqe) = ring.peek_cqe() {
                ring.advance_cq(1);
                cqe_count += 1;
                self.process_cqe(
                    &mut ring,
                    &mut slab,
                    &mut timer_wheel,
                    cqe,
                    now,
                    is_shutting_down,
                )?;
                if cqe_count >= 64 {
                    ring.submit()?;
                    cqe_count = 0;
                }
            }
            if cqe_count > 0 {
                ring.submit()?;
            }
        }

        for i in 0..slab.capacity() {
            if let Some(c) = slab.get_mut(i) {
                if c.state != ConnState::Free {
                    c.close_sendfile();
                    c.body_clear();
                    unsafe {
                        libc::close(c.fd);
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn prune_connections_wheel_uring(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        wheel: &mut TimerWheel,
        now: u32,
    ) {
        const TIMEOUT: u32 = 30;
        if let Some(mut drain) = wheel.advance(now, TIMEOUT) {
            while let Some(indices) = drain.next_slot() {
                for idx in indices {
                    let (timed_out, last_active) = {
                        if let Some(c) = slab.get(idx) {
                            if c.state == ConnState::Free {
                                continue;
                            }
                            (now.wrapping_sub(c.last_active) > TIMEOUT, c.last_active)
                        } else {
                            continue;
                        }
                    };
                    if timed_out {
                        self.close_connection(ring, slab, idx);
                    } else {
                        drain.reinsert(idx, last_active);
                    }
                }
            }
        }
    }
}
