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

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
}

impl Response {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            content_type: "text/plain",
        }
    }

    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            content_type: "application/json",
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            body: b"Not Found".to_vec(),
            content_type: "text/plain",
        }
    }
}

pub struct Context<'a> {
    pub req: Request<'a>,
    pub params: HashMap<String, String>, // Dynamic route parameters
}
