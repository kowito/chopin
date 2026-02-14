use chopin::server::FastRoute;
use hyper::Method;

// ═══ FastRoute construction ═══

#[test]
fn test_fast_route_json() {
    let route = FastRoute::json("/json", br#"{"message":"Hello"}"#);
    let display = format!("{}", route);
    assert!(
        display.contains("/json"),
        "Should contain path: {}",
        display
    );
    // {"message":"Hello"} = 19 bytes
    assert!(
        display.contains("19 bytes"),
        "Should contain body length: {}",
        display
    );
}

#[test]
fn test_fast_route_text() {
    let route = FastRoute::text("/plaintext", b"Hello, World!");
    let display = format!("{}", route);
    assert!(display.contains("/plaintext"));
    assert!(display.contains("13 bytes"), "display: {}", display);
}

#[test]
fn test_fast_route_html() {
    let route = FastRoute::html("/page", b"<h1>Hello</h1>");
    let display = format!("{}", route);
    assert!(display.contains("/page"));
    assert!(display.contains("14 bytes"), "display: {}", display);
}

#[test]
fn test_fast_route_new_custom_content_type() {
    let route = FastRoute::new("/data", b"binary data", "application/octet-stream");
    let display = format!("{}", route);
    assert!(display.contains("/data"));
    assert!(display.contains("11 bytes"), "display: {}", display);
}

// ═══ Decorators ═══

#[test]
fn test_fast_route_cors() {
    let route = FastRoute::json("/api/status", br#"{"ok":true}"#).cors();
    let display = format!("{}", route);
    assert!(display.contains("/api/status"));
    assert!(
        display.contains("+cors"),
        "Display should include +cors: {}",
        display
    );
}

#[test]
fn test_fast_route_cache_control() {
    let route = FastRoute::text("/health", b"OK").cache_control("public, max-age=60");
    let display = format!("{}", route);
    assert!(display.contains("/health"));
    assert!(display.contains("2 bytes"));
}

#[test]
fn test_fast_route_get_only() {
    let route = FastRoute::json("/json", br#"{"msg":"hi"}"#).get_only();
    let display = format!("{}", route);
    assert!(display.contains("GET"));
    assert!(display.contains("HEAD"));
}

#[test]
fn test_fast_route_custom_methods() {
    let route = FastRoute::json("/data", br#"{}"#).methods(&[Method::GET, Method::POST]);
    let display = format!("{}", route);
    assert!(display.contains("GET"));
    assert!(display.contains("POST"));
}

#[test]
fn test_fast_route_single_method_post() {
    let route = FastRoute::json("/post-only", br#"{}"#).methods(&[Method::POST]);
    let display = format!("{}", route);
    assert!(display.contains("POST"));
    assert!(!display.contains("GET"));
}

#[test]
fn test_fast_route_custom_header() {
    let route = FastRoute::json("/api", br#"{"v":1}"#)
        .header(hyper::header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    let display = format!("{}", route);
    assert!(display.contains("/api"));
}

#[test]
fn test_fast_route_chained_decorators() {
    let route = FastRoute::json("/api/v1/status", br#"{"status":"ok"}"#)
        .cors()
        .cache_control("public, max-age=300")
        .get_only()
        .header(hyper::header::X_FRAME_OPTIONS, "DENY");

    let display = format!("{}", route);
    assert!(display.contains("/api/v1/status"));
    assert!(display.contains("+cors"));
    assert!(display.contains("GET"));
    assert!(display.contains("HEAD"));
}

#[test]
fn test_fast_route_clone() {
    let route = FastRoute::json("/json", br#"{"message":"Hello"}"#).cors();
    let cloned = route.clone();
    assert_eq!(format!("{}", route), format!("{}", cloned));
}

#[test]
fn test_fast_route_empty_body() {
    let route = FastRoute::json("/empty", b"");
    let display = format!("{}", route);
    assert!(display.contains("0 bytes"));
}

#[test]
fn test_fast_route_debug() {
    let route = FastRoute::json("/debug", br#"{"a":1}"#).cors().get_only();
    let debug = format!("{:?}", route);
    assert!(debug.contains("FastRoute"));
    assert!(debug.contains("/debug"));
    assert!(debug.contains("cors: true"));
}

// ═══ Multiple fast routes ═══

#[test]
fn test_multiple_fast_routes() {
    let routes = [
        FastRoute::json("/json", br#"{"message":"Hello, World!"}"#),
        FastRoute::text("/plaintext", b"Hello, World!"),
        FastRoute::json("/api/status", br#"{"status":"ok"}"#)
            .cors()
            .get_only(),
        FastRoute::text("/health", b"OK").cache_control("public, max-age=60"),
    ];

    assert_eq!(routes.len(), 4);

    let displays: Vec<String> = routes.iter().map(|r| format!("{}", r)).collect();
    assert!(displays[0].contains("/json"));
    assert!(displays[1].contains("/plaintext"));
    assert!(displays[2].contains("/api/status"));
    assert!(displays[3].contains("/health"));
}

// ═══ Display format correctness ═══

#[test]
fn test_display_bare_route_no_methods_no_cors() {
    let route = FastRoute::text("/bare", b"hi");
    let display = format!("{}", route);
    assert!(!display.contains("+cors"));
    assert!(!display.contains("["));
    assert!(display.contains("/bare"));
    assert!(display.contains("2 bytes"));
}

#[test]
fn test_display_cors_only_no_methods() {
    let route = FastRoute::json("/cors-only", br#"{}"#).cors();
    let display = format!("{}", route);
    assert!(display.contains("+cors"));
    // No method filter → no brackets
    assert!(
        !display.contains("["),
        "No method filter should mean no []: {}",
        display
    );
}

#[test]
fn test_display_methods_only_no_cors() {
    let route = FastRoute::json("/methods-only", br#"{}"#).get_only();
    let display = format!("{}", route);
    assert!(display.contains("["));
    assert!(display.contains("GET"));
    assert!(!display.contains("+cors"));
}

#[test]
fn test_fast_route_cors_with_methods() {
    let route = FastRoute::json("/cors-get", br#"{"cors":true}"#)
        .cors()
        .get_only();
    let display = format!("{}", route);
    assert!(display.contains("+cors"));
    assert!(display.contains("GET"));
    assert!(display.contains("HEAD"));
}
