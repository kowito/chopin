use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get, Post, Put, Delete, Patch, Head, Options, Trace, Connect, Unknown,
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

pub struct Request<'a> {
    pub method: Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: Vec<(&'a str, &'a str)>,
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
    pub fn new(status: u16) -> Self {
        Self {
            status,
            body: Body::Empty,
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    pub fn header(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((key, value.into()));
        self
    }
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: Body::Bytes(body.into()),
            content_type: "application/json",
            headers: Vec::new(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            body: Body::Bytes(b"Not Found".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    pub fn internal_error() -> Self {
        Self {
            status: 500,
            body: Body::Bytes(b"Internal Server Error".to_vec()),
            content_type: "text/plain",
            headers: Vec::new(),
        }
    }

    pub fn stream(iter: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Self {
        Self {
            status: 200,
            body: Body::Stream(Box::new(iter)),
            content_type: "application/octet-stream",
            headers: Vec::new(),
        }
    }
}

pub struct Context<'a> {
    pub req: Request<'a>,
    pub params: HashMap<String, String>, // Dynamic route parameters
}

impl<'a> Context<'a> {
    pub fn extract<T: crate::extract::FromRequest<'a>>(&'a self) -> Result<T, T::Error> {
        T::from_request(self)
    }
}
