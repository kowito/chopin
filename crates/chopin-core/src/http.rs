//! Core HTTP types: [`Request`], [`Response`], [`Context`], [`Method`], and [`Body`].
//!
//! Every handler receives a [`Context`] (which wraps the parsed [`Request`]) and
//! returns a [`Response`].  The most common response builders are:
//!
//! ```rust,no_run
//! use chopin_core::{Context, KJson, Response};
//!
//! fn handler(ctx: Context) -> Response {
//!     // Plain text
//!     Response::text("hello")
//! }
//!
//! fn json_handler(ctx: Context) -> Response {
//!     // Typed JSON — any type that derives chopin_core::KJson
//!     #[derive(KJson)]
//!     struct Payload { ok: bool }
//!     Response::json(&Payload { ok: true })
//! }
//!
//! fn typed_param(ctx: Context) -> Response {
//!     // Parse a path parameter; returns 400 Bad Request on failure
//!     let id: i32 = match ctx.param_parse("id") {
//!         Ok(v)  => v,
//!         Err(r) => return r,
//!     };
//!     Response::text(id.to_string())
//! }
//! ```
// src/http.rs
use crate::headers::{Headers, IntoHeaderValue};
use crate::syscalls;
use std::io;

/// HTTP request method.
///
/// Uses a `u8` repr for fast array-indexed dispatch in the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Method {
    Get = 0,
    Post = 1,
    Put = 2,
    Delete = 3,
    Patch = 4,
    Head = 5,
    Options = 6,
    Trace = 7,
    Connect = 8,
    Unknown = 9,
}

impl Method {
    /// First-byte dispatch for fast HTTP method parsing (picohttpparser technique).
    #[inline(always)]
    pub fn from_bytes(b: &[u8]) -> Self {
        if b.is_empty() {
            return Method::Unknown;
        }
        match b[0] {
            b'G' => {
                if b.len() == 3 && b[1] == b'E' && b[2] == b'T' {
                    Method::Get
                } else {
                    Method::Unknown
                }
            }
            b'P' => {
                if b.len() < 3 {
                    return Method::Unknown;
                }
                match b[1] {
                    b'O' => {
                        if b.len() == 4 && b[2] == b'S' && b[3] == b'T' {
                            Method::Post
                        } else {
                            Method::Unknown
                        }
                    }
                    b'U' => {
                        if b.len() == 3 && b[2] == b'T' {
                            Method::Put
                        } else {
                            Method::Unknown
                        }
                    }
                    b'A' => {
                        if b.len() == 5 && b[2] == b'T' && b[3] == b'C' && b[4] == b'H' {
                            Method::Patch
                        } else {
                            Method::Unknown
                        }
                    }
                    _ => Method::Unknown,
                }
            }
            b'D' => {
                if b == b"DELETE" {
                    Method::Delete
                } else {
                    Method::Unknown
                }
            }
            b'H' => {
                if b == b"HEAD" {
                    Method::Head
                } else {
                    Method::Unknown
                }
            }
            b'O' => {
                if b == b"OPTIONS" {
                    Method::Options
                } else {
                    Method::Unknown
                }
            }
            b'T' => {
                if b == b"TRACE" {
                    Method::Trace
                } else {
                    Method::Unknown
                }
            }
            b'C' => {
                if b == b"CONNECT" {
                    Method::Connect
                } else {
                    Method::Unknown
                }
            }
            _ => Method::Unknown,
        }
    }
}

pub const MAX_HEADERS: usize = 32;
pub const MAX_PARAMS: usize = 4;

/// A parsed HTTP request. All fields borrow from the connection's read buffer
/// — no heap allocation occurs during request parsing.
pub struct Request<'a> {
    pub method: Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: [(&'a str, &'a str); MAX_HEADERS],
    pub header_count: u8,
    pub body: &'a [u8],
}

/// RAII wrapper for a file descriptor. Closes the fd on drop unless taken.
pub struct OwnedFd(i32);

impl OwnedFd {
    /// Wrap an already-opened file descriptor.
    pub fn new(fd: i32) -> Self {
        Self(fd)
    }

    /// Take the raw fd, preventing Drop from closing it.
    /// The caller assumes ownership of closing the fd.
    pub(crate) fn take(&mut self) -> i32 {
        let fd = self.0;
        self.0 = -1;
        fd
    }

    /// Peek at the raw fd without taking ownership.
    #[allow(dead_code)]
    pub fn raw(&self) -> i32 {
        self.0
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe {
                libc::close(self.0);
            }
        }
    }
}

/// The body of an HTTP response.
///
/// Supports multiple storage strategies: zero-copy static slices, heap-allocated
/// bytes, streaming iterators, and kernel-level `sendfile` for files.
pub enum Body {
    /// No body content.
    Empty,
    /// A compile-time static byte slice — zero allocation, zero copy.
    Static(&'static [u8]),
    /// Heap-allocated byte vector.
    Bytes(Vec<u8>),
    /// Chunked streaming body — each call to `next()` yields a chunk.
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
    /// Zero-copy file body — served via `sendfile()` entirely in kernel space.
    /// The fd is owned and will be closed when the response is consumed or dropped.
    File { fd: OwnedFd, offset: u64, len: u64 },
    /// Fully pre-baked raw HTTP response (status line + headers + body) as a
    /// static byte slice. The worker writes this verbatim, bypassing ALL
    /// header serialization logic. Maximum throughput — zero overhead.
    ///
    /// Use [`Response::raw`] to construct. You are responsible for producing a
    /// valid HTTP/1.1 response including "\r\n\r\n" and the body.
    Raw(&'static [u8]),
}

impl Body {
    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            Body::Empty => 0,
            Body::Static(b) => b.len(),
            Body::Bytes(b) => b.len(),
            Body::Stream(_) => 0, // unknown until streamed
            Body::File { len, .. } => *len as usize,
            Body::Raw(b) => b.len(), // full response bytes
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Body::Empty => &[],
            Body::Static(b) => b,
            Body::Bytes(b) => b.as_slice(),
            Body::Stream(_) => &[], // Streams must be polled/chunked iteratively
            Body::File { .. } => &[], // File data lives on disk, sent via sendfile
            Body::Raw(b) => b,      // raw full response
        }
    }

    /// Returns `true` if this body will be served via zero-copy `sendfile`.
    #[inline(always)]
    pub fn is_file(&self) -> bool {
        matches!(self, Body::File { .. })
    }

    /// Returns `true` if this body is a pre-baked full raw HTTP response.
    #[inline(always)]
    pub fn is_raw(&self) -> bool {
        matches!(self, Body::Raw(_))
    }
}

/// An HTTP response to be sent to the client.
///
/// Construct responses using the factory methods ([`Response::text`],
/// [`Response::json`], [`Response::file`], etc.) and customise with
/// [`Response::with_header`] and status code assignment.
///
/// # Examples
///
/// ```rust,ignore
/// // Plain text
/// Response::text("Hello, world!")
///
/// // JSON (Schema-JIT serialization)
/// Response::json(&user)
///
/// // Custom status + headers
/// let mut res = Response::json(&item);
/// res.status = 201;
/// res.with_header("Location", "/items/42")
/// ```
pub struct Response {
    pub status: u16,
    pub body: Body,
    pub content_type: &'static str,
    /// Custom response headers — stored inline (stack) for ≤8 headers,
    /// falling back to heap for more. No allocation for common cases.
    pub headers: Headers,
}

impl Response {
    /// Create a response with no body and a given status code.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            body: Body::Empty,
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// Builder-style method to append an HTTP response header.
    ///
    /// The value may be a `&'static str`, `String`, or any integer type.
    /// Short values (≤ 64 bytes) are stored inline on the stack; longer
    /// values fall back to heap allocation.
    pub fn with_header(mut self, name: &'static str, value: impl IntoHeaderValue) -> Self {
        self.headers.add(name, value);
        self
    }

    /// 200 OK with a plain-text body.
    pub fn text(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 200 OK with a zero-copy static plain-text body.
    /// Avoids heap allocation — ideal for fixed responses like TFB plaintext.
    pub fn text_static(body: &'static [u8]) -> Self {
        Self {
            status: 200,
            body: Body::Static(body),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 200 OK with a pre-serialized JSON byte body.
    /// Use `Response::json()` when you have a typed value to serialize.
    pub fn json_bytes(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "application/json",
            headers: Headers::new(),
        }
    }

    /// 200 OK — serializes a typed value to JSON using the Schema-JIT engine.
    /// Uses the per-worker buffer pool (Phase 1.2) to avoid a fresh allocation
    /// on every request.
    pub fn json<T: kowito_json::serialize::Serialize>(val: &T) -> Self {
        let mut buf = crate::bufpool::get_with_capacity(128);
        val.serialize(&mut buf);
        Self::json_bytes(buf.into_vec())
    }

    /// 200 OK with a zero-copy static pre-serialized JSON body.
    ///
    /// The fastest JSON response: `&'static [u8]` known at compile time.
    /// Zero heap allocation on every request.
    ///
    /// # Example
    /// ```ignore
    /// // Pre-bake at compile time:
    /// Response::json_static(b"{\"message\":\"Hello, World!\"}")
    /// ```
    #[inline(always)]
    pub fn json_static(body: &'static [u8]) -> Self {
        Self {
            status: 200,
            body: Body::Static(body),
            content_type: "application/json",
            headers: Headers::new(),
        }
    }

    /// Emit a fully pre-baked HTTP/1.1 response verbatim.
    ///
    /// The supplied `bytes` must be a **complete**, valid HTTP/1.1 response
    /// (status line + headers + blank line + body). The worker writes them
    /// as-is, bypassing every header serialization step.
    ///
    /// This is the **absolute fastest** response path — a single `memcpy`
    /// into the connection's `write_buf`, then one `write(2)` syscall.
    ///
    /// # Safety contract
    /// You must include `Date:`, `Content-Length:`, and `Content-Type:` headers
    /// yourself. Chopin will NOT add them for `Body::Raw` responses.
    ///
    /// # Example
    /// ```ignore
    /// // Build once at program start:
    /// static PONG: &[u8] = b"HTTP/1.1 200 OK\r\n\
    ///     Server: chopin\r\n\
    ///     Content-Type: text/plain\r\n\
    ///     Content-Length: 4\r\n\
    ///     Connection: keep-alive\r\n\
    ///     \r\n\
    ///     pong";
    ///
    /// fn pong(_ctx: Context) -> Response { Response::raw(PONG) }
    /// ```
    #[inline(always)]
    pub fn raw(bytes: &'static [u8]) -> Self {
        Self {
            status: 200,
            body: Body::Raw(bytes),
            content_type: "",
            headers: Headers::new(),
        }
    }

    /// 404 Not Found.
    pub fn not_found() -> Self {
        Self {
            status: 404,
            body: Body::Static(b"Not Found"),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 500 Internal Server Error.
    pub fn server_error() -> Self {
        Self {
            status: 500,
            body: Body::Static(b"Internal Server Error"),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 400 Bad Request.
    pub fn bad_request() -> Self {
        Self {
            status: 400,
            body: Body::Static(b"Bad Request"),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 401 Unauthorized.
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            body: Body::Static(b"Unauthorized"),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// 403 Forbidden.
    pub fn forbidden() -> Self {
        Self {
            status: 403,
            body: Body::Static(b"Forbidden"),
            content_type: "text/plain",
            headers: Headers::new(),
        }
    }

    /// Chunked streaming response with `application/octet-stream` content type.
    pub fn stream(iter: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Self {
        Self {
            status: 200,
            body: Body::Stream(Box::new(iter)),
            content_type: "application/octet-stream",
            headers: Headers::new(),
        }
    }

    /// Serve a file using zero-copy `sendfile`. Content-Type is inferred from the
    /// file extension. Returns 404 if the file does not exist or cannot be opened.
    ///
    /// Pass `range_header` (the value of the `Range:` request header, e.g.
    /// `"bytes=0-1023"`) to honour RFC 7233 range requests.  A valid range
    /// produces a `206 Partial Content` response with a `Content-Range` header;
    /// an unsatisfiable range produces `416 Range Not Satisfiable`.
    /// Pass `None` (or `Response::file(path)` without a range) for a full 200.
    pub fn file(path: &str) -> Self {
        match Self::try_file(path, None) {
            Ok(resp) => resp,
            Err(_) => Self::not_found(),
        }
    }

    /// Like [`Response::file`] but honours the `Range:` header value for partial
    /// content delivery (RFC 7233).
    ///
    /// # Example
    /// ```ignore
    /// fn handler(ctx: Context) -> Response {
    ///     let range = ctx.header("range");
    ///     Response::file_range("./static/video.mp4", range)
    /// }
    /// ```
    pub fn file_range(path: &str, range_header: Option<&str>) -> Self {
        match Self::try_file(path, range_header) {
            Ok(resp) => resp,
            Err(_) => Self::not_found(),
        }
    }

    /// Internal: attempt to open a file and build a zero-copy response.
    fn try_file(path: &str, range_header: Option<&str>) -> io::Result<Self> {
        // Reject paths with directory-traversal segments, null bytes, or
        // absolute paths — no legitimate static-file path needs these.
        if path.contains("..") || path.contains('\0') || path.starts_with('/') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path traversal rejected",
            ));
        }
        let fd = syscalls::open_file_readonly(path)?;
        let total_size = match syscalls::file_size(fd) {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    libc::close(fd);
                }
                return Err(e);
            }
        };
        let content_type = mime_from_path(path);

        // E.3: Range request handling (RFC 7233)
        if let Some(range_val) = range_header {
            match parse_range(range_val, total_size) {
                RangeResult::Range(start, end) => {
                    // 206 Partial Content
                    let len = end - start + 1;
                    // Build Content-Range: bytes START-END/TOTAL inline (no heap).
                    // Max size: "bytes " + 20 + "-" + 20 + "/" + 20 = 67 bytes
                    let mut cr_buf = [0u8; 68];
                    let cr_len = fmt_content_range(&mut cr_buf, start, end, total_size);
                    let cr_str = std::str::from_utf8(&cr_buf[..cr_len])
                        .unwrap_or("")
                        .to_owned(); // heap only for this header value
                    let mut resp = Self {
                        status: 206,
                        body: Body::File {
                            fd: OwnedFd::new(fd),
                            offset: start,
                            len,
                        },
                        content_type,
                        headers: Headers::new(),
                    };
                    resp.headers.add("Content-Range", cr_str);
                    resp.headers.add("Accept-Ranges", "bytes");
                    return Ok(resp);
                }
                RangeResult::Unsatisfiable => {
                    // 416 Range Not Satisfiable — include Content-Range: bytes */TOTAL
                    unsafe {
                        libc::close(fd);
                    }
                    let mut cr_buf = [0u8; 68];
                    let cr_len = fmt_content_range_star(&mut cr_buf, total_size);
                    let cr_str = std::str::from_utf8(&cr_buf[..cr_len])
                        .unwrap_or("")
                        .to_owned();
                    let mut resp = Self {
                        status: 416,
                        body: Body::Empty,
                        content_type: "text/plain",
                        headers: Headers::new(),
                    };
                    resp.headers.add("Content-Range", cr_str);
                    return Ok(resp);
                }
                RangeResult::None => {} // fall through to 200
            }
        }

        let mut resp = Self {
            status: 200,
            body: Body::File {
                fd: OwnedFd::new(fd),
                offset: 0,
                len: total_size,
            },
            content_type,
            headers: Headers::new(),
        };
        resp.headers.add("Accept-Ranges", "bytes");
        Ok(resp)
    }

    /// Serve a byte range of a file (e.g. for `Range` header support).
    /// The caller provides an already-opened fd, offset, and length.
    /// Ownership of the fd is transferred to the response.
    pub fn sendfile(fd: i32, offset: u64, len: u64, content_type: &'static str) -> Self {
        Self {
            status: 200,
            body: Body::File {
                fd: OwnedFd::new(fd),
                offset,
                len,
            },
            content_type,
            headers: Headers::new(),
        }
    }

    /// Compress the response body with gzip encoding.
    ///
    /// Works on `Body::Bytes` and `Body::Static` variants — `Stream` and `File`
    /// bodies are returned unchanged (they have their own delivery paths).
    /// Adds `Content-Encoding: gzip` and `Vary: Accept-Encoding` headers.
    #[cfg(feature = "compression")]
    pub fn gzip(mut self) -> Self {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;

        let raw = match &self.body {
            Body::Static(b) => *b,
            Body::Bytes(b) => b.as_slice(),
            _ => return self,
        };

        if raw.is_empty() {
            return self;
        }

        let mut encoder = GzEncoder::new(Vec::with_capacity(raw.len() / 2), Compression::fast());
        if encoder.write_all(raw).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                if compressed.len() < raw.len() {
                    self.body = Body::Bytes(compressed);
                    self.headers.add("Content-Encoding", "gzip");
                    self.headers.add("Vary", "Accept-Encoding");
                }
            }
        }
        self
    }
}

// ── E.3: Range request helpers (RFC 7233) ────────────────────────────────────

enum RangeResult {
    /// Resolved byte range [start, end] (inclusive, 0-based).
    Range(u64, u64),
    /// Range header present but unsatisfiable (→ 416).
    Unsatisfiable,
    /// No range header or unparseable format (→ 200).
    None,
}

/// Parse `Range: bytes=<start>-<end>` or `bytes=<start>-` (open-ended).
/// Returns `RangeResult::None` for any syntax we cannot handle.
#[inline]
fn parse_range(header: &str, total: u64) -> RangeResult {
    let s = header.trim();
    let s = match s.strip_prefix("bytes=") {
        Some(v) => v,
        None => return RangeResult::None,
    };
    // Only handle the first range in a multi-range set.
    let s = s.split(',').next().unwrap_or("").trim();
    let dash = match s.find('-') {
        Some(i) => i,
        None => return RangeResult::None,
    };
    let start_str = &s[..dash];
    let end_str = &s[dash + 1..];

    // suffix-range: -N  (last N bytes)
    if start_str.is_empty() {
        let suffix: u64 = match end_str.parse() {
            Ok(n) => n,
            Err(_) => return RangeResult::None,
        };
        if suffix == 0 || total == 0 {
            return RangeResult::Unsatisfiable;
        }
        let start = total.saturating_sub(suffix);
        return RangeResult::Range(start, total - 1);
    }

    let start: u64 = match start_str.parse() {
        Ok(n) => n,
        Err(_) => return RangeResult::None,
    };

    if start >= total {
        return RangeResult::Unsatisfiable;
    }

    let end: u64 = if end_str.is_empty() {
        total - 1
    } else {
        match end_str.parse::<u64>() {
            Ok(n) => n.min(total - 1),
            Err(_) => return RangeResult::None,
        }
    };

    if end < start {
        return RangeResult::Unsatisfiable;
    }

    RangeResult::Range(start, end)
}

/// Write `bytes START-END/TOTAL\0` into `buf`. Returns bytes written (no NUL).
#[inline]
fn fmt_content_range(buf: &mut [u8; 68], start: u64, end: u64, total: u64) -> usize {
    let prefix = b"bytes ";
    let mut pos = 0;
    buf[pos..pos + prefix.len()].copy_from_slice(prefix);
    pos += prefix.len();
    pos += fmt_u64_into(&mut buf[pos..], start);
    buf[pos] = b'-';
    pos += 1;
    pos += fmt_u64_into(&mut buf[pos..], end);
    buf[pos] = b'/';
    pos += 1;
    pos += fmt_u64_into(&mut buf[pos..], total);
    pos
}

/// Write `bytes */TOTAL` into `buf`. Returns bytes written.
#[inline]
fn fmt_content_range_star(buf: &mut [u8; 68], total: u64) -> usize {
    let prefix = b"bytes */";
    buf[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();
    pos += fmt_u64_into(&mut buf[pos..], total);
    pos
}

/// Write a u64 as ASCII decimal into `dst`. Returns number of digits written.
#[inline]
fn fmt_u64_into(dst: &mut [u8], mut n: u64) -> usize {
    if n == 0 {
        dst[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    tmp[..i].reverse();
    dst[..i].copy_from_slice(&tmp[..i]);
    i
}

/// Infer a Content-Type from a file path's extension.
/// Returns a `&'static str` so it can be stored directly in Response.
fn mime_from_path(path: &str) -> &'static str {
    let ext = match path.rsplit('.').next() {
        Some(e) => e,
        None => return "application/octet-stream",
    };
    match ext {
        // Text
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "svg" => "image/svg+xml",
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        // Fonts
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        // Media
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        // Archives / binary
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
}

/// Trait for types that can be converted into an HTTP [`Response`].
///
/// Implemented for `Response`, `String`, `&'static str`, and
/// `Result<T, E>` where both `T` and `E` implement `IntoResponse`.
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::text(self.into_bytes())
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::text(self.as_bytes().to_vec())
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(v) => v.into_response(),
            Err(e) => e.into_response(),
        }
    }
}

/// The request context passed to every handler.
///
/// Provides access to the parsed [`Request`], URL path parameters, headers,
/// and typed extractors via [`Context::extract`].
///
/// # Examples
///
/// ```rust,ignore
/// fn handler(ctx: Context) -> Response {
///     // Path parameter
///     let id = ctx.param("id").unwrap_or("0");
///
///     // Header
///     let ua = ctx.header("user-agent").unwrap_or("unknown");
///
///     // JSON body extractor
///     let Json(body) = ctx.extract::<Json<MyPayload>>().unwrap();
///
///     Response::text("ok")
/// }
/// ```
pub struct Context<'a> {
    pub req: Request<'a>,
    pub params: [(&'a str, &'a str); MAX_PARAMS],
    pub param_count: u8,
    /// IPv6-mapped peer address from the socket layer (set at accept time).
    /// Use this for rate limiting and audit logging — it cannot be forged by clients.
    pub peer_addr: [u8; 16],
}

impl<'a> Context<'a> {
    /// Extract a URL path parameter by name, e.g. `:id` → `ctx.param("id")`.
    pub fn param(&self, key: &str) -> Option<&'a str> {
        for i in 0..self.param_count as usize {
            if self.params[i].0 == key {
                return Some(self.params[i].1);
            }
        }
        None
    }

    /// Retrieve a request header value by name (case-insensitive).
    pub fn header(&self, key: &str) -> Option<&'a str> {
        for i in 0..self.req.header_count as usize {
            if self.req.headers[i].0.eq_ignore_ascii_case(key) {
                return Some(self.req.headers[i].1);
            }
        }
        None
    }

    /// Parse the request body as a multipart/form-data stream.
    /// Returns `None` if the `Content-Type` header is not `multipart/form-data`.
    #[allow(clippy::collapsible_if)]
    pub fn multipart(&self) -> Option<crate::multipart::Multipart<'a>> {
        let ct = self.header("content-type")?;
        if ct.starts_with("multipart/form-data") {
            if let Some(idx) = ct.find("boundary=") {
                let boundary = &ct[idx + 9..];
                return Some(crate::multipart::Multipart::new(self.req.body, boundary));
            }
        }
        None
    }

    /// Use the extractor pattern to parse typed data from the request
    /// (e.g. `ctx.extract::<Json<MyBody>>()`).
    pub fn extract<T: crate::extract::FromRequest<'a>>(&'a self) -> Result<T, T::Error> {
        T::from_request(self)
    }

    /// Serialize a typed value to JSON and return a `200 OK` response.
    /// Shorthand for `Response::json(val)` inside a handler.
    pub fn json<T: crate::json::Serialize>(&self, val: &T) -> Response {
        Response::json(val)
    }

    /// Deserialize the request body as JSON into `T`.
    ///
    /// Returns `Err(400 Bad Request)` when the body is not valid JSON or doesn't
    /// match the expected schema.  Enables `?` in handlers that return
    /// `Result<Response, Response>` or any `E: From<Response>`.
    ///
    /// Shorthand for `ctx.extract::<Json<T>>().map(|Json(v)| v)`.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[post("/users")]
    /// fn create_user(ctx: Context) -> Result<Response, Response> {
    ///     let body: CreateUser = ctx.body_json()?;
    ///     Ok(Response::json(&body))
    /// }
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn body_json<'b, T>(&'b self) -> Result<T, Response>
    where
        T: serde::Deserialize<'b>,
    {
        serde_json::from_slice(self.req.body).map_err(|_| Response::bad_request())
    }

    /// Deserialize the URL query string into `T`.
    ///
    /// Returns `Err(400 Bad Request)` when the query string cannot be deserialized.
    /// Enables `?` in handlers that return `Result<Response, Response>`.
    ///
    /// Shorthand for `ctx.extract::<Query<T>>().map(|Query(v)| v)`.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[derive(serde::Deserialize)]
    /// struct Pagination { page: u32, limit: u32 }
    ///
    /// #[get("/posts")]
    /// fn list_posts(ctx: Context) -> Result<Response, Response> {
    ///     let Pagination { page, limit } = ctx.query_params()?;
    ///     Ok(Response::text(format!("page={page} limit={limit}")))
    /// }
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn query_params<T>(&self) -> Result<T, Response>
    where
        T: serde::de::DeserializeOwned,
    {
        let qs = self.req.query.unwrap_or("");
        serde_urlencoded::from_str::<T>(qs).map_err(|_| Response::bad_request())
    }

    /// Retrieve a clone of the per-thread state value of type `T`.
    ///
    /// Returns `None` if no value of that type was stored via [`chopin_core::set_state`].
    /// The idiomatic pattern is to call `set_state` inside `Chopin::with_worker_init`.
    ///
    /// For `Arc<T>` values this is an O(1) ref-count increment. For large structs,
    /// prefer wrapping in `Arc` to keep the clone cheap.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[get("/users")]
    /// fn list_users(ctx: Context) -> Result<Response, Response> {
    ///     let pool = ctx.state::<Arc<MyPool>>().ok_or_else(Response::server_error)?;
    ///     // use pool ...
    ///     Ok(Response::text("ok"))
    /// }
    /// ```
    pub fn state<T: Clone + 'static>(&self) -> Option<T> {
        crate::state::get_state::<T>()
    }

    /// Extract and parse a URL path parameter by name.
    ///
    /// Returns `Err(400 Bad Request)` if the parameter is absent or fails to
    /// parse as `T`.  Works for any type that implements [`std::str::FromStr`].
    ///
    /// # Example
    /// ```rust,ignore
    /// #[get("/posts/:id")]
    /// fn show(ctx: Context) -> Response {
    ///     let id: i32 = ctx.param_parse("id")?;
    ///     // ...
    /// }
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn param_parse<T: std::str::FromStr>(&self, key: &str) -> Result<T, Response> {
        let raw = self.param(key).ok_or_else(Response::bad_request)?;
        raw.parse::<T>().map_err(|_| Response::bad_request())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Method::from_bytes ───────────────────────────────────────────────────

    #[test]
    fn test_method_get() {
        assert_eq!(Method::from_bytes(b"GET"), Method::Get);
    }
    #[test]
    fn test_method_post() {
        assert_eq!(Method::from_bytes(b"POST"), Method::Post);
    }
    #[test]
    fn test_method_put() {
        assert_eq!(Method::from_bytes(b"PUT"), Method::Put);
    }
    #[test]
    fn test_method_delete() {
        assert_eq!(Method::from_bytes(b"DELETE"), Method::Delete);
    }
    #[test]
    fn test_method_patch() {
        assert_eq!(Method::from_bytes(b"PATCH"), Method::Patch);
    }
    #[test]
    fn test_method_head() {
        assert_eq!(Method::from_bytes(b"HEAD"), Method::Head);
    }
    #[test]
    fn test_method_options() {
        assert_eq!(Method::from_bytes(b"OPTIONS"), Method::Options);
    }
    #[test]
    fn test_method_trace() {
        assert_eq!(Method::from_bytes(b"TRACE"), Method::Trace);
    }
    #[test]
    fn test_method_connect() {
        assert_eq!(Method::from_bytes(b"CONNECT"), Method::Connect);
    }

    #[test]
    fn test_method_empty_is_unknown() {
        assert_eq!(Method::from_bytes(b""), Method::Unknown);
    }

    #[test]
    fn test_method_lowercase_is_unknown() {
        assert_eq!(Method::from_bytes(b"get"), Method::Unknown);
        assert_eq!(Method::from_bytes(b"post"), Method::Unknown);
    }

    #[test]
    fn test_method_truncated_is_unknown() {
        assert_eq!(Method::from_bytes(b"GE"), Method::Unknown);
        assert_eq!(Method::from_bytes(b"POS"), Method::Unknown);
        assert_eq!(Method::from_bytes(b"DEL"), Method::Unknown);
    }

    #[test]
    fn test_method_junk_is_unknown() {
        assert_eq!(Method::from_bytes(b"GETX"), Method::Unknown);
        assert_eq!(Method::from_bytes(b"XPOST"), Method::Unknown);
    }

    #[test]
    fn test_method_eq_and_copy() {
        let m = Method::Get;
        let m2 = m; // Copy
        assert_eq!(m, m2);
        assert_ne!(Method::Get, Method::Post);
    }

    // ─── Response constructors ────────────────────────────────────────────────

    #[test]
    fn test_response_new_status() {
        let r = Response::new(204);
        assert_eq!(r.status, 204);
        assert!(r.body.is_empty());
    }

    #[test]
    fn test_response_text_status_and_ct() {
        let r = Response::text(b"hello".to_vec());
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/plain");
        assert_eq!(r.body.as_bytes(), b"hello");
    }

    #[test]
    fn test_response_text_static() {
        let r = Response::text_static(b"static");
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/plain");
        assert_eq!(r.body.as_bytes(), b"static");
    }

    #[test]
    fn test_response_json_bytes() {
        let r = Response::json_bytes(b"{}".to_vec());
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/json");
        assert_eq!(r.body.as_bytes(), b"{}");
    }

    #[test]
    fn test_response_not_found() {
        let r = Response::not_found();
        assert_eq!(r.status, 404);
    }

    #[test]
    fn test_response_server_error() {
        let r = Response::server_error();
        assert_eq!(r.status, 500);
    }

    #[test]
    fn test_response_bad_request() {
        let r = Response::bad_request();
        assert_eq!(r.status, 400);
    }

    #[test]
    fn test_response_unauthorized() {
        let r = Response::unauthorized();
        assert_eq!(r.status, 401);
    }

    #[test]
    fn test_response_forbidden() {
        let r = Response::forbidden();
        assert_eq!(r.status, 403);
    }

    #[test]
    fn test_response_with_header_adds_header() {
        let r = Response::new(200).with_header("x-custom", "value");
        assert_eq!(r.status, 200);
        // Headers should contain the custom header
        let found = r
            .headers
            .iter()
            .any(|h| h.name == "x-custom" && h.value.as_str() == "value");
        assert!(found, "header x-custom: value not found");
    }

    // ─── Body ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_body_empty() {
        let b = Body::Empty;
        assert_eq!(b.len(), 0);
        assert!(b.is_empty());
        assert_eq!(b.as_bytes(), b"");
        assert!(!b.is_file());
    }

    #[test]
    fn test_body_static() {
        let b = Body::Static(b"hello");
        assert_eq!(b.len(), 5);
        assert!(!b.is_empty());
        assert_eq!(b.as_bytes(), b"hello");
    }

    #[test]
    fn test_body_bytes() {
        let v = b"world".to_vec();
        let b = Body::Bytes(v.clone());
        assert_eq!(b.len(), 5);
        assert_eq!(b.as_bytes(), b"world");
    }

    #[test]
    fn test_body_stream_len_is_zero() {
        let b = Body::Stream(Box::new(std::iter::empty()));
        assert_eq!(b.len(), 0);
        assert!(b.is_empty());
        assert!(!b.is_file());
    }
}
