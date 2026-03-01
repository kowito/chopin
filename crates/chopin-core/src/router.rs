// src/router.rs
use crate::http::{Context, MAX_PARAMS, Method, Response};
use std::collections::HashMap;
use std::sync::Arc;

pub type Handler = fn(Context) -> Response;

pub type BoxedHandler = Arc<dyn Fn(Context) -> Response + Send + Sync>;

pub type MiddlewareFn = fn(Context, BoxedHandler) -> Response;

/// Result of a successful route match.
pub type RouteMatch<'a> = (
    &'a Handler,
    [(&'a str, &'a str); MAX_PARAMS],
    u8,
    Vec<MiddlewareFn>,
);

#[derive(Clone, Copy)]
pub struct RouteDef {
    pub method: Method,
    pub path: &'static str,
    pub handler: Handler,
}

inventory::collect!(RouteDef);

#[derive(Clone)]
pub(crate) struct RouteNode {
    pub(crate) path: String,
    pub(crate) handlers: HashMap<Method, Handler>,
    pub(crate) children: Vec<RouteNode>,
    pub(crate) is_param: bool,
    pub(crate) param_name: Option<String>,
    pub(crate) is_wildcard: bool,
    pub(crate) middleware: Vec<MiddlewareFn>,
}

impl RouteNode {
    pub(crate) fn new(path: String) -> Self {
        Self {
            path,
            handlers: HashMap::new(),
            children: Vec::new(),
            is_param: false,
            param_name: None,
            is_wildcard: false,
            middleware: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Router {
    pub(crate) root: RouteNode,
    pub(crate) global_middleware: Vec<MiddlewareFn>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            root: RouteNode::new(String::new()),
            global_middleware: Vec::new(),
        }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for segment in segments {
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

        current.handlers.insert(method, handler);
    }

    pub fn match_route<'a>(&'a self, method: Method, path: &'a str) -> Option<RouteMatch<'a>> {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut params = [("", ""); MAX_PARAMS];
        let mut param_count: u8 = 0;
        let mut middleware = Vec::new();

        let handler = self.match_recursive(
            &self.root,
            method,
            &segments,
            0,
            &mut params,
            &mut param_count,
            &mut middleware,
        );
        handler.map(|h| (h, params, param_count, middleware))
    }

    #[allow(clippy::too_many_arguments)]
    fn match_recursive<'a, 'b>(
        &'a self,
        node: &'a RouteNode,
        method: Method,
        segments: &[&'b str],
        depth: usize,
        params: &mut [(&'b str, &'b str); MAX_PARAMS],
        param_count: &mut u8,
        middleware: &mut Vec<MiddlewareFn>,
    ) -> Option<&'a Handler>
    where
        'a: 'b,
    {
        let mw_start_len = middleware.len();
        middleware.extend_from_slice(&node.middleware);

        if depth == segments.len() {
            if let Some(h) = node.handlers.get(&method) {
                return Some(h);
            }
            middleware.truncate(mw_start_len);
            return None;
        }

        let segment = segments[depth];

        // Try exact match first
        for child in &node.children {
            if !child.is_param
                && !child.is_wildcard
                && child.path == segment
                && let Some(handler) = self.match_recursive(
                    child,
                    method,
                    segments,
                    depth + 1,
                    params,
                    param_count,
                    middleware,
                )
            {
                return Some(handler);
            }
        }

        // Try param match
        for child in &node.children {
            if child.is_param {
                let old_count = *param_count;
                if (*param_count as usize) < MAX_PARAMS
                    && let Some(ref name) = child.param_name
                {
                    params[*param_count as usize] = (name.as_str(), segment);
                    *param_count += 1;
                }
                if let Some(handler) = self.match_recursive(
                    child,
                    method,
                    segments,
                    depth + 1,
                    params,
                    param_count,
                    middleware,
                ) {
                    return Some(handler);
                }
                // Backtrack
                *param_count = old_count;
            }
        }

        // Try wildcard match
        for child in &node.children {
            if child.is_wildcard {
                if (*param_count as usize) < MAX_PARAMS
                    && let Some(ref name) = child.param_name
                {
                    params[*param_count as usize] = (name.as_str(), segment);
                    *param_count += 1;
                }
                if let Some(h) = child.handlers.get(&method) {
                    middleware.extend_from_slice(&child.middleware);
                    return Some(h);
                }
            }
        }

        middleware.truncate(mw_start_len);
        None
    }

    // Middleware methods
    pub fn wrap(&mut self, mw: MiddlewareFn) {
        self.global_middleware.push(mw);
    }

    pub fn wrap_path(&mut self, path: &str, mw: MiddlewareFn) {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for segment in segments {
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

    // Modular Routing
    #[must_use]
    pub fn merge(mut self, other: Router) -> Self {
        Self::merge_nodes(&mut self.root, other.root);
        self.global_middleware.extend(other.global_middleware);
        self
    }

    #[must_use]
    pub fn nest(mut self, prefix: &str, other: Router) -> Self {
        let segments: Vec<&str> = prefix.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for segment in segments {
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
        // Merge handlers
        for (method, handler) in source.handlers {
            target.handlers.insert(method, handler);
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

    // Convenience methods
    pub fn get(&mut self, path: &str, handler: Handler) {
        self.add(Method::Get, path, handler);
    }
    pub fn post(&mut self, path: &str, handler: Handler) {
        self.add(Method::Post, path, handler);
    }
    pub fn put(&mut self, path: &str, handler: Handler) {
        self.add(Method::Put, path, handler);
    }
    pub fn delete(&mut self, path: &str, handler: Handler) {
        self.add(Method::Delete, path, handler);
    }
    pub fn patch(&mut self, path: &str, handler: Handler) {
        self.add(Method::Patch, path, handler);
    }
    pub fn head(&mut self, path: &str, handler: Handler) {
        self.add(Method::Head, path, handler);
    }
    pub fn options(&mut self, path: &str, handler: Handler) {
        self.add(Method::Options, path, handler);
    }
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
        Response::ok(ctx.req.path.to_string())
    }

    #[test]
    fn test_router_static() {
        let mut router = Router::new();
        router.get("/hello/world", test_handler);

        assert!(router.match_route(Method::Get, "/hello/world").is_some());
        assert!(router.match_route(Method::Get, "/hello").is_none());
        assert!(router.match_route(Method::Post, "/hello/world").is_none());
    }

    #[test]
    fn test_router_params() {
        let mut router = Router::new();
        router.get("/users/:id", test_handler);
        router.post("/users/:id/posts/:post_id", test_handler);

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
        r.headers.push(("X-Middleware", String::from("1")));
        r
    }

    #[test]
    fn test_nested_middleware() {
        let mut auth_router = Router::new();
        auth_router.wrap(dummy_middleware);
        auth_router.post("/login", test_handler);

        let mut root = Router::new();
        root = root.nest("/api", auth_router);
        root.get("/status", test_handler);

        let m1 = root.match_route(Method::Post, "/api/login");
        assert!(m1.is_some());
        assert_eq!(m1.unwrap().3.len(), 1); // Only auth gets middleware

        let m2 = root.match_route(Method::Get, "/status");
        assert!(m2.is_some());
        assert_eq!(m2.unwrap().3.len(), 0); // Root doesn't
    }

    #[test]
    fn test_router_merge() {
        let mut r1 = Router::new();
        r1.get("/r1", test_handler);

        let mut r2 = Router::new();
        r2.get("/r2", test_handler);

        let merged = r1.merge(r2);

        assert!(merged.match_route(Method::Get, "/r1").is_some());
        assert!(merged.match_route(Method::Get, "/r2").is_some());
    }
}
