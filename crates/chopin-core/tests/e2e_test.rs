// tests/e2e_test.rs
//!
//! End-to-end tests that spin up a *real* single-worker Chopin HTTP server on
//! 127.0.0.1:8090 and exercise every observable transport feature via raw TCP
//! sockets (no http client library involved).
//!
//! Coverage:
//!   - All HTTP methods: GET, POST, PUT, DELETE, PATCH, HEAD
//!   - Status codes: 200, 201, 204, 400, 404, 500
//!   - Path parameters (single and double)
//!   - Query string forwarding
//!   - Request header reading
//!   - Custom response headers
//!   - keep-alive (multiple requests on one TCP connection)
//!   - HTTP/1.1 pipelining (send N requests, then read N responses)
//!   - Chunked request body (Transfer-Encoding: chunked)
//!   - Chunked response (streaming)
//!   - Content-Length accuracy
//!   - Server: header
//!   - JSON content-type
//!   - JSON body extractor + bad-JSON → 400
//!   - Large response body (64 KB)
//!   - Empty body (POST with no payload)
//!   - 404 for unknown path
//!   - 404 when method has no registered handler
//!   - 20 concurrent connections racing the same endpoint
//!   - Wildcard route matching

use chopin_core::{Context, Json, Method, Response, Router, Server};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Once;
use std::thread;
use std::time::Duration;

// ─── server setup ───────────────────────────────────────────────────────────

const ADDR: &str = "127.0.0.1:8090";
static SERVER: Once = Once::new();

#[derive(Deserialize)]
struct NameMsg {
    name: String,
}

fn ensure_server() {
    SERVER.call_once(|| {
        let mut router = Router::new();

        // GET /ping → 200 "pong"
        router.add(Method::Get, "/ping", |_: Context| {
            Response::text_static(b"pong")
        });

        // POST /echo → 200, echo body
        router.add(Method::Post, "/echo", |ctx: Context| {
            Response::text(ctx.req.body.to_vec())
        });

        // PUT /resource → 201 Created
        router.add(Method::Put, "/resource", |_: Context| Response {
            status: 201,
            body: chopin_core::http::Body::Static(b"Created"),
            content_type: "text/plain",
            headers: Vec::new(),
        });

        // DELETE /resource → 204 No Content
        router.add(Method::Delete, "/resource", |_: Context| Response {
            status: 204,
            body: chopin_core::http::Body::Empty,
            content_type: "text/plain",
            headers: Vec::new(),
        });

        // PATCH /resource → 200, echo body
        router.add(Method::Patch, "/resource", |ctx: Context| {
            Response::text(ctx.req.body.to_vec())
        });

        // HEAD /ping → 200, no body
        router.add(Method::Head, "/ping", |_: Context| Response {
            status: 200,
            body: chopin_core::http::Body::Empty,
            content_type: "text/plain",
            headers: Vec::new(),
        });

        // GET /json → application/json
        router.add(Method::Get, "/json", |_: Context| {
            Response::json_bytes(b"{\"ok\":true}".to_vec())
        });

        // GET /params/:a/:b → "a,b"
        router.add(Method::Get, "/params/:a/:b", |ctx: Context| {
            let a = ctx.param("a").unwrap_or("?");
            let b = ctx.param("b").unwrap_or("?");
            Response::text(format!("{a},{b}"))
        });

        // GET /query → return raw query string
        router.add(Method::Get, "/query", |ctx: Context| {
            Response::text(ctx.req.query.unwrap_or("").to_owned())
        });

        // GET /header-echo → echo X-Test request header
        router.add(Method::Get, "/header-echo", |ctx: Context| {
            Response::text(ctx.header("x-test").unwrap_or("missing").to_owned())
        });

        // GET /custom-header → sets X-Custom response header
        router.add(Method::Get, "/custom-header", |_: Context| {
            Response::text_static(b"ok").with_header("X-Custom", "chopin-e2e")
        });

        // GET /stream → Transfer-Encoding: chunked response
        router.add(Method::Get, "/stream", |_: Context| {
            let chunks = vec![b"hello ".to_vec(), b"world".to_vec()];
            Response::stream(chunks.into_iter())
        });

        // POST /upload → receive body (including chunked), return byte count
        router.add(Method::Post, "/upload", |ctx: Context| {
            Response::text(ctx.req.body.len().to_string())
        });

        // GET /large → 14 KB response (fits inside the 16 KB write buffer + headers)
        router.add(Method::Get, "/large", |_: Context| {
            Response::text(vec![b'A'; 14_000])
        });

        // GET /overflow → 65 KB response (overflows write buffer → server returns 500)
        router.add(Method::Get, "/overflow", |_: Context| {
            Response::text(vec![b'B'; 65536])
        });

        // POST /json-extract → typed JSON extraction
        router.add(Method::Post, "/json-extract", move |ctx: Context| match ctx
            .extract::<Json<NameMsg>>()
        {
            Ok(Json(m)) => Response::text(format!("Hello, {}!", m.name)),
            Err(r) => r,
        });

        // GET /error → 500
        router.add(Method::Get, "/error", |_: Context| Response::server_error());

        // GET /wildcard/*name → return the request path
        router.add(Method::Get, "/wildcard/*name", |ctx: Context| {
            Response::text(ctx.req.path.to_owned())
        });

        // GET /slow → used to test concurrent load without racing
        router.add(Method::Get, "/slow", |_: Context| {
            thread::sleep(Duration::from_millis(5));
            Response::text_static(b"ok")
        });

        thread::spawn(move || {
            Server::bind(ADDR).workers(1).serve(router).unwrap();
        });

        // Wait for the socket to be ready
        for _ in 0..50 {
            if TcpStream::connect(ADDR).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
}

// ─── raw HTTP/1.1 helper ────────────────────────────────────────────────────

struct Conn(TcpStream);

struct Resp {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Resp {
    fn body_str(&self) -> &str {
        std::str::from_utf8(&self.body).unwrap_or("<invalid utf-8>")
    }
    fn header(&self, k: &str) -> Option<&str> {
        self.headers.get(k).map(String::as_str)
    }
}

impl Conn {
    fn open() -> Self {
        let s = TcpStream::connect(ADDR).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Conn(s)
    }

    fn send(&mut self, req: &[u8]) {
        self.0.write_all(req).expect("write");
    }

    /// Read exactly one HTTP/1.1 response, respecting Content-Length and
    /// Transfer-Encoding: chunked.  Works on keep-alive connections.
    fn recv(&mut self) -> Resp {
        let s = &mut self.0;

        // ── Read headers until \r\n\r\n ──────────────────────────────────
        let mut hdr_buf = Vec::with_capacity(512);
        let mut window = [0u8; 4];
        loop {
            let mut b = [0u8; 1];
            s.read_exact(&mut b).expect("read header byte");
            hdr_buf.push(b[0]);
            window = [window[1], window[2], window[3], b[0]];
            if window == *b"\r\n\r\n" {
                break;
            }
        }

        // ── Parse status line + headers ──────────────────────────────────
        let hdr_str = String::from_utf8_lossy(&hdr_buf);
        let mut lines = hdr_str.split("\r\n");

        let status_line = lines.next().unwrap_or("");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        let mut headers: HashMap<String, String> = HashMap::new();
        let mut content_length: Option<usize> = None;
        let mut is_chunked = false;

        for line in lines.filter(|l| !l.is_empty()) {
            if let Some((k, v)) = line.split_once(": ") {
                let k_lc = k.to_lowercase();
                if k_lc == "content-length" {
                    content_length = v.trim().parse().ok();
                }
                if k_lc == "transfer-encoding" && v.to_lowercase().contains("chunked") {
                    is_chunked = true;
                }
                headers.insert(k_lc, v.to_owned());
            }
        }

        // ── Read body ────────────────────────────────────────────────────
        let body = if is_chunked {
            let mut body = Vec::new();
            loop {
                // Read chunk-size line (hex CRLF)
                let mut sz_buf = Vec::new();
                loop {
                    let mut b = [0u8; 1];
                    s.read_exact(&mut b).expect("read chunk size byte");
                    if b[0] == b'\n' {
                        break;
                    }
                    if b[0] != b'\r' {
                        sz_buf.push(b[0]);
                    }
                }
                let sz =
                    usize::from_str_radix(std::str::from_utf8(&sz_buf).unwrap_or("0").trim(), 16)
                        .unwrap_or(0);

                if sz == 0 {
                    // Trailing CRLF after last chunk
                    let mut crlf = [0u8; 2];
                    let _ = s.read_exact(&mut crlf);
                    break;
                }

                let mut chunk = vec![0u8; sz];
                s.read_exact(&mut chunk).expect("read chunk data");
                body.extend_from_slice(&chunk);

                // CRLF after chunk data
                let mut crlf = [0u8; 2];
                s.read_exact(&mut crlf).expect("read chunk CRLF");
            }
            body
        } else {
            let n = content_length.unwrap_or(0);
            let mut body = vec![0u8; n];
            if n > 0 {
                s.read_exact(&mut body).expect("read body");
            }
            body
        };

        Resp {
            status,
            headers,
            body,
        }
    }
}

/// One-shot: open, send, recv, drop.
fn once(req: &[u8]) -> Resp {
    let mut c = Conn::open();
    c.send(req);
    c.recv()
}

// ─── tests ──────────────────────────────────────────────────────────────────

// ── basic GET ───────────────────────────────────────────────────────────────

#[test]
fn test_get_200_text() {
    ensure_server();
    let r = once(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "pong");
    assert_eq!(r.header("content-type"), Some("text/plain"));
}

#[test]
fn test_server_header_present() {
    ensure_server();
    let r = once(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.header("server"), Some("chopin"));
}

#[test]
fn test_content_length_matches_body() {
    ensure_server();
    let r = once(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let cl: usize = r.header("content-length").unwrap().parse().unwrap();
    assert_eq!(
        cl,
        r.body.len(),
        "Content-Length must match actual body size"
    );
    assert_eq!(cl, 4); // "pong"
}

// ── HTTP methods ────────────────────────────────────────────────────────────

#[test]
fn test_post_echoes_body() {
    ensure_server();
    let body = b"hello chopin";
    let req = format!(
        "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);
    let r = once(&raw);
    assert_eq!(r.status, 200);
    assert_eq!(r.body.as_slice(), body);
}

#[test]
fn test_put_returns_201() {
    ensure_server();
    let r = once(b"PUT /resource HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 201);
    assert_eq!(r.body_str(), "Created");
}

#[test]
fn test_delete_returns_204_empty_body() {
    ensure_server();
    let r = once(b"DELETE /resource HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 204);
    assert!(r.body.is_empty(), "204 must have no body");
}

#[test]
fn test_patch_echoes_body() {
    ensure_server();
    let body = b"patch-payload";
    let req = format!(
        "PATCH /resource HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);
    let r = once(&raw);
    assert_eq!(r.status, 200);
    assert_eq!(r.body.as_slice(), body);
}

#[test]
fn test_head_returns_no_body() {
    ensure_server();
    let r = once(b"HEAD /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty(), "HEAD response must have no body");
}

// ── JSON ────────────────────────────────────────────────────────────────────

#[test]
fn test_json_content_type() {
    ensure_server();
    let r = once(b"GET /json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.body_str(), r#"{"ok":true}"#);
}

#[test]
fn test_json_extractor_valid() {
    ensure_server();
    let body = br#"{"name":"Chopin"}"#;
    let req = format!(
        "POST /json-extract HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);
    let r = once(&raw);
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "Hello, Chopin!");
}

#[test]
fn test_json_extractor_bad_json_returns_400() {
    ensure_server();
    let body = b"not-valid-json";
    let req = format!(
        "POST /json-extract HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);
    let r = once(&raw);
    assert_eq!(r.status, 400);
}

// ── path params & query strings ─────────────────────────────────────────────

#[test]
fn test_single_path_param() {
    ensure_server();
    let r =
        once(b"GET /params/hello/world HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "hello,world");
}

#[test]
fn test_path_params_url_segment_values() {
    ensure_server();
    let r = once(b"GET /params/foo/42 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "foo,42");
}

#[test]
fn test_query_string_forwarded_to_handler() {
    ensure_server();
    let r = once(
        b"GET /query?key=value&other=123 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "key=value&other=123");
}

#[test]
fn test_query_string_empty_when_absent() {
    ensure_server();
    let r = once(b"GET /query HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "");
}

// ── request / response headers ──────────────────────────────────────────────

#[test]
fn test_request_header_read_by_handler() {
    ensure_server();
    let r = once(b"GET /header-echo HTTP/1.1\r\nHost: localhost\r\nX-Test: my-value\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "my-value");
}

#[test]
fn test_missing_request_header_returns_fallback() {
    ensure_server();
    let r = once(b"GET /header-echo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "missing");
}

#[test]
fn test_custom_response_header() {
    ensure_server();
    let r = once(b"GET /custom-header HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("x-custom"), Some("chopin-e2e"));
}

// ── chunked transfer encoding ────────────────────────────────────────────────

#[test]
fn test_chunked_response_streaming() {
    ensure_server();
    let r = once(b"GET /stream HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("transfer-encoding"), Some("chunked"));
    // chunks "hello " + "world" should be concatenated by our reader
    assert_eq!(r.body_str(), "hello world");
}

#[test]
fn test_chunked_request_body_decoded() {
    ensure_server();
    // send "hello" (5 bytes) as a single chunk
    let req = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let r = once(req);
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "5");
}

#[test]
fn test_chunked_request_multi_chunks() {
    ensure_server();
    // "hello " (6) + "world" (5) = 11 bytes total
    let req = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n6\r\nhello \r\n5\r\nworld\r\n0\r\n\r\n";
    let r = once(req);
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "11");
}

// ── error responses ──────────────────────────────────────────────────────────

#[test]
fn test_404_unknown_path() {
    ensure_server();
    let r = once(b"GET /does-not-exist HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 404);
}

#[test]
fn test_404_wrong_method() {
    ensure_server();
    // /ping only has GET; POST must 404
    let r = once(
        b"POST /ping HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(r.status, 404);
}

#[test]
fn test_500_server_error_response() {
    ensure_server();
    let r = once(b"GET /error HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 500);
}

// ── body edge cases ──────────────────────────────────────────────────────────

#[test]
fn test_post_empty_body() {
    ensure_server();
    let r = once(
        b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty());
}

#[test]
fn test_large_response_body_14kb() {
    ensure_server();
    // 14 000 bytes fits inside the 16 KB write buffer alongside response headers.
    let r = once(b"GET /large HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert_eq!(r.body.len(), 14_000);
    assert!(
        r.body.iter().all(|&b| b == b'A'),
        "body must be all 'A' bytes"
    );
    let cl: usize = r.header("content-length").unwrap().parse().unwrap();
    assert_eq!(cl, 14_000, "Content-Length must match body length");
}

#[test]
fn test_response_overflow_returns_500() {
    ensure_server();
    // A 65 536-byte body exceeds the 16 KB write buffer.  The server is
    // expected to replace the response with a 500 Internal Server Error.
    let r = once(b"GET /overflow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(
        r.status, 500,
        "write-buffer overflow must produce a 500 response"
    );
}

#[test]
fn test_large_request_body() {
    ensure_server();
    // 5 000 bytes fits inside the 8 KB read buffer alongside request headers.
    let body = vec![b'Z'; 5_000];
    let req = format!(
        "POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(&body);
    let r = once(&raw);
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "5000");
}

// ── keep-alive & pipelining ──────────────────────────────────────────────────

#[test]
fn test_keep_alive_multiple_requests_same_conn() {
    ensure_server();
    let mut c = Conn::open();

    // Request 1
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
    let r1 = c.recv();
    assert_eq!(r1.status, 200);
    assert_eq!(r1.body_str(), "pong");
    assert_eq!(r1.header("connection"), Some("keep-alive"));

    // Request 2 on same TCP connection
    c.send(b"GET /json HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
    let r2 = c.recv();
    assert_eq!(r2.status, 200);
    assert_eq!(r2.header("content-type"), Some("application/json"));

    // Request 3: signal close
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let r3 = c.recv();
    assert_eq!(r3.status, 200);
    assert_eq!(r3.header("connection"), Some("close"));
}

#[test]
fn test_keep_alive_five_requests() {
    ensure_server();
    let mut c = Conn::open();

    for _ in 0..4 {
        c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
        let r = c.recv();
        assert_eq!(r.status, 200);
        assert_eq!(r.body_str(), "pong");
    }

    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let r = c.recv();
    assert_eq!(r.status, 200);
    assert_eq!(r.body_str(), "pong");
    assert_eq!(r.header("connection"), Some("close"));
}

#[test]
fn test_pipeline_two_requests() {
    ensure_server();
    let mut c = Conn::open();

    // Send both before reading either
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    let r1 = c.recv();
    let r2 = c.recv();

    assert_eq!(r1.status, 200, "pipelined r1");
    assert_eq!(r1.body_str(), "pong");
    assert_eq!(r2.status, 200, "pipelined r2");
    assert_eq!(r2.body_str(), "pong");
}

#[test]
fn test_pipeline_five_requests() {
    ensure_server();
    let mut c = Conn::open();

    // Blast 5 keep-alive requests then one close
    for _ in 0..5 {
        c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
    }
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    for i in 0..6 {
        let r = c.recv();
        assert_eq!(r.status, 200, "pipeline request {i}");
        assert_eq!(r.body_str(), "pong");
    }
}

// ── concurrency ─────────────────────────────────────────────────────────────

#[test]
fn test_20_concurrent_connections() {
    ensure_server();

    let handles: Vec<_> = (0..20)
        .map(|_| {
            thread::spawn(|| {
                let r = once(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
                assert_eq!(r.status, 200);
                assert_eq!(r.body_str(), "pong");
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_concurrent_mixed_endpoints() {
    ensure_server();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                if i % 2 == 0 {
                    let r =
                        once(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
                    assert_eq!(r.status, 200);
                    assert_eq!(r.body_str(), "pong");
                } else {
                    let r =
                        once(b"GET /json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
                    assert_eq!(r.status, 200);
                    assert_eq!(r.header("content-type"), Some("application/json"));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

// ── routing ──────────────────────────────────────────────────────────────────

#[test]
fn test_wildcard_route_matches() {
    ensure_server();
    let r = once(b"GET /wildcard/foo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert!(
        r.body_str().contains("/wildcard/foo"),
        "wildcard path: {}",
        r.body_str()
    );
}

#[test]
fn test_root_not_registered_returns_404() {
    ensure_server();
    let r = once(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 404);
}

// ── misc protocol details ────────────────────────────────────────────────────

#[test]
fn test_connection_close_ends_conversation() {
    ensure_server();
    let mut c = Conn::open();
    c.send(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    let r = c.recv();
    assert_eq!(r.status, 200);
    assert_eq!(r.header("connection"), Some("close"));

    // After Connection: close, server should have closed – further reads yield 0 bytes
    let mut leftover = Vec::new();
    let _ = c.0.read_to_end(&mut leftover);
    assert!(leftover.is_empty(), "no extra bytes after close");
}

#[test]
fn test_multiple_response_headers_coexist() {
    ensure_server();
    let r = once(b"GET /custom-header HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    // All three framework + custom headers must be present
    assert!(r.header("server").is_some(), "Server header missing");
    assert!(
        r.header("content-length").is_some(),
        "Content-Length header missing"
    );
    assert!(
        r.header("content-type").is_some(),
        "Content-Type header missing"
    );
    assert_eq!(r.header("x-custom"), Some("chopin-e2e"));
}
