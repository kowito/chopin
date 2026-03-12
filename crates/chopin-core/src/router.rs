// src/router.rs
use crate::http::{Context, MAX_PARAMS, Method, Response};
use std::sync::Arc;

/// A route handler — a plain function pointer taking a [`Context`] and returning a [`Response`].
pub type Handler = fn(Context) -> Response;

/// A boxed handler used internally for middleware composition.
pub type BoxedHandler = Arc<dyn Fn(Context) -> Response + Send + Sync>;

/// A middleware function. Receives the request context and the next handler in the chain.
pub type MiddlewareFn = fn(Context, BoxedHandler) -> Response;

pub const MAX_MIDDLEWARE: usize = 8;
pub const MAX_SEGMENTS: usize = 16;
const METHOD_COUNT: usize = 10;

/// Result of a successful route match.
pub type RouteMatch<'a> = (
    &'a Handler,
    [(&'a str, &'a str); MAX_PARAMS],
    u8,
    Option<&'a BoxedHandler>, // pre-composed middleware chain; None when no middleware
);

/// A static route definition registered via `#[get("/path")]` and friends.
///
/// Collected at link time using the `inventory` crate — no manual registration needed.
#[derive(Clone, Copy)]
pub struct RouteDef {
    pub method: Method,
    pub path: &'static str,
    pub handler: Handler,
    pub summary: &'static str,
    pub description: &'static str,
}

inventory::collect!(RouteDef);

#[derive(Clone)]
pub(crate) struct RouteNode {
    pub(crate) path: String,
    pub(crate) handlers: [Option<Handler>; METHOD_COUNT],
    pub(crate) children: Vec<RouteNode>,
    pub(crate) is_param: bool,
    pub(crate) param_name: Option<String>,
    pub(crate) is_wildcard: bool,
    pub(crate) middleware: Vec<MiddlewareFn>,
    pub(crate) composed_handlers: [Option<BoxedHandler>; METHOD_COUNT],
}

impl RouteNode {
    pub(crate) fn new(path: String) -> Self {
        Self {
            path,
            handlers: [None; METHOD_COUNT],
            children: Vec::new(),
            is_param: false,
            param_name: None,
            is_wildcard: false,
            middleware: Vec::new(),
            composed_handlers: std::array::from_fn(|_| None),
        }
    }
}

/// Map Method enum to array index for O(1) dispatch
#[inline(always)]
fn method_index(m: Method) -> usize {
    m as usize
}

/// A trie-based HTTP router with O(path-depth) matching.
///
/// Supports static segments, named parameters (`:id`), wildcards (`*path`),
/// and per-route or global middleware.
///
/// # Example
///
/// ```rust,ignore
/// use chopin_core::{Router, Method, Context, Response};
///
/// fn hello(_ctx: Context) -> Response {
///     Response::text("hello")
/// }
///
/// let mut router = Router::new();
/// router.get("/hello", hello);
/// ```
#[derive(Clone)]
pub struct Router {
    pub(crate) root: RouteNode,
    pub(crate) global_middleware: Vec<MiddlewareFn>,
}

impl Router {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            root: RouteNode::new(String::new()),
            global_middleware: Vec::new(),
        }
    }

    /// Register a handler for the given HTTP method and path.
    ///
    /// Path segments starting with `:` are captured as named parameters;
    /// segments starting with `*` match the rest of the URL.
    pub fn add(&mut self, method: Method, path: &str, handler: Handler) {
        // Stack-allocated segments — no Vec heap allocation
        let mut seg_buf = [""; MAX_SEGMENTS];
        let mut seg_count = 0;
        for s in path.split('/').filter(|s| !s.is_empty()) {
            if seg_count >= MAX_SEGMENTS {
                break;
            }
            seg_buf[seg_count] = s;
            seg_count += 1;
        }
        let mut current = &mut self.root;

        for &segment in &seg_buf[..seg_count] {
            // Check if segment is a param or wildcard
            let is_param = segment.starts_with(':');
            let is_wildcard = segment.starts_with('*');

            let param_name = if is_param || is_wildcard {
                Some(segment[1..].to_string())
            } else {
                None
            };

            let segment_path = if is_param || is_wildcard {
                String::new()
            } else {
                segment.to_string()
            };

            // Find or create child
            let mut found_idx = None;
            for (i, child) in current.children.iter().enumerate() {
                if child.is_param == is_param
                    && child.is_wildcard == is_wildcard
                    && (is_param || is_wildcard || child.path == segment_path)
                {
                    found_idx = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_idx {
                current = &mut current.children[idx];
            } else {
                let mut new_node = RouteNode::new(segment_path);
                new_node.is_param = is_param;
                new_node.param_name = param_name;
                new_node.is_wildcard = is_wildcard;
                current.children.push(new_node);
                current = current.children.last_mut().unwrap();
            }
        }

        current.handlers[method_index(method)] = Some(handler);
    }

    /// Look up a handler for the given method and URL path.
    ///
    /// Returns the matched handler, captured path parameters, parameter count,
    /// and an optional pre-composed middleware chain.
    #[inline]
    pub fn match_route<'a>(&'a self, method: Method, path: &'a str) -> Option<RouteMatch<'a>> {
        // Fast path for root "/" — skip segment splitting entirely
        if path == "/" || path.is_empty() {
            let idx = method_index(method);
            if let Some(ref h) = self.root.handlers[idx] {
                return Some((
                    h,
                    [("", ""); MAX_PARAMS],
                    0,
                    self.root.composed_handlers[idx].as_ref(),
                ));
            }
            return None;
        }

        // Stack-allocated segments — no heap allocation
        let mut segments = [""; MAX_SEGMENTS];
        let mut seg_count: usize = 0;
        for s in path.split('/').filter(|s| !s.is_empty()) {
            if seg_count >= MAX_SEGMENTS {
                return None; // Path too deep
            }
            segments[seg_count] = s;
            seg_count += 1;
        }
        let segments = &segments[..seg_count];

        let mut params = [("", ""); MAX_PARAMS];
        let mut param_count: u8 = 0;

        let result = self.match_recursive(
            &self.root,
            method,
            segments,
            0,
            &mut params,
            &mut param_count,
        );
        result.map(|(h, c)| (h, params, param_count, c))
    }

    #[allow(clippy::collapsible_if)]
    #[inline(always)]
    fn match_recursive<'a, 'b>(
        &'a self,
        node: &'a RouteNode,
        method: Method,
        segments: &[&'b str],
        depth: usize,
        params: &mut [(&'b str, &'b str); MAX_PARAMS],
        param_count: &mut u8,
    ) -> Option<(&'a Handler, Option<&'a BoxedHandler>)>
    where
        'a: 'b,
    {
        if depth == segments.len() {
            let idx = method_index(method);
            if let Some(ref h) = node.handlers[idx] {
                return Some((h, node.composed_handlers[idx].as_ref()));
            }
            return None;
        }

        let segment = segments[depth];

        // Count static children (they are sorted first after finalize())
        let static_end = node
            .children
            .partition_point(|c| !c.is_param && !c.is_wildcard);

        // Binary search over sorted static children for O(log n) exact match
        if static_end > 0 {
            let statics = &node.children[..static_end];
            if let Ok(idx) = statics.binary_search_by(|c| c.path.as_str().cmp(segment)) {
                if let Some(result) = self.match_recursive(
                    &statics[idx],
                    method,
                    segments,
                    depth + 1,
                    params,
                    param_count,
                ) {
                    return Some(result);
                }
            }
        }

        // Try param match (children after static_end)
        for child in &node.children[static_end..] {
            if child.is_param {
                let old_count = *param_count;
                if (*param_count as usize) < MAX_PARAMS {
                    if let Some(ref name) = child.param_name {
                        params[*param_count as usize] = (name.as_str(), segment);
                        *param_count += 1;
                    }
                }
                if let Some(result) =
                    self.match_recursive(child, method, segments, depth + 1, params, param_count)
                {
                    return Some(result);
                }
                // Backtrack
                *param_count = old_count;
            }
        }

        // Try wildcard match (after params in sorted order)
        for child in &node.children[static_end..] {
            if child.is_wildcard {
                if (*param_count as usize) < MAX_PARAMS {
                    if let Some(ref name) = child.param_name {
                        params[*param_count as usize] = (name.as_str(), segment);
                        *param_count += 1;
                    }
                }
                let idx = method_index(method);
                if let Some(ref h) = child.handlers[idx] {
                    return Some((h, child.composed_handlers[idx].as_ref()));
                }
            }
        }

        None
    }

    // ── Middleware registration ───────────────────────────────────────────────

    /// Apply a middleware function to every route in this router (global layer).
    pub fn layer(&mut self, mw: MiddlewareFn) {
        self.global_middleware.push(mw);
    }

    /// Apply a middleware function to all routes under the given path prefix.
    pub fn layer_path(&mut self, path: &str, mw: MiddlewareFn) {
        let mut seg_buf = [""; MAX_SEGMENTS];
        let mut seg_count = 0;
        for s in path.split('/').filter(|s| !s.is_empty()) {
            if seg_count >= MAX_SEGMENTS {
                break;
            }
            seg_buf[seg_count] = s;
            seg_count += 1;
        }
        let mut current = &mut self.root;

        for &segment in &seg_buf[..seg_count] {
            let is_param = segment.starts_with(':');
            let is_wildcard = segment.starts_with('*');

            let param_name = if is_param || is_wildcard {
                Some(segment[1..].to_string())
            } else {
                None
            };

            let segment_str = if is_param || is_wildcard {
                String::new()
            } else {
                segment.to_string()
            };

            let mut found_idx = None;
            for (i, child) in current.children.iter().enumerate() {
                if child.is_param == is_param
                    && child.is_wildcard == is_wildcard
                    && (is_param || is_wildcard || child.path == segment_str)
                {
                    found_idx = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_idx {
                current = &mut current.children[idx];
            } else {
                let mut new_node = RouteNode::new(segment_str);
                new_node.is_param = is_param;
                new_node.param_name = param_name;
                new_node.is_wildcard = is_wildcard;
                current.children.push(new_node);
                current = current.children.last_mut().unwrap();
            }
        }

        current.middleware.push(mw);
    }

    // ── Modular Routing ───────────────────────────────────────────────────────
    /// Merge all routes from `other` into this router.
    #[must_use]
    pub fn merge(mut self, other: Router) -> Self {
        Self::merge_nodes(&mut self.root, other.root);
        self.global_middleware.extend(other.global_middleware);
        self
    }

    /// Mount `other` under `prefix`, e.g. `router.nest("/api/v1", api_router)`.
    #[must_use]
    pub fn nest(mut self, prefix: &str, other: Router) -> Self {
        let mut seg_buf = [""; MAX_SEGMENTS];
        let mut seg_count = 0;
        for s in prefix.split('/').filter(|s| !s.is_empty()) {
            if seg_count >= MAX_SEGMENTS {
                break;
            }
            seg_buf[seg_count] = s;
            seg_count += 1;
        }
        let mut current = &mut self.root;

        for &segment in &seg_buf[..seg_count] {
            let is_param = segment.starts_with(':');
            let is_wildcard = segment.starts_with('*');

            let param_name = if is_param || is_wildcard {
                Some(segment[1..].to_string())
            } else {
                None
            };

            let segment_str = if is_param || is_wildcard {
                String::new()
            } else {
                segment.to_string()
            };

            let mut found_idx = None;
            for (i, child) in current.children.iter().enumerate() {
                if child.is_param == is_param
                    && child.is_wildcard == is_wildcard
                    && (is_param || is_wildcard || child.path == segment_str)
                {
                    found_idx = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_idx {
                current = &mut current.children[idx];
            } else {
                let mut new_node = RouteNode::new(segment_str);
                new_node.is_param = is_param;
                new_node.param_name = param_name;
                new_node.is_wildcard = is_wildcard;
                current.children.push(new_node);
                current = current.children.last_mut().unwrap();
            }
        }

        Self::merge_nodes(current, other.root);
        current.middleware.extend(other.global_middleware);
        self
    }

    fn merge_nodes(target: &mut RouteNode, mut source: RouteNode) {
        // Merge handlers (array-based)
        for i in 0..METHOD_COUNT {
            if source.handlers[i].is_some() {
                target.handlers[i] = source.handlers[i];
            }
        }

        target.middleware.append(&mut source.middleware);

        // Merge children
        for source_child in source.children {
            let mut found_idx = None;
            for (i, target_child) in target.children.iter().enumerate() {
                if target_child.is_param == source_child.is_param
                    && target_child.is_wildcard == source_child.is_wildcard
                    && (source_child.is_param
                        || source_child.is_wildcard
                        || target_child.path == source_child.path)
                {
                    found_idx = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_idx {
                Self::merge_nodes(&mut target.children[idx], source_child);
            } else {
                target.children.push(source_child);
            }
        }
    }

    /// Prepare the router for production use. Sorts static children at every
    /// level so `match_recursive` can binary-search instead of linear-scan.
    /// Pre-composes middleware chains so the hot path pays zero `Arc::new` cost.
    /// Call once after all routes are registered, before serving.
    pub fn finalize(&mut self) {
        Self::sort_children_recursive(&mut self.root);
        let global_mw: Vec<MiddlewareFn> = self.global_middleware.clone();
        Self::compose_tree(&mut self.root, &global_mw, &[]);
    }

    fn sort_children_recursive(node: &mut RouteNode) {
        // Partition: static children first (sorted by path), then params, then wildcards.
        // This lets match_recursive do a binary search over the static prefix.
        node.children.sort_by(|a, b| {
            match (a.is_param || a.is_wildcard, b.is_param || b.is_wildcard) {
                (false, false) => a.path.cmp(&b.path), // both static → sort lexically
                (false, true) => std::cmp::Ordering::Less, // static before dynamic
                (true, false) => std::cmp::Ordering::Greater,
                (true, true) => {
                    // params before wildcards
                    a.is_wildcard.cmp(&b.is_wildcard)
                }
            }
        });
        for child in &mut node.children {
            Self::sort_children_recursive(child);
        }
    }

    /// Pre-compose middleware chains at `finalize()` time so the hot request
    /// path pays zero `Arc::new` cost — just a single `Arc` clone (~3 ns).
    /// `ancestor_mw` carries route-level middleware accumulated while descending
    /// from the root, mirroring the original `match_recursive` accumulation.
    #[allow(clippy::collapsible_if)]
    fn compose_tree(
        node: &mut RouteNode,
        global_mw: &[MiddlewareFn],
        ancestor_mw: &[MiddlewareFn],
    ) {
        // Effective route middleware for handlers at this node =
        // ancestor_mw (outer) ++ node.middleware (inner)
        let mut node_route_mw: Vec<MiddlewareFn> = ancestor_mw.to_vec();
        node_route_mw.extend_from_slice(&node.middleware);

        for method_idx in 0..METHOD_COUNT {
            if let Some(handler) = node.handlers[method_idx] {
                if !node_route_mw.is_empty() || !global_mw.is_empty() {
                    let mut composed: BoxedHandler = Arc::new(handler);
                    // Route-level middleware (innermost → outermost, matching original)
                    for mw in node_route_mw.iter().rev() {
                        let next = composed.clone();
                        let mw = *mw;
                        composed = Arc::new(move |ctx| mw(ctx, next.clone()));
                    }
                    // Global middleware (outermost layer)
                    for mw in global_mw.iter().rev() {
                        let next = composed.clone();
                        let mw = *mw;
                        composed = Arc::new(move |ctx| mw(ctx, next.clone()));
                    }
                    node.composed_handlers[method_idx] = Some(composed);
                }
            }
        }
        for child in &mut node.children {
            Self::compose_tree(child, global_mw, &node_route_mw);
        }
    }

    // Convenience methods for common HTTP methods.
    /// Register a `GET` handler.
    pub fn get(&mut self, path: &str, handler: Handler) {
        self.add(Method::Get, path, handler);
    }
    /// Register a `POST` handler.
    pub fn post(&mut self, path: &str, handler: Handler) {
        self.add(Method::Post, path, handler);
    }
    /// Register a `PUT` handler.
    pub fn put(&mut self, path: &str, handler: Handler) {
        self.add(Method::Put, path, handler);
    }
    /// Register a `DELETE` handler.
    pub fn delete(&mut self, path: &str, handler: Handler) {
        self.add(Method::Delete, path, handler);
    }
    /// Register a `PATCH` handler.
    pub fn patch(&mut self, path: &str, handler: Handler) {
        self.add(Method::Patch, path, handler);
    }
    /// Register a `HEAD` handler.
    pub fn head(&mut self, path: &str, handler: Handler) {
        self.add(Method::Head, path, handler);
    }
    /// Register an `OPTIONS` handler.
    pub fn options(&mut self, path: &str, handler: Handler) {
        self.add(Method::Options, path, handler);
    }
    /// Register a `TRACE` handler.
    pub fn trace(&mut self, path: &str, handler: Handler) {
        self.add(Method::Trace, path, handler);
    }
    pub fn connect(&mut self, path: &str, handler: Handler) {
        self.add(Method::Connect, path, handler);
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handler(ctx: Context) -> Response {
        Response::text(ctx.req.path.to_string())
    }

    #[test]
    fn test_router_static() {
        let mut router = Router::new();
        router.get("/hello/world", test_handler);
        router.finalize();

        assert!(router.match_route(Method::Get, "/hello/world").is_some());
        assert!(router.match_route(Method::Get, "/hello").is_none());
        assert!(router.match_route(Method::Post, "/hello/world").is_none());
    }

    #[test]
    fn test_router_params() {
        let mut router = Router::new();
        router.get("/users/:id", test_handler);
        router.post("/users/:id/posts/:post_id", test_handler);
        router.finalize();

        let match1 = router.match_route(Method::Get, "/users/123");
        assert!(match1.is_some());
        let (_, params1, _, _) = match1.unwrap();
        assert_eq!(
            params1
                .iter()
                .find(|(k, _)| *k == "id")
                .map(|(_, v)| *v)
                .unwrap(),
            "123"
        );

        let match2 = router.match_route(Method::Post, "/users/123/posts/abc");
        assert!(match2.is_some());
        let (_, params2, _, _) = match2.unwrap();
        assert_eq!(
            params2
                .iter()
                .find(|(k, _)| *k == "id")
                .map(|(_, v)| *v)
                .unwrap(),
            "123"
        );
        assert_eq!(
            params2
                .iter()
                .find(|(k, _)| *k == "post_id")
                .map(|(_, v)| *v)
                .unwrap(),
            "abc"
        );
    }

    #[test]
    fn test_router_wildcard() {
        let mut router = Router::new();
        router.get("/assets/*path", test_handler);
        router.finalize();

        let match1 = router.match_route(Method::Get, "/assets/js/app.js");
        assert!(match1.is_some());
        let (_, params1, _, _) = match1.unwrap();
        assert_eq!(
            params1
                .iter()
                .find(|(k, _)| *k == "path")
                .map(|(_, v)| *v)
                .unwrap(),
            "js"
        );
    }

    #[test]
    fn test_router_nest() {
        let mut auth_router = Router::new();
        auth_router.post("/login", test_handler);

        let mut api_router = Router::new();
        api_router.get("/status", test_handler);
        api_router = api_router.nest("/auth", auth_router);

        let mut root = Router::new();
        root = root.nest("/api/v1", api_router);
        root.finalize();

        assert!(
            root.match_route(Method::Post, "/api/v1/auth/login")
                .is_some()
        );
        assert!(root.match_route(Method::Get, "/api/v1/status").is_some());
        assert!(
            root.match_route(Method::Get, "/api/v1/auth/login")
                .is_none()
        );
    }

    fn dummy_middleware(ctx: Context, next: BoxedHandler) -> Response {
        let mut r = next(ctx);
        r.headers.add("X-Middleware", "1");
        r
    }

    #[test]
    fn test_nested_middleware() {
        let mut auth_router = Router::new();
        auth_router.layer(dummy_middleware);
        auth_router.post("/login", test_handler);

        let mut root = Router::new();
        root = root.nest("/api", auth_router);
        root.get("/status", test_handler);
        root.finalize();

        let m1 = root.match_route(Method::Post, "/api/login");
        assert!(m1.is_some());
        assert!(m1.unwrap().3.is_some()); // middleware composed → Some(BoxedHandler)

        let m2 = root.match_route(Method::Get, "/status");
        assert!(m2.is_some());
        assert!(m2.unwrap().3.is_none()); // no middleware → None
    }

    #[test]
    fn test_router_merge() {
        let mut r1 = Router::new();
        r1.get("/r1", test_handler);

        let mut r2 = Router::new();
        r2.get("/r2", test_handler);

        let merged = r1.merge(r2);
        // Note: finalize not needed here — merge test just checks existence.
        // But let's finalize for consistency with production path.
        let mut merged = merged;
        merged.finalize();

        assert!(merged.match_route(Method::Get, "/r1").is_some());
        assert!(merged.match_route(Method::Get, "/r2").is_some());
    }
}
