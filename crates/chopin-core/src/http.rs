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

pub const MAX_HEADERS: usize = 16;
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
}

impl Body {
    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            Body::Empty => 0,
            Body::Static(b) => b.len(),
            Body::Bytes(b) => b.len(),
            Body::Stream(_) => 0, // Chunked has no predefined length
            Body::File { len, .. } => *len as usize,
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
        }
    }

    /// Returns `true` if this body will be served via zero-copy `sendfile`.
    #[inline(always)]
    pub fn is_file(&self) -> bool {
        matches!(self, Body::File { .. })
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
    /// This is the primary way to return structured data from a handler.
    pub fn json<T: kowito_json::serialize::Serialize>(val: &T) -> Self {
        let mut buf = Vec::with_capacity(128);
        val.serialize(&mut buf);
        Self::json_bytes(buf)
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
    pub fn file(path: &str) -> Self {
        match Self::try_file(path) {
            Ok(resp) => resp,
            Err(_) => Self::not_found(),
        }
    }

    /// Internal: attempt to open a file and build a zero-copy response.
    fn try_file(path: &str) -> io::Result<Self> {
        let fd = syscalls::open_file_readonly(path)?;
        let size = match syscalls::file_size(fd) {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    libc::close(fd);
                }
                return Err(e);
            }
        };
        let content_type = mime_from_path(path);

        Ok(Self {
            status: 200,
            body: Body::File {
                fd: OwnedFd::new(fd),
                offset: 0,
                len: size,
            },
            content_type,
            headers: Headers::new(),
        })
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
