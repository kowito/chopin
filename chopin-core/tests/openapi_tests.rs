use chopin_core::TestApp;

#[tokio::test]
async fn test_openapi_json_endpoint() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;

    assert_eq!(res.status, 200);

    let json = res.json();

    // Verify OpenAPI structure
    assert_eq!(json["openapi"], "3.1.0");
    assert!(json["info"]["title"].is_string());
    assert_eq!(json["info"]["title"], "Chopin API");
    assert!(json["paths"].is_object());
}

#[tokio::test]
async fn test_openapi_has_auth_paths() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;
    let json = res.json();

    // Should have auth endpoints
    assert!(
        json["paths"]["/api/auth/signup"].is_object(),
        "Missing /api/auth/signup in OpenAPI spec"
    );
    assert!(
        json["paths"]["/api/auth/login"].is_object(),
        "Missing /api/auth/login in OpenAPI spec"
    );
}

#[tokio::test]
async fn test_openapi_has_security_scheme() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;
    let json = res.json();

    // Should have bearer auth security scheme
    assert!(
        json["components"]["securitySchemes"]["bearer_auth"].is_object(),
        "Missing bearer_auth security scheme in OpenAPI spec"
    );
}

#[tokio::test]
async fn test_openapi_has_schemas() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;
    let json = res.json();

    let schemas = &json["components"]["schemas"];

    assert!(schemas["SignupRequest"].is_object(), "Missing SignupRequest schema");
    assert!(schemas["LoginRequest"].is_object(), "Missing LoginRequest schema");
    assert!(schemas["AuthResponse"].is_object(), "Missing AuthResponse schema");
    assert!(schemas["UserResponse"].is_object(), "Missing UserResponse schema");
}

#[tokio::test]
async fn test_swagger_ui_accessible() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api-docs")).await;

    // Scalar UI serves HTML
    assert_eq!(res.status, 200);
    assert!(res.body.contains("html") || res.body.contains("script"),
        "Swagger/Scalar UI should return HTML");
}
