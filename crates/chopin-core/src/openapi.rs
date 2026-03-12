use crate::http::{Context, Method, Response};
use crate::router::RouteDef;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Generates the OpenAPI 3.0.0 JSON specification for all registered routes.
pub fn generate_spec() -> Value {
    let mut paths: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();

    for route in inventory::iter::<RouteDef> {
        let method = match route.method {
            Method::Get => "get",
            Method::Post => "post",
            Method::Put => "put",
            Method::Delete => "delete",
            Method::Patch => "patch",
            Method::Head => "head",
            Method::Options => "options",
            Method::Trace => "trace",
            Method::Connect => "connect",
            Method::Unknown => "unknown",
        };

        // Convert Chopin path format (/users/:id) to OpenAPI format (/users/{id})
        let mut openapi_path = String::new();
        let mut parameters = Vec::new();

        for segment in route.path.split('/') {
            if segment.is_empty() {
                continue;
            }
            if let Some(param) = segment.strip_prefix(':') {
                openapi_path.push_str(&format!("/{{{}}}", param));
                parameters.push(json!({
                    "name": param,
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" }
                }));
            } else if let Some(wildcard) = segment.strip_prefix('*') {
                openapi_path.push_str(&format!("/{{{}}}", wildcard));
                parameters.push(json!({
                    "name": wildcard,
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" }
                }));
            } else {
                openapi_path.push_str(&format!("/{}", segment));
            }
        }

        if openapi_path.is_empty() {
            openapi_path = "/".to_string();
        }

        let mut operation = json!({
            "summary": route.summary,
            "description": route.description,
            "responses": {
                "200": {
                    "description": "OK"
                }
            }
        });

        if !parameters.is_empty() {
            operation.as_object_mut().unwrap().insert("parameters".to_string(), json!(parameters));
        }

        paths.entry(openapi_path).or_default().insert(method.to_string(), operation);
    }

    json!({
        "openapi": "3.0.0",
        "info": {
            "title": "Chopin API",
            "version": "1.0.0",
            "description": "High-fidelity API documentation for the Chopin framework."
        },
        "paths": paths
    })
}

/// Handler for openapi.json
pub fn openapi_json_handler(_ctx: Context) -> Response {
    let spec = generate_spec();
    let bytes = serde_json::to_vec(&spec).unwrap_or_default();
    Response::json_bytes(bytes)
}

/// Handler for Scalar API Reference (at /docs)
pub fn scalar_docs_handler(_ctx: Context) -> Response {
    let html = r#"<!doctype html>
<html>
  <head>
    <title>Chopin API Reference</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      body { margin: 0; }
    </style>
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#;

    Response::text(html.to_string()).with_header("Content-Type", "text/html; charset=utf-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_spec() {
        let spec = generate_spec();
        assert_eq!(spec["openapi"], "3.0.0");
        assert_eq!(spec["info"]["title"], "Chopin API");
        assert!(spec["paths"].is_object());
    }
}
