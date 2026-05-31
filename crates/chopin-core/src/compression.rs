//! Response compression middleware.
//!
//! Wraps the inner handler and gzip-encodes the response body when the client
//! sent `Accept-Encoding: gzip` *and* the response is a compressible content
//! type that is large enough to be worth compressing.
//!
//! Behind the `compression` cargo feature (which also enables [`Response::gzip`]).
//!
//! # Usage
//!
//! ```rust,no_run
//! use chopin_core::{Router, compression};
//!
//! let mut router = Router::new();
//! router.layer(compression::gzip);
//! ```
//!
//! # Heuristics
//!
//! - The response is compressed only when `Accept-Encoding` contains `gzip`.
//! - Responses already carrying a `Content-Encoding` header are passed through
//!   (we never double-compress).
//! - Body must be `Body::Bytes` or `Body::Static` (streamed/file bodies have
//!   their own delivery paths).
//! - Compressible content types: `text/*`, `application/json`,
//!   `application/javascript`, `application/xml`, `image/svg+xml`.
//! - Bodies smaller than [`MIN_COMPRESS_SIZE`] bytes are skipped (gzip framing
//!   overhead dwarfs any savings).
//!
//! Set `CHOPIN_GZIP_MIN_SIZE` to override the threshold (default 256 bytes).

use crate::http::{Body, Context, Response};
use crate::router::BoxedHandler;

/// Default minimum body size, in bytes, to attempt gzip compression for.
pub const MIN_COMPRESS_SIZE: usize = 256;

fn min_size() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("CHOPIN_GZIP_MIN_SIZE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(MIN_COMPRESS_SIZE)
    })
}

fn accepts_gzip(ae: &str) -> bool {
    // RFC 7231: comma-separated list of codings, each may have ";q=…".
    ae.split(',').any(|part| {
        let token = part.split(';').next().unwrap_or("").trim();
        token.eq_ignore_ascii_case("gzip")
    })
}

fn is_compressible(content_type: &str) -> bool {
    if content_type.starts_with("text/") {
        return true;
    }
    matches!(
        content_type,
        "application/json"
            | "application/javascript"
            | "application/xml"
            | "application/xhtml+xml"
            | "application/x-yaml"
            | "image/svg+xml"
    )
}

fn body_len(body: &Body) -> Option<usize> {
    match body {
        Body::Static(b) => Some(b.len()),
        Body::Bytes(b) => Some(b.len()),
        _ => None,
    }
}

/// Middleware that conditionally gzip-encodes the response body.
pub fn gzip(ctx: Context, next: BoxedHandler) -> Response {
    let wants_gzip = ctx
        .header("accept-encoding")
        .map(accepts_gzip)
        .unwrap_or(false);
    let resp = next(ctx);

    if !wants_gzip {
        return resp;
    }
    if resp.headers.get("Content-Encoding").is_some() {
        return resp;
    }
    if !is_compressible(resp.content_type) {
        return resp;
    }
    match body_len(&resp.body) {
        Some(n) if n >= min_size() => resp.gzip(),
        _ => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Method, Request};
    use std::sync::Arc;

    fn make_ctx<'a>(accept_encoding: Option<&'a str>) -> Context<'a> {
        let mut headers = [("", ""); crate::http::MAX_HEADERS];
        let mut count = 0u8;
        if let Some(ae) = accept_encoding {
            headers[0] = ("Accept-Encoding", ae);
            count = 1;
        }
        Context {
            req: Request {
                method: Method::Get,
                path: "/",
                query: None,
                headers,
                header_count: count,
                body: &[],
            },
            params: [("", ""); crate::http::MAX_PARAMS],
            param_count: 0,
            peer_addr: [0u8; 16],
        }
    }

    #[test]
    fn accepts_gzip_parses_qvalues() {
        assert!(accepts_gzip("gzip"));
        assert!(accepts_gzip("deflate, gzip;q=0.8"));
        assert!(accepts_gzip("GZIP"));
        assert!(!accepts_gzip("deflate, br"));
    }

    #[test]
    fn skips_without_accept_encoding() {
        let big = vec![b'a'; 1024];
        let ctx = make_ctx(None);
        let handler: BoxedHandler = Arc::new(move |_| Response::json_bytes(big.clone()));
        let resp = gzip(ctx, handler);
        assert!(resp.headers.get("Content-Encoding").is_none());
    }

    #[test]
    fn skips_small_bodies() {
        let ctx = make_ctx(Some("gzip"));
        let handler: BoxedHandler = Arc::new(|_| Response::json_bytes(vec![b'a'; 10]));
        let resp = gzip(ctx, handler);
        assert!(resp.headers.get("Content-Encoding").is_none());
    }

    #[test]
    fn skips_non_compressible_content_type() {
        // application/octet-stream — Response::raw_status preserves it
        let ctx = make_ctx(Some("gzip"));
        let handler: BoxedHandler = Arc::new(|_| {
            let mut r = Response::new(200);
            r.body = Body::Bytes(vec![0u8; 1024]);
            r.content_type = "application/octet-stream";
            r
        });
        let resp = gzip(ctx, handler);
        assert!(resp.headers.get("Content-Encoding").is_none());
    }

    #[test]
    fn compresses_large_json() {
        let body = vec![b'a'; 4096];
        let ctx = make_ctx(Some("gzip"));
        let handler: BoxedHandler = Arc::new(move |_| Response::json_bytes(body.clone()));
        let resp = gzip(ctx, handler);
        assert_eq!(resp.headers.get("Content-Encoding").unwrap(), "gzip");
        assert_eq!(resp.headers.get("Vary").unwrap(), "Accept-Encoding");
    }

    #[test]
    fn does_not_double_encode() {
        let ctx = make_ctx(Some("gzip"));
        let handler: BoxedHandler = Arc::new(|_| {
            Response::json_bytes(vec![b'a'; 4096]).with_header("Content-Encoding", "br")
        });
        let resp = gzip(ctx, handler);
        assert_eq!(resp.headers.get("Content-Encoding").unwrap(), "br");
    }
}
