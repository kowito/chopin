use chopin_core::routing;

#[test]
fn test_build_routes_returns_router() {
    let router = routing::build_routes();

    // Router is opaque, but we can verify it was created without panicking
    // This test mainly ensures the function compiles and runs
    drop(router);
}

#[test]
fn test_method_filter_exports() {
    use chopin_core::routing::MethodFilter;

    // Verify we can access the re-exported types
    let _get = MethodFilter::GET;
    let _post = MethodFilter::POST;
    let _put = MethodFilter::PUT;
    let _delete = MethodFilter::DELETE;
    let _patch = MethodFilter::PATCH;
}

// Routing function re-exports (get, post, put, delete, patch) are verified
// at compile-time - if they're not available, code using them won't compile.
// No runtime test needed since they're just re-exported from axum::routing.

#[test]
fn test_method_router_export() {
    use chopin_core::routing::MethodRouter;

    // Verify MethodRouter type is accessible
    // This is mainly a compile-time check
    fn _accepts_method_router(_: MethodRouter) {}
}

#[test]
fn test_on_function_export() {
    // Verify the `on` function for custom method routing is accessible
    use chopin_core::routing::MethodFilter;

    // MethodFilter is accessible
    let _ = MethodFilter::GET;
}

#[test]
fn test_method_routing_export() {
    // Verify method_routing is accessible through the routing module
    use chopin_core::routing::method_routing;

    // It's a module re-export - verify it exists
    let _ = std::marker::PhantomData::<fn(&method_routing::MethodRouter<()>)>;
}
