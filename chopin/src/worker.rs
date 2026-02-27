// src/worker.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::syscalls::{self, Epoll, epoll_event, EPOLLIN, EPOLLOUT};
use crate::slab::ConnectionSlab;
use crate::conn::ConnState;

use crate::router::Router;
use crate::metrics::WorkerMetrics;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Worker {
    id: usize,
    router: Router,
    metrics: Arc<WorkerMetrics>,
    pipe_fd: i32, // Read end of pipe from acceptor
}

impl Worker {
    pub fn new(id: usize, router: Router, metrics: Arc<WorkerMetrics>, pipe_fd: i32) -> Self {
        Self {
            id,
            router,
            metrics,
            pipe_fd,
        }
    }

    pub fn run(&mut self, shutdown: Arc<AtomicBool>) {
        // 1. Setup epoll/kqueue instance
        let epoll = Epoll::new().expect("Failed to create epoll instance");

        // Register the pipe read FD to receive new connections from the acceptor
        let pipe_token = u64::MAX;
        epoll.add(self.pipe_fd, pipe_token, EPOLLIN).expect("Failed to register pipe fd");

        // 2. Initialize Slab Allocator
        let mut slab = ConnectionSlab::new(100_000); // 100k connections per core capacity

        println!("Worker {} entering main event loop (pipe_fd={}).", self.id, self.pipe_fd);

        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024]; // Process up to 1024 events at once
        
        // Wait timeout in ms. Low during shutdown, otherwise bounded for pruning
        let mut timeout = 1000; 
        
        // Track time efficiently in the loop to avoid syscalls during inner parsing
        let mut now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let mut last_prune = now;

        while !shutdown.load(Ordering::Acquire) {
            now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

            // Prune stale connections every 1 second
            if now - last_prune >= 1 {
                for i in 0..slab.high_water() {
                    // Check timeouts
                    if let Some(conn) = slab.get(i) {
                        if conn.state != ConnState::Free && now - conn.last_active > 30 {
                            let fd = conn.fd;
                            epoll.delete(fd).ok();
                            unsafe { libc::close(fd); }
                            slab.free(i);
                            self.metrics.dec_conn();
                        }
                    }
                }
                last_prune = now;
            }

            let n = match epoll.wait(&mut events, timeout) {
                Ok(n) => n,
                Err(_) => continue, // Interrupted likely
            };

            for i in 0..n {
                let token = events[i].u64;
                let is_read = (events[i].events & EPOLLIN as u32) != 0;
                let is_write = (events[i].events & EPOLLOUT as u32) != 0;

                if token == pipe_token {
                    // Drain all FDs from the acceptor pipe
                    if shutdown.load(Ordering::Acquire) {
                        continue;
                    }

                    loop {
                        match syscalls::recv_fd_from_pipe(self.pipe_fd) {
                            Ok(Some(client_fd)) => {
                                // Add to slab
                                if let Some(idx) = slab.allocate(client_fd) {
                                    // Register with epoll
                                    if let Err(_e) = epoll.add(client_fd, idx as u64, EPOLLIN) {
                                        slab.free(idx);
                                        unsafe { libc::close(client_fd); }
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
                                    unsafe { libc::close(client_fd); }
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
                                match syscalls::read_nonblocking(fd, &mut conn.read_buf) {
                                    Ok(0) => {
                                        // EOF - client closed connection
                                        next_state = ConnState::Closing;
                                    }
                                    Ok(n) => {
                                        conn.parse_pos = n as u16;
                                        next_state = ConnState::Parsing;
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                        // Not ready, keep waiting
                                    }
                                    Err(_) => {
                                        next_state = ConnState::Closing;
                                    }
                                }
                            }
                        }

                        if next_state == ConnState::Parsing {
                            if let Some(conn) = slab.get_mut(idx) {
                                let read_len = conn.parse_pos as usize;
                                let buf = &conn.read_buf[..read_len];
                                
                                match crate::parser::parse_request(buf) {
                                    Ok((req, _consumed)) => {
                                        let mut ctx = crate::http::Context {
                                            req,
                                            params: [("", ""); crate::http::MAX_PARAMS],
                                            param_count: 0,
                                        };
                                        
                                        // HTTP/1.1 defaults to keep-alive per RFC 7230
                                        let mut keep_alive = true;
                                        for i in 0..ctx.req.header_count as usize {
                                            let (k, v) = ctx.req.headers[i];
                                            if k.eq_ignore_ascii_case("Connection") && v.eq_ignore_ascii_case("close") {
                                                keep_alive = false;
                                            }
                                        }
                                        
                                        self.metrics.inc_req();
                                        conn.requests_served += 1;
                                        
                                        // Hard cap on keep alive requests per connection
                                        if conn.requests_served >= 10_000 {
                                            keep_alive = false;
                                        }
                                        
                                        let response = match self.router.match_route(ctx.req.method, ctx.req.path) {
                                            Some((handler, params, param_count)) => {
                                                ctx.params = params;
                                                ctx.param_count = param_count;
                                                let mw = self.router.global_middleware;
                                                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                                    if let Some(middleware) = mw {
                                                        middleware(ctx, *handler)
                                                    } else {
                                                        handler(ctx)
                                                    }
                                                }));
                                                match result {
                                                    Ok(r) => r,
                                                    Err(_) => {
                                                        crate::http::Response::internal_error()
                                                    }
                                                }
                                            }
                                            None => {
                                                crate::http::Response::not_found()
                                            }
                                        };

                                        // Format response using raw byte copies (zero-fmt overhead)
                                        let buf = &mut conn.write_buf[..];
                                        let mut pos: usize = 0;
                                        
                                        // Status line
                                        let status_line: &[u8] = match response.status {
                                            200 => b"HTTP/1.1 200 OK\r\n",
                                            404 => b"HTTP/1.1 404 Not Found\r\n",
                                            500 => b"HTTP/1.1 500 Internal Server Error\r\n",
                                            _ => b"HTTP/1.1 200 OK\r\n",
                                        };
                                        buf[pos..pos + status_line.len()].copy_from_slice(status_line);
                                        pos += status_line.len();
                                        
                                        // Content-Type
                                        buf[pos..pos + 14].copy_from_slice(b"Content-Type: ");
                                        pos += 14;
                                        let ct = response.content_type.as_bytes();
                                        buf[pos..pos + ct.len()].copy_from_slice(ct);
                                        pos += ct.len();
                                        buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                        pos += 2;
                                        
                                        let is_chunked = matches!(response.body, crate::http::Body::Stream(_));
                                        
                                        if is_chunked {
                                            buf[pos..pos + 28].copy_from_slice(b"Transfer-Encoding: chunked\r\n");
                                            pos += 28;
                                        } else {
                                            // Content-Length with inline itoa
                                            buf[pos..pos + 16].copy_from_slice(b"Content-Length: ");
                                            pos += 16;
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
                                            buf[pos..pos + itoa_len].copy_from_slice(&itoa_buf[..itoa_len]);
                                            pos += itoa_len;
                                            buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                            pos += 2;
                                        }
                                        
                                        // Connection header
                                        if keep_alive {
                                            buf[pos..pos + 24].copy_from_slice(b"Connection: keep-alive\r\n");
                                            pos += 24;
                                        } else {
                                            buf[pos..pos + 19].copy_from_slice(b"Connection: close\r\n");
                                            pos += 19;
                                        }
                                        
                                        // Custom headers
                                        for (k, v) in &response.headers {
                                            let kb = k.as_bytes();
                                            let vb = v.as_bytes();
                                            buf[pos..pos + kb.len()].copy_from_slice(kb);
                                            pos += kb.len();
                                            buf[pos..pos + 2].copy_from_slice(b": ");
                                            pos += 2;
                                            buf[pos..pos + vb.len()].copy_from_slice(vb);
                                            pos += vb.len();
                                            buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                            pos += 2;
                                        }
                                        
                                        // End of headers
                                        buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                        pos += 2;

                                        // Body
                                        match response.body {
                                            crate::http::Body::Empty => {}
                                            crate::http::Body::Bytes(ref b) => {
                                                buf[pos..pos + b.len()].copy_from_slice(b);
                                                pos += b.len();
                                            }
                                            crate::http::Body::Stream(mut iter) => {
                                                for chunk in iter.by_ref() {
                                                    // Hex length
                                                    let hex_len = {
                                                        let mut n = chunk.len();
                                                        let mut hex_buf = [0u8; 8];
                                                        let mut i = 0;
                                                        if n == 0 { hex_buf[0] = b'0'; i = 1; }
                                                        else {
                                                            while n > 0 {
                                                                let d = (n % 16) as u8;
                                                                hex_buf[i] = if d < 10 { b'0' + d } else { b'A' + d - 10 };
                                                                n /= 16;
                                                                i += 1;
                                                            }
                                                            hex_buf[..i].reverse();
                                                        }
                                                        (hex_buf, i)
                                                    };
                                                    buf[pos..pos + hex_len.1].copy_from_slice(&hex_len.0[..hex_len.1]);
                                                    pos += hex_len.1;
                                                    buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                                    pos += 2;
                                                    buf[pos..pos + chunk.len()].copy_from_slice(&chunk);
                                                    pos += chunk.len();
                                                    buf[pos..pos + 2].copy_from_slice(b"\r\n");
                                                    pos += 2;
                                                }
                                                buf[pos..pos + 5].copy_from_slice(b"0\r\n\r\n");
                                                pos += 5;
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
                        }

                        if next_state == ConnState::Writing || is_write {
                             if let Some(conn) = slab.get_mut(idx) {
                                  let write_total = conn.parse_pos as usize;
                                  let write_start = conn.write_pos as usize;
                                  if write_start < write_total {
                                      match syscalls::write_nonblocking(fd, &conn.write_buf[write_start..write_total]) {
                                          Ok(n) if n > 0 => {
                                               self.metrics.add_bytes(n);
                                               conn.write_pos += n as u16;
                                               if conn.write_pos as usize >= write_total {
                                                   // Full write complete
                                                   if conn.route_id == 1 && !shutdown.load(Ordering::Acquire) {
                                                       conn.state = ConnState::Reading;
                                                       conn.parse_pos = 0;
                                                       conn.write_pos = 0;
                                                       next_state = ConnState::Reading;
                                                   } else {
                                                       conn.state = ConnState::Closing;
                                                       next_state = ConnState::Closing;
                                                   }
                                               }
                                               // else: partial write, stay in Writing state
                                          }
                                          Ok(_) => {
                                               // WouldBlock / 0 bytes, stay in Writing
                                          }
                                          Err(_) => {
                                               next_state = ConnState::Closing;
                                          }
                                      }
                                  }
                             }
                        }

                        if next_state == ConnState::Closing {
                             epoll.delete(fd).ok();
                             unsafe { libc::close(fd); }
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
            if shutdown.load(Ordering::Acquire) { timeout = 100; }
        }

        println!("Worker {} exiting gracefully.", self.id);
        unsafe { libc::close(self.pipe_fd); }
        for i in 0..slab.capacity() {
             if let Some(conn) = slab.get(i) {
                  if conn.state != ConnState::Free {
                       unsafe { libc::close(conn.fd); }
                  }
             }
        }
    }
}
