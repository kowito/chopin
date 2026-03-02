// src/http.rs
use crate::syscalls;
use std::io;

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

pub enum Body {
    Empty,
    Static(&'static [u8]),
    Bytes(Vec<u8>),
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
    /// Zero-copy file body — served via `sendfile()` entirely in kernel space.
    /// The fd is owned and will be closed when the response is consumed or dropped.
    File {
        fd: OwnedFd,
        offset: u64,
        len: u64,
    },
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

pub struct Response {
    pub status: u16,
    pub body: Body,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
}

impl Response {
    /// Create a response with no body and a given status code.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            body: Body::Empty,
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// Builder-style method to append an HTTP response header.
    pub fn with_header(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((key, value.into()));
        self
    }

    /// 200 OK with a plain-text body.
    pub fn text(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 200 OK with a zero-copy static plain-text body.
    /// Avoids heap allocation — ideal for fixed responses like TFB plaintext.
    pub fn text_static(body: &'static [u8]) -> Self {
        Self {
            status: 200,
            body: Body::Static(body),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 200 OK with a pre-serialized JSON byte body.
    /// Use `Response::json()` when you have a typed value to serialize.
    pub fn json_bytes(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "application/json",
            headers: Vec::new(),
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
            headers: Vec::new(),
        }
    }

    /// 500 Internal Server Error.
    pub fn server_error() -> Self {
        Self {
            status: 500,
            body: Body::Static(b"Internal Server Error"),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 400 Bad Request.
    pub fn bad_request() -> Self {
        Self {
            status: 400,
            body: Body::Static(b"Bad Request"),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 401 Unauthorized.
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            body: Body::Static(b"Unauthorized"),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 403 Forbidden.
    pub fn forbidden() -> Self {
        Self {
            status: 403,
            body: Body::Static(b"Forbidden"),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// Chunked streaming response with `application/octet-stream` content type.
    pub fn stream(iter: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Self {
        Self {
            status: 200,
            body: Body::Stream(Box::new(iter)),
            content_type: "application/octet-stream",
            headers: Vec::new(),
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
            headers: Vec::new(),
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
            headers: Vec::new(),
        }
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
