//! CORS (Cross-Origin Resource Sharing) middleware.
//!
//! Implements [the Fetch CORS protocol](https://fetch.spec.whatwg.org/#http-cors-protocol)
//! at the middleware layer. Preflight `OPTIONS` requests are short-circuited
//! with a `204 No Content` and the appropriate `Access-Control-*` headers;
//! simple and "actual" requests pass through to the inner handler and have
//! the relevant CORS response headers appended.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use chopin_core::{Router, cors::Cors};
//!
//! let cors = Cors::permissive(); // any origin, common methods/headers
//! let mut router = Router::new();
//! router.layer_fn(cors.into_middleware());
//! ```
//!
//! # Restrictive example
//!
//! ```rust,no_run
//! use chopin_core::{Router, cors::Cors};
//!
//! let cors = Cors::new()
//!     .allow_origin("https://app.example.com")
//!     .allow_methods(&["GET", "POST"])
//!     .allow_headers(&["Content-Type", "Authorization"])
//!     .allow_credentials(true)
//!     .max_age(3600);
//!
//! let mut router = Router::new();
//! router.layer_fn(cors.into_middleware());
//! ```

use crate::http::{Context, Method, Response};
use crate::router::BoxedHandler;
use std::sync::Arc;

/// Allowed-origin policy.
#[derive(Clone, Debug)]
enum OriginPolicy {
    /// Reflect any origin (sends `Access-Control-Allow-Origin: *`).
    Any,
    /// Reflect the request origin if it matches one of these values.
    /// Stored lowercased for case-insensitive comparison of the scheme/host parts.
    List(Vec<String>),
}

/// CORS configuration. Build with [`Cors::new`] (deny-by-default) or
/// [`Cors::permissive`], then call [`Cors::into_middleware`] to attach.
#[derive(Clone, Debug)]
pub struct Cors {
    origins: OriginPolicy,
    methods: String,
    headers: String,
    expose_headers: String,
    allow_credentials: bool,
    max_age: Option<u32>,
}

impl Default for Cors {
    fn default() -> Self {
        Self::new()
    }
}

impl Cors {
    /// Deny-by-default configuration. No origin is allowed until
    /// [`allow_origin`](Self::allow_origin) or [`allow_any_origin`](Self::allow_any_origin)
    /// is called.
    pub fn new() -> Self {
        Self {
            origins: OriginPolicy::List(Vec::new()),
            methods: "GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD".into(),
            headers: "Content-Type, Authorization".into(),
            expose_headers: String::new(),
            allow_credentials: false,
            max_age: None,
        }
    }

    /// Permissive preset: any origin, common methods and headers, no credentials.
    /// Suitable for public APIs that don't rely on cookies.
    pub fn permissive() -> Self {
        Self {
            origins: OriginPolicy::Any,
            methods: "GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD".into(),
            headers: "Content-Type, Authorization, X-Requested-With".into(),
            expose_headers: String::new(),
            allow_credentials: false,
            max_age: Some(3600),
        }
    }

    /// Reflect any origin. Incompatible with `allow_credentials(true)` — the
    /// CORS spec forbids the `*` wildcard with credentials. When both are set,
    /// the request's `Origin` header is echoed back verbatim instead.
    pub fn allow_any_origin(mut self) -> Self {
        self.origins = OriginPolicy::Any;
        self
    }

    /// Add an allowed origin (exact, case-insensitive match on the full URL).
    pub fn allow_origin(mut self, origin: impl Into<String>) -> Self {
        let lower = origin.into().to_ascii_lowercase();
        match &mut self.origins {
            OriginPolicy::List(v) => v.push(lower),
            OriginPolicy::Any => {
                self.origins = OriginPolicy::List(vec![lower]);
            }
        }
        self
    }

    /// Replace the `Access-Control-Allow-Methods` header value.
    pub fn allow_methods(mut self, methods: &[&str]) -> Self {
        self.methods = methods.join(", ");
        self
    }

    /// Replace the `Access-Control-Allow-Headers` header value.
    pub fn allow_headers(mut self, headers: &[&str]) -> Self {
        self.headers = headers.join(", ");
        self
    }

    /// Set the `Access-Control-Expose-Headers` value (response headers the
    /// browser may expose to JS).
    pub fn expose_headers(mut self, headers: &[&str]) -> Self {
        self.expose_headers = headers.join(", ");
        self
    }

    /// Enable `Access-Control-Allow-Credentials: true`. When enabled, the
    /// wildcard `*` origin is replaced by an echoed request origin.
    pub fn allow_credentials(mut self, yes: bool) -> Self {
        self.allow_credentials = yes;
        self
    }

    /// Preflight cache duration in seconds (`Access-Control-Max-Age`).
    pub fn max_age(mut self, secs: u32) -> Self {
        self.max_age = Some(secs);
        self
    }

    /// Decide which value to send for `Access-Control-Allow-Origin`.
    fn resolve_allow_origin(&self, req_origin: Option<&str>) -> Option<String> {
        match &self.origins {
            OriginPolicy::Any => {
                if self.allow_credentials {
                    // Must echo a concrete origin when credentials are allowed.
                    req_origin.map(|s| s.to_string())
                } else {
                    Some("*".into())
                }
            }
            OriginPolicy::List(allowed) => {
                let o = req_origin?.to_ascii_lowercase();
                if allowed.iter().any(|a| *a == o) {
                    req_origin.map(|s| s.to_string())
                } else {
                    None
                }
            }
        }
    }

    fn apply_to(&self, mut resp: Response, req_origin: Option<&str>) -> Response {
        let Some(allow_origin) = self.resolve_allow_origin(req_origin) else {
            return resp;
        };
        resp = resp.with_header("Access-Control-Allow-Origin", allow_origin);
        // Per spec, when the allowed origin is not `*`, vary on Origin so caches
        // don't serve the wrong response to a different origin.
        if !matches!(self.origins, OriginPolicy::Any) || self.allow_credentials {
            resp = resp.with_header("Vary", "Origin");
        }
        if self.allow_credentials {
            resp = resp.with_header("Access-Control-Allow-Credentials", "true");
        }
        if !self.expose_headers.is_empty() {
            resp = resp.with_header(
                "Access-Control-Expose-Headers",
                self.expose_headers.clone(),
            );
        }
        resp
    }

    fn preflight(&self, req_origin: Option<&str>) -> Response {
        let mut resp = Response::new(204);
        resp = resp.with_header("Access-Control-Allow-Methods", self.methods.clone());
        resp = resp.with_header("Access-Control-Allow-Headers", self.headers.clone());
        if let Some(age) = self.max_age {
            resp = resp.with_header("Access-Control-Max-Age", age as u64);
        }
        self.apply_to(resp, req_origin)
    }

    /// Convert this config into a middleware closure suitable for
    /// `Router::layer_fn` / `Chopin::layer_fn`.
    pub fn into_middleware(
        self,
    ) -> impl Fn(Context, BoxedHandler) -> Response + Send + Sync + 'static {
        let cfg = Arc::new(self);
        move |ctx, next| {
            let origin = ctx.header("origin").map(|s| s.to_string());
            // Preflight: OPTIONS request with Access-Control-Request-Method header.
            if matches!(ctx.req.method, Method::Options)
                && ctx.header("access-control-request-method").is_some()
            {
                return cfg.preflight(origin.as_deref());
            }
            let resp = next(ctx);
            cfg.apply_to(resp, origin.as_deref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_sends_wildcard_origin() {
        let cors = Cors::permissive();
        let resp = cors.apply_to(Response::new(200), Some("https://example.com"));
        let v = resp.headers.get("Access-Control-Allow-Origin").unwrap();
        assert_eq!(v, "*");
    }

    #[test]
    fn list_origin_match_echoes_request_origin() {
        let cors = Cors::new().allow_origin("https://app.example.com");
        let resp = cors.apply_to(Response::new(200), Some("https://app.example.com"));
        let v = resp.headers.get("Access-Control-Allow-Origin").unwrap();
        assert_eq!(v, "https://app.example.com");
        assert_eq!(resp.headers.get("Vary").unwrap(), "Origin");
    }

    #[test]
    fn list_origin_mismatch_omits_header() {
        let cors = Cors::new().allow_origin("https://app.example.com");
        let resp = cors.apply_to(Response::new(200), Some("https://evil.example.com"));
        assert!(resp.headers.get("Access-Control-Allow-Origin").is_none());
    }

    #[test]
    fn credentials_forces_concrete_origin_even_when_any() {
        let cors = Cors::permissive().allow_credentials(true);
        let resp = cors.apply_to(Response::new(200), Some("https://x.test"));
        assert_eq!(
            resp.headers.get("Access-Control-Allow-Origin").unwrap(),
            "https://x.test"
        );
        assert_eq!(
            resp.headers.get("Access-Control-Allow-Credentials").unwrap(),
            "true"
        );
    }

    #[test]
    fn preflight_includes_methods_headers_max_age() {
        let cors = Cors::permissive()
            .allow_methods(&["GET", "POST"])
            .allow_headers(&["X-Test"])
            .max_age(600);
        let resp = cors.preflight(Some("https://x.test"));
        assert_eq!(resp.status, 204);
        assert_eq!(
            resp.headers.get("Access-Control-Allow-Methods").unwrap(),
            "GET, POST"
        );
        assert_eq!(
            resp.headers.get("Access-Control-Allow-Headers").unwrap(),
            "X-Test"
        );
        assert_eq!(resp.headers.get("Access-Control-Max-Age").unwrap(), "600");
    }
}
