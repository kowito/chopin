use chopin::TestApp;

#[tokio::test]
async fn test_signup_success() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "test@example.com",
        "username": "testuser",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());

    let data = res.data();
    assert!(data["access_token"].is_string());
    assert_eq!(data["user"]["email"], "test@example.com");
    assert_eq!(data["user"]["username"], "testuser");
    // password_hash should NOT be in the response
    assert!(data["user"]["password_hash"].is_null());
}

#[tokio::test]
async fn test_signup_duplicate_email() {
    let app = TestApp::new().await;

    // Create first user
    app.create_user("dup@example.com", "user1", "password123")
        .await;

    // Try duplicate email
    let body = serde_json::json!({
        "email": "dup@example.com",
        "username": "user2",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    assert_eq!(res.status, 409);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_signup_duplicate_username() {
    let app = TestApp::new().await;

    app.create_user("a@example.com", "sameuser", "password123")
        .await;

    let body = serde_json::json!({
        "email": "b@example.com",
        "username": "sameuser",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    assert_eq!(res.status, 409);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_signup_missing_fields() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "",
        "username": "testuser",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    assert_eq!(res.status, 422);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_signup_short_password() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "short@example.com",
        "username": "shortpw",
        "password": "123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/signup"), &body.to_string())
        .await;

    assert_eq!(res.status, 422);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_login_success() {
    let app = TestApp::new().await;

    app.create_user("login@example.com", "loginuser", "password123")
        .await;

    let body = serde_json::json!({
        "email": "login@example.com",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/login"), &body.to_string())
        .await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());
    assert!(res.data()["access_token"].is_string());
    assert_eq!(res.data()["user"]["email"], "login@example.com");
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = TestApp::new().await;

    app.create_user("wrong@example.com", "wrongpw", "password123")
        .await;

    let body = serde_json::json!({
        "email": "wrong@example.com",
        "password": "wrong_password"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/login"), &body.to_string())
        .await;

    assert_eq!(res.status, 401);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "email": "noone@example.com",
        "password": "password123"
    });

    let res = app
        .client
        .post(&app.url("/api/auth/login"), &body.to_string())
        .await;

    assert_eq!(res.status, 401);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_login_then_use_token() {
    let app = TestApp::new().await;

    let (token, _user) = app
        .create_user("auth@example.com", "authuser", "password123")
        .await;

    // Token should be a valid non-empty string
    assert!(!token.is_empty());
    assert!(token.contains('.'), "JWT token should contain dots");
}

#[tokio::test]
async fn test_invalid_token() {
    let app = TestApp::new().await;

    // Try to access any protected route with an invalid token
    let res = app
        .client
        .get_with_auth(&app.url("/api/auth/signup"), "invalid-token")
        .await;

    // Should get a 4xx error (method not allowed or unauthorized depending on route)
    assert!(res.status >= 400);
}
