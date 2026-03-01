// src/worker.rs
use crate::conn::ConnState;
use crate::error::{ChopinError, ChopinResult};
use crate::slab::ConnectionSlab;
use crate::syscalls::{self, EPOLLIN, EPOLLOUT, Epoll, epoll_event};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::metrics::WorkerMetrics;
use crate::router::Router;
use std::time::{SystemTime, UNIX_EPOCH};

/// Format an HTTP status line into a fixed 40-byte buffer. Returns the slice length.
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

pub struct Worker {
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
        let epoll = Epoll::new().map_err(|e| {
            eprintln!("Worker {} failed to create epoll instance: {}", self.id, e);
            e
        })?;

        // Register the listen fd
        let listen_token = u64::MAX;
        if let Err(e) = epoll.add(self.listen_fd, listen_token, EPOLLIN) {
            eprintln!("Worker {} failed to register listen fd: {}", self.id, e);
            return Ok(());
        }

        // 2. Initialize Slab Allocator
        let mut slab = ConnectionSlab::new(100_000); // 100k connections per core capacity

        println!(
            "Worker {} entering main event loop (listen_fd={}).",
            self.id, self.listen_fd
        );

        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024]; // Process up to 1024 events at once

        // Wait timeout in ms.
        let mut timeout = 1000;

        let mut now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ChopinError::Other("Clock went backwards".to_string()))?
            .as_secs() as u32;
        let mut last_prune = now;
        let mut iter_count: u32 = 0;

        // Pre-formatted Date header (RFC 7231 §7.1.1.2). Refresh every second.
        let mut date_header: [u8; 64] = [0u8; 64];
        let mut date_header_len: usize = 0;
        let mut last_date_update: u32;

        let update_date_header =
            |date_header: &mut [u8; 64], date_header_len: &mut usize, now_secs: u32| {
                let sys = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(now_secs as u64);
                let formatted = httpdate::fmt_http_date(sys);
                let prefix = b"Date: ";
                let suffix = b"\r\n";
                let formatted_bytes = formatted.as_bytes();
                let total = prefix.len() + formatted_bytes.len() + suffix.len();
                if total <= 64 {
                    date_header[..prefix.len()].copy_from_slice(prefix);
                    date_header[prefix.len()..prefix.len() + formatted_bytes.len()]
                        .copy_from_slice(formatted_bytes);
                    date_header[prefix.len() + formatted_bytes.len()
                        ..prefix.len() + formatted_bytes.len() + suffix.len()]
                        .copy_from_slice(suffix);
                    *date_header_len = total;
                }
            };

        update_date_header(&mut date_header, &mut date_header_len, now);
        last_date_update = now;

        loop {
            let is_shutting_down = shutdown.load(Ordering::Acquire);
            if is_shutting_down && slab.is_empty() {
                break;
            }
            iter_count = iter_count.wrapping_add(1);

            // Only update time and prune every 1024 iterations or after wait
            if iter_count.is_multiple_of(1024) {
                now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::Other("Clock went backwards".to_string()))?
                    .as_secs() as u32;
                if now - last_date_update >= 1 {
                    update_date_header(&mut date_header, &mut date_header_len, now);
                    last_date_update = now;
                }
                if now - last_prune >= 1 {
                    self.prune_connections(&mut slab, &epoll, now);
                    last_prune = now;
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
                                            conn.last_active = now;
                                            conn.requests_served = 0;
                                            self.metrics.inc_conn();
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
                            let read_start = conn.parse_pos as usize;
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
                                        conn.parse_pos += n as u16;
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

                        if next_state == ConnState::Parsing
                            && let Some(conn) = slab.get_mut(idx)
                        {
                            let read_len = conn.parse_pos as usize;
                            let buf = &mut conn.read_buf[..read_len];

                            match crate::parser::parse_request(buf) {
                                Ok((req, _consumed)) => {
                                    let mut ctx = crate::http::Context {
                                        req,
                                        params: [("", ""); crate::http::MAX_PARAMS],
                                        param_count: 0,
                                    };

                                    let mut keep_alive = true;
                                    if is_shutting_down {
                                        keep_alive = false; // Drain
                                    } else {
                                        for i in 0..ctx.req.header_count as usize {
                                            let (k, v) = ctx.req.headers[i];
                                            if k.eq_ignore_ascii_case("Connection")
                                                && v.eq_ignore_ascii_case("close")
                                            {
                                                keep_alive = false;
                                            }
                                        }

                                        // Hard cap on keep alive requests per connection
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
                                        Some((handler, params, param_count, route_middleware)) => {
                                            ctx.params = params;
                                            ctx.param_count = param_count;

                                            // Extract the handler and the middleware stack
                                            let handler_ptr = *handler;

                                            // Combine router.global_middleware with route-specific middleware
                                            let mut mw_stack =
                                                self.router.global_middleware.clone();
                                            mw_stack.extend(route_middleware);

                                            let result = std::panic::catch_unwind(
                                                std::panic::AssertUnwindSafe(|| {
                                                    if mw_stack.is_empty() {
                                                        handler_ptr(ctx)
                                                    } else {
                                                        // Base execution
                                                        let mut current_handler: crate::router::BoxedHandler =
                                                            std::sync::Arc::new(handler_ptr);

                                                        // Wrap middlewares from last to first
                                                        for mw in mw_stack.into_iter().rev() {
                                                            let next = current_handler;
                                                            current_handler =
                                                                std::sync::Arc::new(move |ctx| {
                                                                    mw(ctx, next.clone())
                                                                });
                                                        }

                                                        current_handler(ctx)
                                                    }
                                                }),
                                            );
                                            match result {
                                                Ok(r) => r,
                                                Err(_) => crate::http::Response::internal_error(),
                                            }
                                        }
                                        None => crate::http::Response::not_found(),
                                    };

                                    // Format response into the write buffer.
                                    // Uses bounds-checked buf_write() — on overflow we
                                    // discard the partial write and send a 500 + close.
                                    let buf = &mut conn.write_buf[..];
                                    let mut pos: usize = 0;
                                    let mut overflow = false;

                                    macro_rules! w {
                                        ($src:expr) => {
                                            if !overflow {
                                                let c = $src;
                                                let end = pos + c.len();
                                                if let Some(slice) = buf.get_mut(pos..end) {
                                                    slice.copy_from_slice(c);
                                                    pos = end;
                                                } else {
                                                    overflow = true;
                                                }
                                            }
                                        };
                                    }

                                    // Status line
                                    let mut sl_buf = [0u8; 40];
                                    let sl_len = status_line(response.status, &mut sl_buf);
                                    w!(&sl_buf[..sl_len]);

                                    // Date header (RFC 7231 §7.1.1.2)
                                    w!(&date_header[..date_header_len]);

                                    // Server header
                                    w!(b"Server: chopin\r\n");

                                    // Content-Type
                                    w!(b"Content-Type: ");
                                    w!(response.content_type.as_bytes());
                                    w!(b"\r\n");

                                    let is_chunked =
                                        matches!(response.body, crate::http::Body::Stream(_));

                                    if is_chunked {
                                        w!(b"Transfer-Encoding: chunked\r\n");
                                    } else {
                                        // Content-Length with inline itoa
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

                                    // Connection header
                                    if keep_alive {
                                        w!(b"Connection: keep-alive\r\n");
                                    } else {
                                        w!(b"Connection: close\r\n");
                                    }

                                    // Custom headers
                                    for (k, v) in &response.headers {
                                        w!(k.as_bytes());
                                        w!(b": ");
                                        w!(v.as_bytes());
                                        w!(b"\r\n");
                                    }

                                    // End of headers
                                    w!(b"\r\n");

                                    if overflow {
                                        // Response too large for write buffer — send minimal 500
                                        const ERR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
                                        buf[..ERR.len()].copy_from_slice(ERR);
                                        pos = ERR.len();
                                        keep_alive = false;
                                    } else {
                                        // Body
                                        match response.body {
                                            crate::http::Body::Empty => {}
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
                                        if overflow {
                                            // Body overflowed — truncate to headers-only 500
                                            const ERR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
                                            buf[..ERR.len()].copy_from_slice(ERR);
                                            pos = ERR.len();
                                            keep_alive = false;
                                        }
                                    }

                                    conn.parse_pos = pos as u16; // total write len
                                    conn.write_pos = 0; // start from beginning
                                    conn.route_id = if keep_alive { 1 } else { 0 };

                                    conn.state = ConnState::Writing;
                                    next_state = ConnState::Writing;

                                    let _ = epoll.modify(fd, idx as u64, EPOLLIN | EPOLLOUT);
                                }
                                Err(crate::parser::ParseError::Incomplete) => {
                                    conn.state = ConnState::Reading;
                                    next_state = ConnState::Reading;
                                }
                                Err(_) => {
                                    next_state = ConnState::Closing;
                                }
                            }
                        }

                        if (next_state == ConnState::Writing || is_write)
                            && let Some(conn) = slab.get_mut(idx)
                        {
                            let write_total = conn.parse_pos as usize;
                            let write_start = conn.write_pos as usize;
                            if write_start < write_total {
                                match syscalls::write_nonblocking(
                                    fd,
                                    &conn.write_buf[write_start..write_total],
                                ) {
                                    Ok(n) if n > 0 => {
                                        self.metrics.add_bytes(n);
                                        conn.write_pos += n as u16;
                                        if conn.write_pos as usize >= write_total {
                                            if conn.route_id == 1 && !is_shutting_down {
                                                conn.state = ConnState::Reading;
                                                conn.parse_pos = 0;
                                                conn.write_pos = 0;
                                                next_state = ConnState::Reading;
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

        println!("Worker {} exiting gracefully.", self.id);
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

    fn prune_connections(&self, slab: &mut ConnectionSlab, epoll: &Epoll, now: u32) {
        for i in 0..slab.high_water() {
            if let Some(conn) = slab.get(i)
                && conn.state != ConnState::Free
                && now - conn.last_active > 30
            {
                let fd = conn.fd;
                // Remove from epoll BEFORE closing the fd to avoid stale events.
                epoll.delete(fd).ok();
                unsafe {
                    libc::close(fd);
                }
                slab.free(i);
                self.metrics.dec_conn();
            }
        }
    }
}
