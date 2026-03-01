// src/http.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    Trace,
    Connect,
    Unknown,
}

impl Method {
    pub fn from_bytes(b: &[u8]) -> Self {
        match b {
            b"GET" => Method::Get,
            b"POST" => Method::Post,
            b"PUT" => Method::Put,
            b"DELETE" => Method::Delete,
            b"PATCH" => Method::Patch,
            b"HEAD" => Method::Head,
            b"OPTIONS" => Method::Options,
            b"TRACE" => Method::Trace,
            b"CONNECT" => Method::Connect,
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

pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
}

impl Body {
    pub fn len(&self) -> usize {
        match self {
            Body::Empty => 0,
            Body::Bytes(b) => b.len(),
            Body::Stream(_) => 0, // Chunked has no predefined length
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Body::Empty => &[],
            Body::Bytes(b) => b.as_slice(),
            Body::Stream(_) => &[], // Streams must be polled/chunked iteratively
        }
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
            body: Body::Bytes(b"Not Found".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 500 Internal Server Error.
    pub fn server_error() -> Self {
        Self {
            status: 500,
            body: Body::Bytes(b"Internal Server Error".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 400 Bad Request.
    pub fn bad_request() -> Self {
        Self {
            status: 400,
            body: Body::Bytes(b"Bad Request".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 401 Unauthorized.
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            body: Body::Bytes(b"Unauthorized".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    /// 403 Forbidden.
    pub fn forbidden() -> Self {
        Self {
            status: 403,
            body: Body::Bytes(b"Forbidden".to_vec()),
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
