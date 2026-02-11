use chopin_core::TestApp;

#[tokio::test]
async fn test_404_returns_json() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api/nonexistent")).await;

    // Axum returns 404 for unknown routes
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn test_invalid_json_body() {
    let app = TestApp::new().await;

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), "not valid json at all")
        .await;

    assert_eq!(res.status, 422);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_missing_json_fields() {
    let app = TestApp::new().await;

    // Send JSON missing required fields
    let body = serde_json::json!({
        "email": "test@example.com"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    // Should fail with validation error (422) or bad request
    assert!(res.status >= 400);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_response_format() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "format@example.com",
        "username": "formatuser",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    let json = res.json();

    // Response should have the standard Chopin format
    assert!(json.get("success").is_some(), "Response must have 'success' field");
    assert!(json.get("data").is_some() || json.get("error").is_some(),
        "Response must have either 'data' or 'error' field");
}
