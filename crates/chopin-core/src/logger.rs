//! Request logger middleware: method, path, status, latency.
//!
//! Emits one line per request via the `tracing` crate at `INFO` level. When
//! the `logging` feature is not enabled the call is a no-op, but the
//! middleware still records latency so downstream layers can read it from
//! the response if needed.
//!
//! # Usage
//!
//! ```rust,no_run
//! use chopin_core::{Router, logger};
//!
//! let mut router = Router::new();
//! router.layer(logger::request_log);
//! ```

use crate::http::{Context, Response};
use crate::router::BoxedHandler;
use std::time::Instant;

/// Standard `fn`-pointer middleware that times every request and emits a log
/// line on completion.
pub fn request_log(ctx: Context, next: BoxedHandler) -> Response {
    let start = Instant::now();
    let method = ctx.req.method;
    // SAFETY: `path` borrows from the connection's read buffer that outlives
    // this stack frame. Copy into a small `String` so we can log after `next`
    // consumes `ctx`.
    let path = ctx.req.path.to_string();
    let resp = next(ctx);
    let elapsed = start.elapsed();
    tracing::info!(
        target: "chopin::request",
        method = ?method,
        path = %path,
        status = resp.status,
        latency_us = elapsed.as_micros() as u64,
        "request"
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Method, Request};
    use std::sync::Arc;

    fn make_ctx<'a>(method: Method, path: &'a str) -> Context<'a> {
        Context {
            req: Request {
                method,
                path,
                query: None,
                headers: [("", ""); crate::http::MAX_HEADERS],
                header_count: 0,
                body: &[],
            },
            params: [("", ""); crate::http::MAX_PARAMS],
            param_count: 0,
            peer_addr: [0u8; 16],
        }
    }

    #[test]
    fn middleware_passes_through_response() {
        let ctx = make_ctx(Method::Get, "/health");
        let handler: BoxedHandler = Arc::new(|_ctx| Response::new(200));
        let resp = request_log(ctx, handler);
        assert_eq!(resp.status, 200);
    }
}
