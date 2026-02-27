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

    pub fn json_fast<T: kowito_json::serialize::Serialize>(val: &T) -> Self {
        let mut buf = Vec::with_capacity(128); // Initial small buffer
        val.serialize(&mut buf);
        Self::json(buf)
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

    pub fn bad_request() -> Self {
        Self {
            status: 400,
            body: Body::Bytes(b"Bad Request".to_vec()),
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
    pub params: [(&'a str, &'a str); MAX_PARAMS],
    pub param_count: u8,
}

impl<'a> Context<'a> {
    pub fn get_param(&self, key: &str) -> Option<&'a str> {
        for i in 0..self.param_count as usize {
            if self.params[i].0 == key {
                return Some(self.params[i].1);
            }
        }
        None
    }

    pub fn get_header(&self, key: &str) -> Option<&'a str> {
        for i in 0..self.req.header_count as usize {
            if self.req.headers[i].0.eq_ignore_ascii_case(key) {
                return Some(self.req.headers[i].1);
            }
        }
        None
    }

    pub fn multipart(&self) -> Option<crate::multipart::Multipart<'a>> {
        let ct = self.get_header("content-type")?;
        if ct.starts_with("multipart/form-data")
            && let Some(idx) = ct.find("boundary=")
        {
            let boundary = &ct[idx + 9..];
            return Some(crate::multipart::Multipart::new(self.req.body, boundary));
        }
        None
    }

    pub fn extract<T: crate::extract::FromRequest<'a>>(&'a self) -> Result<T, T::Error> {
        T::from_request(self)
    }

    pub fn respond_json<T: crate::json::Serialize>(&self, val: &T) -> Response {
        Response::json_fast(val)
    }
}
