// src/router.rs
use std::collections::HashMap;
use crate::http::{Method, Context, Response};

pub type Handler = fn(Context) -> Response;

#[derive(Clone)]
pub struct RouteNode {
    pub path: String,
    pub handlers: HashMap<Method, Handler>,
    pub children: Vec<RouteNode>,
    pub is_param: bool,
    pub param_name: Option<String>,
    pub is_wildcard: bool,
}

impl RouteNode {
    pub fn new(path: String) -> Self {
        Self {
            path,
            handlers: HashMap::new(),
            children: Vec::new(),
            is_param: false,
            param_name: None,
            is_wildcard: false,
        }
    }
}

#[derive(Clone)]
pub struct Router {
    pub root: RouteNode,
}

impl Router {
    pub fn new() -> Self {
        Self {
            root: RouteNode::new(String::new()),
        }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for segment in segments {
            // Check if segment is a param or wildcard
            let is_param = segment.starts_with(':');
            let is_wildcard = segment.starts_with('*');
            
            let param_name = if is_param {
                Some(segment[1..].to_string())
            } else if is_wildcard {
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
                if child.is_param == is_param && child.is_wildcard == is_wildcard {
                    if is_param || is_wildcard || child.path == segment_path {
                        found_idx = Some(i);
                        break;
                    }
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
    
    pub fn match_route<'a>(&self, method: Method, path: &'a str) -> Option<(&Handler, HashMap<String, String>)> {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut params = HashMap::new();
        
        let handler = self.match_recursive(&self.root, method, &segments, 0, &mut params);
        handler.map(|h| (h, params))
    }

    fn match_recursive<'a>(&'a self, node: &'a RouteNode, method: Method, segments: &[&str], depth: usize, params: &mut HashMap<String, String>) -> Option<&'a Handler> {
        if depth == segments.len() {
            return node.handlers.get(&method);
        }

        let segment = segments[depth];

        // Try exact match first
        for child in &node.children {
            if !child.is_param && !child.is_wildcard && child.path == segment {
                if let Some(handler) = self.match_recursive(child, method, segments, depth + 1, params) {
                    return Some(handler);
                }
            }
        }

        // Try param match
        for child in &node.children {
            if child.is_param {
                if let Some(ref name) = child.param_name {
                    params.insert(name.clone(), segment.to_string());
                }
                if let Some(handler) = self.match_recursive(child, method, segments, depth + 1, params) {
                    return Some(handler);
                }
                // Backtrack if not matched
                if let Some(ref name) = child.param_name {
                    params.remove(name);
                }
            }
        }

        // Try wildcard match
        for child in &node.children {
            if child.is_wildcard {
                if let Some(ref name) = child.param_name {
                    params.insert(name.clone(), segments[depth..].join("/"));
                }
                return child.handlers.get(&method);
            }
        }

        None
    }
    
    // Convenience methods
    pub fn get(&mut self, path: &str, handler: Handler) { self.add(Method::Get, path, handler); }
    pub fn post(&mut self, path: &str, handler: Handler) { self.add(Method::Post, path, handler); }
    pub fn put(&mut self, path: &str, handler: Handler) { self.add(Method::Put, path, handler); }
    pub fn delete(&mut self, path: &str, handler: Handler) { self.add(Method::Delete, path, handler); }
    pub fn patch(&mut self, path: &str, handler: Handler) { self.add(Method::Patch, path, handler); }
    pub fn head(&mut self, path: &str, handler: Handler) { self.add(Method::Head, path, handler); }
    pub fn options(&mut self, path: &str, handler: Handler) { self.add(Method::Options, path, handler); }
    pub fn trace(&mut self, path: &str, handler: Handler) { self.add(Method::Trace, path, handler); }
    pub fn connect(&mut self, path: &str, handler: Handler) { self.add(Method::Connect, path, handler); }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Request;

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
        let (_, params1) = match1.unwrap();
        assert_eq!(params1.get("id").unwrap(), "123");

        let match2 = router.match_route(Method::Post, "/users/123/posts/abc");
        assert!(match2.is_some());
        let (_, params2) = match2.unwrap();
        assert_eq!(params2.get("id").unwrap(), "123");
        assert_eq!(params2.get("post_id").unwrap(), "abc");
    }

    #[test]
    fn test_router_wildcard() {
        let mut router = Router::new();
        router.get("/assets/*path", test_handler);

        let match1 = router.match_route(Method::Get, "/assets/js/app.js");
        assert!(match1.is_some());
        let (_, params1) = match1.unwrap();
        assert_eq!(params1.get("path").unwrap(), "js/app.js");
    }
}
