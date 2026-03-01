// src/worker.rs
use crate::conn::ConnState;
use crate::error::{ChopinError, ChopinResult};
use crate::slab::ConnectionSlab;
use crate::syscalls::{self, EPOLLIN, EPOLLOUT, Epoll, epoll_event};
use crate::timer::TimerWheel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::metrics::WorkerMetrics;
use crate::router::Router;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pre-baked status line for 200 OK (the overwhelming majority in TFB).
const STATUS_200_OK: &[u8] = b"HTTP/1.1 200 OK\r\n";

/// Format an HTTP status line into a fixed 40-byte buffer. Returns the slice length.
#[inline]
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

/// Format the current time as an HTTP-date (RFC 7231 §7.1.1.2) directly
/// into a 29-byte stack buffer with **zero heap allocation**.
///
/// Output: `"Thu, 01 Jan 1970 00:00:00 GMT"` (exactly 29 bytes, always).
///
/// Uses the Howard Hinnant civil_from_days algorithm for calendar conversion.
#[inline]
fn format_http_date(buf: &mut [u8; 29], ts: u64) {
    // Day-of-week names (epoch = Thursday, index 0)
    static WDAY: [[u8; 3]; 7] = [
        *b"Thu", *b"Fri", *b"Sat", *b"Sun", *b"Mon", *b"Tue", *b"Wed",
    ];
    static MON: [[u8; 3]; 12] = [
        *b"Jan", *b"Feb", *b"Mar", *b"Apr", *b"May", *b"Jun", *b"Jul", *b"Aug", *b"Sep", *b"Oct",
        *b"Nov", *b"Dec",
    ];

    let total_days = (ts / 86400) as u32;
    let wday = (total_days % 7) as usize;

    let tod = (ts % 86400) as u32;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;

    // Civil date from days since epoch (Hinnant algorithm)
    let z = total_days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    // "Thu, 01 Jan 1970 00:00:00 GMT"  (29 bytes)
    buf[0..3].copy_from_slice(&WDAY[wday]);
    buf[3] = b',';
    buf[4] = b' ';
    buf[5] = b'0' + (d / 10) as u8;
    buf[6] = b'0' + (d % 10) as u8;
    buf[7] = b' ';
    buf[8..11].copy_from_slice(&MON[(m - 1) as usize]);
    buf[11] = b' ';
    buf[12] = b'0' + (y / 1000 % 10) as u8;
    buf[13] = b'0' + (y / 100 % 10) as u8;
    buf[14] = b'0' + (y / 10 % 10) as u8;
    buf[15] = b'0' + (y % 10) as u8;
    buf[16] = b' ';
    buf[17] = b'0' + (hh / 10) as u8;
    buf[18] = b'0' + (hh % 10) as u8;
    buf[19] = b':';
    buf[20] = b'0' + (mm / 10) as u8;
    buf[21] = b'0' + (mm % 10) as u8;
    buf[22] = b':';
    buf[23] = b'0' + (ss / 10) as u8;
    buf[24] = b'0' + (ss % 10) as u8;
    buf[25] = b' ';
    buf[26..29].copy_from_slice(b"GMT");
}

pub struct Worker {
    #[allow(dead_code)]
    id: usize,
    router: Router,
    metrics: Arc<WorkerMetrics>,
    listen_fd: i32, // Dedicated SO_REUSEPORT listener
}

impl Worker {
    pub fn new(id: usize, router: Router, metrics: Arc<WorkerMetrics>, listen_fd: i32) -> Self {
        Self {
            id,
            router,
            metrics,
            listen_fd,
        }
    }

    pub fn run(&mut self, shutdown: Arc<AtomicBool>) -> ChopinResult<()> {
        // 1. Setup epoll/kqueue instance
        let epoll = Epoll::new()?;

        // Register the listen fd
        let listen_token = u64::MAX;
        if let Err(_e) = epoll.add(self.listen_fd, listen_token, EPOLLIN) {
            return Ok(());
        }

        // 2. Initialize Slab Allocator
        // 10k = ~80 MB per worker (Conn is ~8 KB each). Override via CLI / config for heavy load.
        let mut slab = ConnectionSlab::new(10_000);

        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024]; // Process up to 1024 events at once

        // Wait timeout in ms.
        let mut timeout = 1000;

        let mut now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ChopinError::Other("Clock went backwards".to_string()))?
            .as_secs() as u32;
        let mut last_prune = now;
        let mut timer_wheel = TimerWheel::new(now);
        let mut iter_count: u32 = 0;

        // Cached HTTP-date: refreshed once per second, shared across all responses
        let mut cached_date = [0u8; 29];
        format_http_date(&mut cached_date, now as u64);
        let mut cached_date_sec: u32 = now;

        loop {
            let is_shutting_down = shutdown.load(Ordering::Acquire);
            if is_shutting_down && slab.is_empty() {
                break;
            }
            iter_count = iter_count.wrapping_add(1);

            // Update time and prune every 1024 iterations
            if iter_count.is_multiple_of(1024) {
                now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::Other("Clock went backwards".to_string()))?
                    .as_secs() as u32;

                if now - last_prune >= 1 {
                    self.prune_connections_wheel(&mut slab, &epoll, &mut timer_wheel, now);
                    last_prune = now;
                }

                // Refresh cached HTTP-date once per second
                if now != cached_date_sec {
                    format_http_date(&mut cached_date, now as u64);
                    cached_date_sec = now;
                }
            }

            let n = match epoll.wait(&mut events, timeout) {
                Ok(n) => n,
                Err(_) => continue, // Interrupted likely
            };

            for event in events.iter().take(n) {
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
                                // TCP_NODELAY + SO_NOSIGPIPE are inherited from the listener
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

                        if is_read && let Some(conn) = slab.get_mut(idx) {
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
                                                if conn.requests_served >= 10_000 {
                                                    keep_alive = false;
                                                }
                                            }

                                            self.metrics.inc_req();
                                            conn.requests_served += 1;

                                            let response = match self
                                                .router
                                                .match_route(ctx.req.method, ctx.req.path)
                                            {
                                                Some((
                                                    handler,
                                                    params,
                                                    param_count,
                                                    route_middleware,
                                                    route_mw_count,
                                                )) => {
                                                    ctx.params = params;
                                                    ctx.param_count = param_count;
                                                    let handler_ptr = *handler;
                                                    let global_mw = &self.router.global_middleware;
                                                    let total_mw = global_mw.len() + route_mw_count;

                                                    #[cfg(feature = "catch-panic")]
                                                    let result = std::panic::catch_unwind(
                                                        std::panic::AssertUnwindSafe(|| {
                                                            if total_mw == 0 {
                                                                handler_ptr(ctx)
                                                            } else {
                                                                let mut current_handler: crate::router::BoxedHandler =
                                                                    std::sync::Arc::new(
                                                                        handler_ptr,
                                                                    );
                                                                for i in (0..route_mw_count).rev() {
                                                                    if let Some(mw) =
                                                                        route_middleware[i]
                                                                    {
                                                                        let next = current_handler;
                                                                        current_handler =
                                                                            std::sync::Arc::new(
                                                                                move |ctx| {
                                                                                    mw(
                                                                                        ctx,
                                                                                        next.clone(
                                                                                        ),
                                                                                    )
                                                                                },
                                                                            );
                                                                    }
                                                                }
                                                                for mw in global_mw.iter().rev() {
                                                                    let mw = *mw;
                                                                    let next = current_handler;
                                                                    current_handler =
                                                                        std::sync::Arc::new(
                                                                            move |ctx| {
                                                                                mw(
                                                                                    ctx,
                                                                                    next.clone(),
                                                                                )
                                                                            },
                                                                        );
                                                                }
                                                                current_handler(ctx)
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
                                                    let response = if total_mw == 0 {
                                                        handler_ptr(ctx)
                                                    } else {
                                                        let mut current_handler: crate::router::BoxedHandler =
                                                            std::sync::Arc::new(handler_ptr);
                                                        for i in (0..route_mw_count).rev() {
                                                            if let Some(mw) = route_middleware[i] {
                                                                let next = current_handler;
                                                                current_handler =
                                                                    std::sync::Arc::new(
                                                                        move |ctx| {
                                                                            mw(ctx, next.clone())
                                                                        },
                                                                    );
                                                            }
                                                        }
                                                        for mw in global_mw.iter().rev() {
                                                            let mw = *mw;
                                                            let next = current_handler;
                                                            current_handler =
                                                                std::sync::Arc::new(move |ctx| {
                                                                    mw(ctx, next.clone())
                                                                });
                                                        }
                                                        current_handler(ctx)
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

                                            // Pre-baked 200 OK status line (hot path)
                                            if response.status == 200 {
                                                w!(STATUS_200_OK);
                                            } else {
                                                let mut sl_buf = [0u8; 40];
                                                let sl_len =
                                                    status_line(response.status, &mut sl_buf);
                                                w!(&sl_buf[..sl_len]);
                                            }

                                            // Cached HTTP-date (refreshed once per second)
                                            w!(b"Date: ");
                                            w!(&cached_date);
                                            w!(b"\r\n");

                                            w!(b"Server: chopin\r\n");

                                            w!(b"Content-Type: ");
                                            w!(response.content_type.as_bytes());
                                            w!(b"\r\n");

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

                                            for (k, v) in &response.headers {
                                                w!(k.as_bytes());
                                                w!(b": ");
                                                w!(v.as_bytes());
                                                w!(b"\r\n");
                                            }

                                            w!(b"\r\n");

                                            // Body (only when headers didn't overflow)
                                            if !overflow {
                                                match response.body {
                                                    crate::http::Body::Empty => {}
                                                    crate::http::Body::Static(b) => {
                                                        w!(b);
                                                    }
                                                    crate::http::Body::Bytes(ref b) => {
                                                        w!(b.as_slice());
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
                                                const ERR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
                                                wbuf[..ERR.len()].copy_from_slice(ERR);
                                                pos = ERR.len();
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
                            if read_offset > 0
                                && let Some(conn) = slab.get_mut(idx)
                            {
                                let remaining = conn.read_len as usize;
                                if remaining > 0 {
                                    conn.read_buf
                                        .copy_within(read_offset..read_offset + remaining, 0);
                                }
                            }

                            // ── Write ──
                            if (next_state == ConnState::Writing || is_write)
                                && let Some(conn) = slab.get_mut(idx)
                            {
                                let write_total = conn.write_len as usize;
                                let ws = conn.write_pos as usize;
                                if ws < write_total {
                                    match syscalls::write_nonblocking(
                                        fd,
                                        &conn.write_buf[ws..write_total],
                                    ) {
                                        Ok(n) if n > 0 => {
                                            self.metrics.add_bytes(n);
                                            conn.write_pos += n as u16;
                                            if conn.write_pos as usize >= write_total {
                                                // Fully flushed — reset write buffer
                                                conn.write_len = 0;
                                                conn.write_pos = 0;
                                                let ka = (conn.flags
                                                    & crate::conn::CONN_KEEP_ALIVE)
                                                    != 0;
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
                                        Ok(_) => {}
                                        Err(_) => {
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
                        }

                        if next_state == ConnState::Closing {
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
            }
        }

        for i in 0..slab.capacity() {
            if let Some(conn) = slab.get(i)
                && conn.state != ConnState::Free
            {
                unsafe {
                    libc::close(conn.fd);
                }
            }
        }

        Ok(())
    }

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
                    if let Some(conn) = slab.get(idx) {
                        if conn.state == ConnState::Free {
                            continue; // Already freed
                        }
                        if now.wrapping_sub(conn.last_active) > TIMEOUT {
                            let fd = conn.fd;
                            epoll.delete(fd).ok();
                            unsafe {
                                libc::close(fd);
                            }
                            slab.free(idx);
                            self.metrics.dec_conn();
                        } else {
                            // Connection still alive — re-insert at its current slot
                            drain.reinsert(idx, conn.last_active);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify our zero-alloc date formatter matches httpdate crate output.
    #[test]
    fn test_format_http_date_matches_httpdate() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut buf = [0u8; 29];
        format_http_date(&mut buf, ts);
        let ours = std::str::from_utf8(&buf).expect("valid utf-8");

        let expected = httpdate::fmt_http_date(SystemTime::now());

        // Both should produce the same second (allow 1-second skew)
        assert_eq!(ours.len(), 29, "HTTP date must be exactly 29 bytes");
        // The date portion (first 26 bytes) must match since we're within the same second
        // (in rare cases of second boundary, at least the prefix matches)
        assert_eq!(
            &ours[..26],
            &expected[..26],
            "Date mismatch: ours={ours} expected={expected}"
        );
    }
}
