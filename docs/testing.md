# Testing (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Overview

Chopin provides `TestApp` and `TestClient` utilities for integration testing. Tests run against an in-memory SQLite database with auto-migrations.

## Setup

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["test-util", "macros"] }
serde_json = "1"
```

## Basic Test

```rust
use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_welcome() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/")).await;
    assert_eq!(res.status, 200);
    assert!(res.body.contains("Welcome to Chopin"));
}
```

## TestApp

`TestApp::new()` spins up a complete Chopin server:

- In-memory SQLite database
- Auto-runs all migrations
- Binds to a random port
- Starts the server in a background task

```rust
pub struct TestApp {
    pub addr: SocketAddr,      // Server address
    pub client: TestClient,    // HTTP client
    pub db: DatabaseConnection, // Direct DB access
    pub config: Config,        // App configuration
}
```

### Custom Config

```rust
let app = TestApp::with_config(Config {
    server_mode: ServerMode::Standard,
    database_url: "sqlite::memory:".to_string(),
    jwt_secret: "test-secret".to_string(),
    jwt_expiry_hours: 1,
    // ...
}).await;
```

## TestClient

The `TestClient` provides HTTP methods:

```rust
// GET
let res = app.client.get(&app.url("/api/posts")).await;

// POST
let res = app.client.post(&app.url("/api/posts"), &body).await;

// PUT
let res = app.client.put(&app.url("/api/posts/1"), &body).await;

// DELETE
let res = app.client.delete(&app.url("/api/posts/1")).await;

// With auth header
let res = app.client.get_with_token(&app.url("/api/me"), &token).await;
let res = app.client.post_with_token(&app.url("/api/posts"), &body, &token).await;
let res = app.client.put_with_token(&app.url("/api/posts/1"), &body, &token).await;
let res = app.client.delete_with_token(&app.url("/api/posts/1"), &token).await;
```

## TestResponse

```rust
pub struct TestResponse {
    pub status: u16,          // HTTP status code
    pub body: String,         // Response body as string
    pub headers: HeaderMap,   // Response headers
}
```

Parse JSON from the response:

```rust
let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
assert_eq!(json["success"], true);
assert_eq!(json["data"]["email"], "alice@example.com");
```

## Helper Methods

### Create a User

```rust
let (token, user) = app.create_user("alice@test.com", "alice", "password123").await;
// token = JWT string
// user = serde_json::Value with id, email, username, role
```

### Login

```rust
let token = app.login("alice@test.com", "password123").await;
```

## Example Test Suite

```rust
use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_auth_signup() {
    let app = TestApp::new().await;
    let body = r#"{"email":"bob@test.com","username":"bob","password":"secret123"}"#;
    let res = app.client.post(&app.url("/api/auth/signup"), body).await;
    assert_eq!(res.status, 201);

    let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]["access_token"].is_string());
}

#[tokio::test]
async fn test_auth_login() {
    let app = TestApp::new().await;
    app.create_user("bob@test.com", "bob", "secret123").await;

    let body = r#"{"email":"bob@test.com","password":"secret123"}"#;
    let res = app.client.post(&app.url("/api/auth/login"), body).await;
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_protected_endpoint() {
    let app = TestApp::new().await;
    let (token, _) = app.create_user("bob@test.com", "bob", "secret123").await;

    // Without token → 401
    let res = app.client.get(&app.url("/api/protected")).await;
    assert_eq!(res.status, 401);

    // With token → 200
    let res = app.client.get_with_token(&app.url("/api/protected"), &token).await;
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_404_returns_json() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/nonexistent")).await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn test_openapi_spec() {
    let app = TestApp::new().await;
    let res = app.client.get(&app.url("/api-docs/openapi.json")).await;
    assert_eq!(res.status, 200);

    let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
    assert_eq!(json["openapi"], "3.1.0");
}
```

## Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run a specific test
cargo test test_auth_signup
```
