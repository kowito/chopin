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
    host_port: String,
    router: Router,
    metrics: Arc<WorkerMetrics>,
}

impl Worker {
    pub fn new(id: usize, host_port: String, router: Router, metrics: Arc<WorkerMetrics>) -> Self {
        Self {
            id,
            host_port,
            router,
            metrics,
        }
    }

    pub fn run(&mut self, shutdown: Arc<AtomicBool>) {
        // 1. Create SO_REUSEPORT socket
        let parts: Vec<&str> = self.host_port.split(':').collect();
        let port: u16 = parts.get(1).unwrap_or(&"8080").parse().unwrap();
        let host = parts.get(0).unwrap_or(&"0.0.0.0");
        
        let listen_fd = match syscalls::create_listen_socket(host, port) {
            Ok(fd) => fd,
            Err(e) => {
                eprintln!("Worker {} failed to bind: {}", self.id, e);
                return;
            }
        };

        // 2. Setup epoll instance
        let epoll = Epoll::new().expect("Failed to create epoll instance");
        let listen_token = u64::MAX; // Use MAX for the listen socket
        epoll.add(listen_fd, listen_token, EPOLLIN).expect("Failed to register listen socket");

        // 3. Initialize Slab Allocator
        let mut slab = ConnectionSlab::new(100_000); // 100k connections per core capacity

        println!("Worker {} entering main event loop.", self.id);

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
                for i in 0..slab.capacity() {
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

                if token == listen_token {
                    // Accept loop: drain all pending accept queue entries (Edge Triggered)
                    if shutdown.load(Ordering::Acquire) {
                        continue; // Do not accept new connections during graceful shutdown
                    }
                    
                    loop {
                        match syscalls::accept_connection(listen_fd) {
                            Ok(Some(client_fd)) => {
                                // Add to slab
                                if let Some(idx) = slab.allocate(client_fd) {
                                    // Register with epoll
                                    if let Err(_e) = epoll.add(client_fd, idx as u64, EPOLLIN) {
                                        slab.free(idx);
                                        unsafe { libc::close(client_fd); }
                                    } else {
                                        // Wait until EOF or read is available
                                        if let Some(conn) = slab.get_mut(idx) {
                                            conn.state = ConnState::Reading;
                                            conn.last_active = now;
                                            conn.requests_served = 0;
                                            self.metrics.inc_conn();
                                        }
                                    }
                                } else {
                                    // Out of capacity - backpressure.
                                    // Drop the connection immediately
                                    unsafe { libc::close(client_fd); }
                                }
                            }
                            Ok(None) => break, // WouldBlock, no more connections to accept right now
                            Err(_) => break, // Connection reset / failed
                        }
                    }
                } else {
                    // Regular connection event
                    let idx = token as usize;
                    if let Some(conn_ref) = slab.get(idx) {
                        let fd = conn_ref.fd;
                        let mut next_state = conn_ref.state;
                        
                        // Because we borrow `conn` exclusively from `slab.get_mut`, 
                        // we must manage borrow splitting manually here if we modify it.
                        // To keep it simple for now, pull it mutably per action.
                        
                        if is_read {
                            if let Some(conn) = slab.get_mut(idx) {
                                match syscalls::read_nonblocking(fd, &mut conn.read_buf) {
                                    Ok(0) => {
                                        // EOF, client closed
                                        next_state = ConnState::Closing;
                                    }
                                    Ok(n) => {
                                        // Process HTTP Request in chunks using `conn.parse_pos` etc.
                                        // For the moment, let's just transition to Handling -> Writing
                                        conn.parse_pos += n as u16;
                                        conn.state = ConnState::Parsing;
                                        next_state = ConnState::Parsing;
                                    }
                                    Err(_) => {
                                         next_state = ConnState::Closing;
                                    }
                                }
                            }
                        }

                        if next_state == ConnState::Parsing {
                            if let Some(conn) = slab.get_mut(idx) {
                                let slice = &conn.read_buf[..conn.parse_pos as usize];
                                match crate::parser::parse_request(slice) {
                                    Ok((req, _consumed)) => {
                                        let mut ctx = crate::http::Context {
                                            req,
                                            params: std::collections::HashMap::new(),
                                        };
                                        
                                        let mut keep_alive = false;
                                        for (k, v) in ctx.req.headers.iter() {
                                            if k.eq_ignore_ascii_case("Connection") && v.eq_ignore_ascii_case("keep-alive") {
                                                keep_alive = true;
                                            }
                                        }
                                        
                                        self.metrics.inc_req();
                                        conn.requests_served += 1;
                                        
                                        // Hard cap on keep alive requests per connection
                                        if conn.requests_served >= 10_000 {
                                            keep_alive = false;
                                        }
                                        
                                        let response = match self.router.match_route(ctx.req.method, ctx.req.path) {
                                            Some((handler, params)) => {
                                                ctx.params = params;
                                                handler(ctx)
                                            }
                                            None => {
                                                crate::http::Response::not_found()
                                            }
                                        };

                                        // Format response
                                        use std::io::Write;
                                        let mut cursor = std::io::Cursor::new(&mut conn.write_buf[..]);
                                        let _ = write!(cursor, "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n", 
                                            response.status, response.content_type, response.body.len(),
                                            if keep_alive { "keep-alive" } else { "close" }
                                        );
                                        let _ = cursor.write_all(&response.body);
                                        
                                        conn.parse_pos = cursor.position() as u16; // abuse parse_pos for write len
                                        
                                        // Cache keep alive state in route_id (0: close, 1: keep-alive)
                                        conn.route_id = if keep_alive { 1 } else { 0 };
                                        
                                        conn.state = ConnState::Writing;
                                        next_state = ConnState::Writing;
                                        
                                        let _ = epoll.modify(fd, idx as u64, EPOLLIN | EPOLLOUT);
                                    }
                                    Err(crate::parser::ParseError::Incomplete) => {
                                        // Wait for more data
                                        conn.state = ConnState::Reading;
                                        next_state = ConnState::Reading;
                                    }
                                    Err(crate::parser::ParseError::InvalidFormat) | Err(crate::parser::ParseError::TooLarge) => {
                                        next_state = ConnState::Closing;
                                    }
                                }
                            }
                        }

                        if next_state == ConnState::Writing || is_write {
                             if let Some(conn) = slab.get_mut(idx) {
                                 let write_len = conn.parse_pos as usize;
                                 match syscalls::write_nonblocking(fd, &conn.write_buf[..write_len]) {
                                     Ok(n) => {
                                          self.metrics.add_bytes(n);
                                          // Assume full write for now
                                          if conn.route_id == 1 && !shutdown.load(Ordering::Acquire) {
                                              conn.state = ConnState::Reading;
                                              conn.parse_pos = 0;
                                              next_state = ConnState::Reading;
                                          } else {
                                              conn.state = ConnState::Closing;
                                              next_state = ConnState::Closing;
                                          }
                                     }
                                     Err(_) => {
                                          next_state = ConnState::Closing;
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
                             // Update last_active since we did work in this state transition
                             if let Some(conn) = slab.get_mut(idx) {
                                  conn.last_active = now;
                             }
                        }
                    }
                }
            }
            
            // Adjust timeout if shutdown requested so we don't hang in epoll_wait infinitely
            if shutdown.load(Ordering::Acquire) { timeout = 100; }
        }

        println!("Worker {} exiting gracefully.", self.id);
        
        // Cleanup loop
        unsafe { libc::close(listen_fd); }
        for i in 0..slab.capacity() {
             if let Some(conn) = slab.get(i) {
                  if conn.state != ConnState::Free {
                       unsafe { libc::close(conn.fd); }
                  }
             }
        }
    }
}
