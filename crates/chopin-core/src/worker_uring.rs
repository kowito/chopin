// src/worker_uring.rs — io_uring completion-based event loop (Linux only)
//
// This module replaces the epoll event loop in worker.rs with a fully
// asynchronous io_uring-driven loop. Each Worker owns a single UringRing
// and submits all I/O (accept, read, write, writev, close) as SQEs.
//
// Key differences from the epoll path:
// - No epoll_wait / read / write syscalls in the hot path
// - All I/O submitted as SQEs; completions consumed from CQE ring
// - Multi-shot accept: single SQE generates one CQE per accepted connection
// - Batch submission: multiple SQEs submitted with a single io_uring_enter
// - With SQPOLL: zero syscalls in steady state
//
// Serialization logic (parse → handle → serialize) is intentionally duplicated
// from worker.rs to keep both paths independently optimizable without overhead
// from abstraction. The hot-path serialization is identical.

use crate::conn::{self, ConnState};
use crate::error::{ChopinError, ChopinResult};
use crate::http_date::format_http_date;
use crate::slab::ConnectionSlab;
use crate::syscalls::{self};
use crate::syscalls::uring::{
    self, UringRing, io_uring_cqe,
    IORING_CQE_F_MORE, IORING_SETUP_COOP_TASKRUN, IORING_SETUP_SINGLE_ISSUER,
    IORING_SETUP_SQPOLL,
    OP_TYPE_ACCEPT, OP_TYPE_CLOSE, OP_TYPE_READ, OP_TYPE_WRITE, OP_TYPE_WRITEV,
    ACCEPT_CONN_IDX,
    encode_user_data, decode_user_data,
    prep_accept_multishot, prep_close, prep_read, prep_write, prep_writev,
};
use crate::timer::TimerWheel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::metrics::WorkerMetrics;
use crate::router::Router;
use std::time::{SystemTime, UNIX_EPOCH};

// Re-use the same pre-baked constants from worker.rs
const CT_TEXT_PLAIN: &[u8] = b"Content-Type: text/plain\r\n";
const CT_APP_JSON: &[u8] = b"Content-Type: application/json\r\n";
const STATUS_200_PREFIX: &[u8] = b"HTTP/1.1 200 OK\r\nServer: chopin\r\n";
const FAST_200_JSON: &[u8] =
    b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: application/json\r\n";
const FAST_200_TEXT: &[u8] = b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: text/plain\r\n";
const FAST_200_HTML: &[u8] =
    b"HTTP/1.1 200 OK\r\nServer: chopin\r\nContent-Type: text/html; charset=utf-8\r\n";

/// io_uring SQ ring size — must be power of two.
/// 256 entries gives plenty of headroom for high-concurrency workloads.
const RING_ENTRIES: u32 = 256;

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

pub struct Worker {
    #[allow(dead_code)]
    id: usize,
    router: Router,
    metrics: Arc<WorkerMetrics>,
    listen_fd: i32,
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

    /// Submit a multi-shot accept on the listen fd.
    #[inline]
    fn submit_accept(&self, ring: &mut UringRing) {
        if let Some(sqe) = ring.get_sqe() {
            let ud = encode_user_data(ACCEPT_CONN_IDX as usize, OP_TYPE_ACCEPT);
            prep_accept_multishot(sqe, self.listen_fd, ud);
        }
    }

    /// Submit a read SQE for the given connection.
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

    /// Submit a writev SQE (headers + body in one scatter-gather write).
    /// The iovecs must remain valid until the CQE arrives, so we store them
    /// on the Conn. We use a 2-element array (headers, body).
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

    /// Submit a close SQE for the connection fd.
    #[inline]
    fn submit_close(&self, ring: &mut UringRing, fd: i32, idx: usize) {
        let ud = encode_user_data(idx, OP_TYPE_CLOSE);
        if let Some(sqe) = ring.get_sqe() {
            prep_close(sqe, fd, ud);
        }
    }

    /// Close a connection: free slab slot, submit async close.
    #[inline]
    fn close_connection(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
    ) {
        if let Some(conn) = slab.get_mut(idx) {
            conn.close_sendfile();
            conn.body_clear();
            let fd = conn.fd;
            conn.pending_op = 0;
            slab.free(idx);
            self.metrics.dec_conn();
            // Async close — kernel closes fd without blocking this thread
            self.submit_close(ring, fd, idx);
        }
    }

    /// Process a completed CQE: dispatch based on operation type encoded in user_data.
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
                self.handle_read(ring, slab, timer_wheel, conn_idx, cqe, now, is_shutting_down)?;
            }
            OP_TYPE_WRITE | OP_TYPE_WRITEV => {
                self.handle_write(ring, slab, conn_idx, cqe, now, is_shutting_down)?;
            }
            OP_TYPE_CLOSE => {
                // Close completed — nothing to do (slab already freed)
            }
            _ => {}
        }
        Ok(())
    }

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
            // Accept failed — resubmit if not multi-shot continuation
            if (cqe.flags & IORING_CQE_F_MORE) == 0 {
                self.submit_accept(ring);
            }
            return Ok(());
        }

        let client_fd = cqe.res;

        // Set non-blocking (io_uring accept doesn't inherit SOCK_NONBLOCK on all kernels)
        unsafe {
            let flags = libc::fcntl(client_fd, libc::F_GETFL);
            libc::fcntl(client_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }

        // TCP_NODELAY
        unsafe {
            let one: libc::c_int = 1;
            libc::setsockopt(
                client_fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
            // TCP_QUICKACK for lower latency
            libc::setsockopt(
                client_fd,
                libc::IPPROTO_TCP,
                libc::TCP_QUICKACK,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
        }

        if let Ok(idx) = slab.allocate(client_fd) {
            if let Some(conn) = slab.get_mut(idx) {
                conn.state = ConnState::Reading;
                conn.flags = conn::CONN_KEEP_ALIVE;
                conn.last_active = now;
                conn.requests_served = 0;
                conn.pending_op = 0;
                self.metrics.inc_conn();
                timer_wheel.insert(idx, now);
            }
            // Immediately submit a read for the new connection
            self.submit_read(ring, slab, idx);
        } else {
            // Out of capacity — close immediately
            self.submit_close(ring, client_fd, 0);
        }

        // Multi-shot accept: CQE_F_MORE means the accept SQE is still live.
        // If cleared, we need to resubmit.
        if (cqe.flags & IORING_CQE_F_MORE) == 0 {
            self.submit_accept(ring);
        }

        Ok(())
    }

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
        // Clear pending op
        if let Some(conn) = slab.get_mut(idx) {
            conn.pending_op = 0;
        }

        if cqe.res <= 0 {
            // 0 = EOF, negative = error
            if let Some(conn) = slab.get(idx) {
                if cqe.res == 0 && conn.read_len > 0 {
                    // EOF with data in buffer — try to parse what we have
                    self.pipeline_and_write(ring, slab, timer_wheel, idx, now, is_shutting_down)?;
                    return Ok(());
                }
            }
            self.close_connection(ring, slab, idx);
            return Ok(());
        }

        let bytes_read = cqe.res as usize;
        if let Some(conn) = slab.get_mut(idx) {
            conn.read_len += bytes_read as u16;
            conn.last_active = now;
        }

        // Parse → Handle → Serialize → submit write
        self.pipeline_and_write(ring, slab, timer_wheel, idx, now, is_shutting_down)?;
        Ok(())
    }

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
        // Clear pending op
        if let Some(conn) = slab.get_mut(idx) {
            conn.pending_op = 0;
        }

        if cqe.res < 0 {
            self.close_connection(ring, slab, idx);
            return Ok(());
        }

        let bytes_written = cqe.res as usize;
        self.metrics.add_bytes(bytes_written);

        if let Some(conn) = slab.get_mut(idx) {
            conn.last_active = now;
            let wt = conn.write_len as usize;
            let ws = conn.write_pos as usize;

            // Phase 1: headers (+ possibly partial body for writev)
            if ws < wt {
                if ws == 0 && conn.body_ptr != 0 && conn.body_sent == 0 {
                    // writev CQE: bytes_written spans headers + body
                    if bytes_written >= wt {
                        conn.write_pos = wt as u16;
                        conn.body_sent = (bytes_written - wt) as u32;
                    } else {
                        conn.write_pos = bytes_written as u16;
                    }
                } else {
                    conn.write_pos += bytes_written as u16;
                }
            } else if conn.body_ptr != 0 && conn.body_sent < conn.body_total {
                // Phase 1b: body bytes
                conn.body_sent += bytes_written as u32;
            } else if conn.sendfile_remaining > 0 {
                // Phase 2: sendfile bytes
                conn.sendfile_remaining -= bytes_written as u64;
                conn.sendfile_offset += bytes_written as u64;
            }
        }

        // Check what's next
        if let Some(conn) = slab.get(idx) {
            let ws = conn.write_pos as usize;
            let wt = conn.write_len as usize;

            if ws < wt {
                // More header bytes to write
                self.submit_write(ring, slab, idx);
            } else if conn.body_ptr != 0 && conn.body_sent < conn.body_total {
                // More body bytes to write
                if let Some(conn) = slab.get_mut(idx) {
                    let remaining = (conn.body_total - conn.body_sent) as usize;
                    let body_slice_ptr =
                        (conn.body_ptr + conn.body_sent as usize) as *const u8;
                    let ud = encode_user_data(idx, OP_TYPE_WRITE);
                    conn.pending_op = OP_TYPE_WRITE;
                    if let Some(sqe) = ring.get_sqe() {
                        prep_write(sqe, conn.fd, body_slice_ptr, remaining as u32, ud);
                    }
                }
            } else if conn.sendfile_remaining > 0 {
                // Sendfile: fall back to synchronous sendfile for now
                if let Some(conn) = slab.get_mut(idx) {
                    match syscalls::sendfile_nonblocking(
                        conn.fd,
                        conn.sendfile_fd,
                        &mut conn.sendfile_offset,
                        conn.sendfile_remaining,
                    ) {
                        Ok(n) if n > 0 => {
                            self.metrics.add_bytes(n);
                            conn.sendfile_remaining -= n as u64;
                            if conn.sendfile_remaining > 0 {
                                // Need more writes — re-submit
                                // Use a write of 0 bytes as trigger to re-enter handle_write
                                // Actually, just re-check after sendfile
                                self.complete_response(ring, slab, idx, is_shutting_down)?;
                                return Ok(());
                            }
                        }
                        _ => {
                            conn.close_sendfile();
                        }
                    }
                }
                self.complete_response(ring, slab, idx, is_shutting_down)?;
            } else {
                // All data sent — response complete
                self.complete_response(ring, slab, idx, is_shutting_down)?;
            }
        }

        Ok(())
    }

    /// Response fully sent — clean up and either continue pipeline or wait for next request.
    fn complete_response(
        &self,
        ring: &mut UringRing,
        slab: &mut ConnectionSlab,
        idx: usize,
        is_shutting_down: bool,
    ) -> ChopinResult<()> {
        if let Some(conn) = slab.get_mut(idx) {
            conn.close_sendfile();
            conn.body_clear();
            conn.write_len = 0;
            conn.write_pos = 0;
            conn.pending_op = 0;

            let ka = (conn.flags & conn::CONN_KEEP_ALIVE) != 0;
            if ka && !is_shutting_down {
                if conn.read_len > 0 {
                    // Pipelined request data — parse immediately
                    conn.state = ConnState::Parsing;
                    drop(conn);
                    // Re-enter pipeline
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| ChopinError::ClockError)?
                        .as_secs() as u32;
                    self.pipeline_and_write(ring, slab, &mut TimerWheel::new(now), idx, now, is_shutting_down)?;
                } else {
                    conn.state = ConnState::Reading;
                    drop(conn);
                    self.submit_read(ring, slab, idx);
                }
            } else {
                drop(conn);
                self.close_connection(ring, slab, idx);
            }
        }
        Ok(())
    }

    /// Core pipeline: parse all complete requests from read_buf, serialize responses
    /// into write_buf, then submit write. This mirrors the inner pipeline loop from
    /// worker.rs exactly.
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
            let should_break = {
                if let Some(conn) = slab.get_mut(idx) {
                    let rl = conn.read_len as usize;
                    if rl == 0 {
                        // No data — submit read
                        conn.state = ConnState::Reading;
                        drop(conn);
                        self.submit_read(ring, slab, idx);
                        return Ok(());
                    }

                    // Headroom check
                    let wl = conn.write_len as usize;
                    if wl + 512 > conn::WRITE_BUF_SIZE {
                        true // Need to flush
                    } else {
                        false
                    }
                } else {
                    return Ok(());
                }
            };

            if should_break {
                break;
            }

            // Parse a request
            let parse_result = {
                if let Some(conn) = slab.get_mut(idx) {
                    let rl = conn.read_len as usize;
                    let buf = &mut conn.read_buf[read_offset..read_offset + rl];
                    match crate::parser::parse_request(buf) {
                        Ok((req, consumed)) => Some((req, consumed)),
                        Err(crate::parser::ParseError::Incomplete) => {
                            if conn.write_len > 0 {
                                None // Flush existing writes
                            } else {
                                // Wait for more data
                                conn.state = ConnState::Reading;
                                drop(conn);
                                // Compact before re-reading
                                if read_offset > 0 {
                                    if let Some(conn) = slab.get_mut(idx) {
                                        let remaining = conn.read_len as usize;
                                        if remaining > 0 {
                                            conn.read_buf.copy_within(
                                                read_offset..read_offset + remaining,
                                                0,
                                            );
                                        }
                                    }
                                }
                                self.submit_read(ring, slab, idx);
                                return Ok(());
                            }
                        }
                        Err(_) => {
                            self.close_connection(ring, slab, idx);
                            return Ok(());
                        }
                    }
                } else {
                    return Ok(());
                }
            };

            let Some((req, consumed)) = parse_result else {
                break; // Flush
            };

            // Handle the request
            if let Some(conn) = slab.get_mut(idx) {
                let mut ctx = crate::http::Context {
                    req,
                    params: [("", ""); crate::http::MAX_PARAMS],
                    param_count: 0,
                };

                let mut keep_alive = (conn.flags & conn::CONN_KEEP_ALIVE) != 0;
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
                conn.requests_served += 1;

                let response = match self.router.match_route(ctx.req.method, ctx.req.path) {
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
                            Err(_) => crate::http::Response::server_error(),
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
                let wstart = conn.write_len as usize;
                let wbuf = &mut conn.write_buf[wstart..];
                let mut pos: usize = 0;
                let mut overflow = false;

                macro_rules! w {
                    ($src:expr) => {
                        if !overflow {
                            let c = $src;
                            let end = pos + c.len();
                            if let Some(slice) = wbuf.get_mut(pos..end) {
                                slice.copy_from_slice(c);
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

                // Date header: fresh timestamp per response — no caching.
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
                            if wstart == 0
                                && pos + b.len() <= conn::WRITE_BUF_SIZE
                            {
                                conn.body_ptr = b.as_ptr() as usize;
                                conn.body_total = b.len() as u32;
                            } else {
                                w!(b);
                            }
                        }
                        crate::http::Body::Bytes(b) => {
                            if wstart == 0
                                && pos + b.len() <= conn::WRITE_BUF_SIZE
                            {
                                let boxed = b.into_boxed_slice();
                                conn.body_ptr = boxed.as_ptr() as usize;
                                conn.body_total = boxed.len() as u32;
                                conn.body_owned = Some(boxed);
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
                            conn.sendfile_fd = fd.take();
                            conn.sendfile_offset = offset;
                            conn.sendfile_remaining = len;
                        }
                    }
                }

                if overflow {
                    if wstart > 0 {
                        // Flush previously queued responses
                        break;
                    }
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

                conn.write_len = (wstart + pos) as u16;
                read_offset += consumed;
                conn.read_len = (conn.read_len as usize - consumed) as u16;

                if !keep_alive {
                    conn.flags &= !conn::CONN_KEEP_ALIVE;
                    break; // Stop pipelining, flush
                }

                if conn.body_ptr != 0 {
                    break; // Need writev, stop pipelining
                }

                // Continue inner loop → next pipelined request
            } else {
                return Ok(());
            }
        }

        // Deferred compaction
        if read_offset > 0 {
            if let Some(conn) = slab.get_mut(idx) {
                let remaining = conn.read_len as usize;
                if remaining > 0 {
                    conn.read_buf
                        .copy_within(read_offset..read_offset + remaining, 0);
                }
            }
        }

        // Submit the write
        if let Some(conn) = slab.get(idx) {
            let ws = conn.write_pos as usize;
            let wt = conn.write_len as usize;

            if ws == 0 && conn.body_ptr != 0 && conn.body_sent == 0 && wt > 0 {
                // writev: headers + body
                let header_slice = &conn.write_buf[0..wt];
                let body_slice = unsafe {
                    std::slice::from_raw_parts(
                        conn.body_ptr as *const u8,
                        conn.body_total as usize,
                    )
                };
                // Build iovecs on the stack
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
            } else if conn.write_len == 0 {
                // Nothing was serialized — wait for more data
                drop(conn);
                self.submit_read(ring, slab, idx);
            }
        }

        Ok(())
    }

    /// Main io_uring event loop.
    #[allow(clippy::collapsible_if)]
    pub fn run(&mut self, shutdown: Arc<AtomicBool>) -> ChopinResult<()> {
        // Setup io_uring ring with single-issuer + cooperative task run hints.
        // Start without SQPOLL for initial stability; can be enabled later.
        let setup_flags = IORING_SETUP_SINGLE_ISSUER | IORING_SETUP_COOP_TASKRUN;
        let mut ring = UringRing::new(RING_ENTRIES, setup_flags)
            .map_err(|e| ChopinError::Other(format!("io_uring_setup failed: {e}")))?;

        let mut slab = ConnectionSlab::new(10_000);

        let mut now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ChopinError::ClockError)?
            .as_secs() as u32;
        let mut last_prune = now;
        let mut timer_wheel = TimerWheel::new(now);
        let mut iter_count: u32 = 0;

        // Submit initial multi-shot accept
        self.submit_accept(&mut ring);
        ring.submit()?;

        loop {
            let is_shutting_down = shutdown.load(Ordering::Acquire);
            if is_shutting_down && slab.is_empty() {
                break;
            }
            iter_count = iter_count.wrapping_add(1);

            // Time update and pruning every 1024 iterations
            #[allow(clippy::manual_is_multiple_of)]
            if iter_count % 1024 == 0 {
                now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ChopinError::ClockError)?
                    .as_secs() as u32;

                if now - last_prune >= 1 {
                    self.prune_connections_wheel_uring(
                        &mut ring,
                        &mut slab,
                        &mut timer_wheel,
                        now,
                    );
                    last_prune = now;
                }
            }

            // Submit pending SQEs and wait for at least 1 CQE
            ring.submit_and_wait(1)?;

            // Drain all available CQEs
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

                // Batch: process up to 64 CQEs before re-submitting
                if cqe_count >= 64 {
                    ring.submit()?;
                    cqe_count = 0;
                }
            }

            // Submit any SQEs generated during CQE processing
            if cqe_count > 0 {
                ring.submit()?;
            }
        }

        // Graceful shutdown: close all active connections
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
                        if let Some(conn) = slab.get(idx) {
                            if conn.state == ConnState::Free {
                                continue;
                            }
                            (now.wrapping_sub(conn.last_active) > TIMEOUT, conn.last_active)
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
