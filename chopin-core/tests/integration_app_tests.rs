use chopin::TestApp;

// ═══ TestApp creation ═══

#[tokio::test]
async fn test_app_creates_successfully() {
    let app = TestApp::new().await;
    // Should have a valid URL
    assert!(app.url("/").starts_with("http://"));
}

#[tokio::test]
async fn test_app_url_builds_correctly() {
    let app = TestApp::new().await;
    let url = app.url("/api/users");
    assert!(url.ends_with("/api/users"));
}

// ═══ GET / welcome endpoint ═══

#[tokio::test]
async fn test_welcome_endpoint() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/")).await;
    assert_eq!(res.status, 200, "Welcome endpoint should return 200");
    let json = res.json();
    assert!(json.get("message").is_some());
    assert!(json.get("status").is_some());
}

// ═══ OpenAPI ═══

#[tokio::test]
async fn test_openapi_json_endpoint() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;
    assert_eq!(res.status, 200, "OpenAPI endpoint should return 200");
    let json = res.json();
    assert!(
        json.get("openapi").is_some(),
        "Should have openapi version field"
    );
}

// ═══ Auth signup ═══

#[tokio::test]
async fn test_signup_success() {
    let app = TestApp::new().await;
    let body = serde_json::json!({
        "email": "integration@test.com",
        "username": "integrationuser",
        "password": "password123"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;
    assert!(
        res.is_success(),
        "Signup should succeed: status={}",
        res.status
    );
    let data = res.data();
    assert!(
        data.get("access_token").is_some(),
        "Should return an access_token"
    );
    assert!(data.get("user").is_some(), "Should return user info");
}

#[tokio::test]
async fn test_signup_duplicate_email() {
    let app = TestApp::new().await;
    let body = serde_json::json!({
        "email": "dup@test.com",
        "username": "user1",
        "password": "password123"
    });

    // First signup
    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;
    assert!(res.is_success());

    // Second signup with same email should conflict
    let body2 = serde_json::json!({
        "email": "dup@test.com",
        "username": "user2",
        "password": "password123"
    });
    let res2 = app
        .client
        .post(&app.url("/api/auth/signup"), &body2.to_string())
        .await;
    assert!(!res2.is_success(), "Duplicate email should fail");
    assert!(res2.status >= 400);
}

// ═══ Auth login ═══

#[tokio::test]
async fn test_login_success() {
    let app = TestApp::new().await;

    // First signup
    let body = serde_json::json!({
        "email": "login@test.com",
        "username": "loginuser",
        "password": "password123"
    });
    app.client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    // Then login
    let login_body = serde_json::json!({
        "email": "login@test.com",
        "password": "password123"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/login"), &login_body.to_string())
        .await;
    assert!(
        res.is_success(),
        "Login should succeed: status={}",
        res.status
    );
    let data = res.data();
    assert!(data.get("access_token").is_some());
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "wrong@test.com",
        "username": "wronguser",
        "password": "password123"
    });
    app.client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    let login = serde_json::json!({
        "email": "wrong@test.com",
        "password": "wrongpassword"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/login"), &login.to_string())
        .await;
    assert!(!res.is_success());
    assert_eq!(res.status, 401);
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let app = TestApp::new().await;

    let login = serde_json::json!({
        "email": "nobody@test.com",
        "password": "password123"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/login"), &login.to_string())
        .await;
    assert!(!res.is_success());
}

// ═══ Auth protected - no /me route exists ═══
// The framework doesn't include a /me route by default.
// Users are expected to add their own protected routes.

#[tokio::test]
async fn test_nonexistent_protected_endpoint() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/api/auth/me")).await;
    // Should return 404 since /me doesn't exist
    assert_eq!(res.status, 404);
}

// ═══ TestResponse helpers ═══

#[tokio::test]
async fn test_response_json_method() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/")).await;
    let json = res.json();
    assert!(json.is_object(), "json() should return a JSON object");
}

#[tokio::test]
async fn test_response_is_success_for_api_response() {
    // is_success() checks for {"success": true} in body — only works with ApiResponse format
    let app = TestApp::new().await;
    let body = serde_json::json!({
        "email": "issuccess@test.com",
        "username": "issuccessuser",
        "password": "password123"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;
    assert!(res.is_success());
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_response_not_success_for_bad_login() {
    let app = TestApp::new().await;
    let login = serde_json::json!({
        "email": "nobody@test.com",
        "password": "wrongpassword"
    });
    let res = app
        .client
        .post(&app.url("/api/auth/login"), &login.to_string())
        .await;
    assert!(!res.is_success());
    assert!(res.status >= 400);
}

#[tokio::test]
async fn test_404_returns_empty_body() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/nonexistent/path")).await;
    assert_eq!(res.status, 404);
}
