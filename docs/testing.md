# Testing Guide

Comprehensive guide to testing Chopin applications.

## Table of Contents

- [Quick Start](#quick-start)
- [Test Utilities](#test-utilities)
- [Unit Tests](#unit-tests)
- [Integration Tests](#integration-tests)
- [API Tests](#api-tests)
- [Database Tests](#database-tests)
- [Authentication Tests](#authentication-tests)
- [Best Practices](#best-practices)

## Quick Start

Chopin provides `TestApp` for easy integration testing:

```rust
use chopin_core::TestApp;

#[tokio::test]
async fn test_create_post() {
    let app = TestApp::new().await;
    
    // Create and authenticate user
    let (token, _) = app.create_user("test@example.com", "testuser", "password123").await;
    
    // Make authenticated request
    let response = app.client
        .post(&app.url("/api/posts"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "title": "Test Post",
            "body": "Test body"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 200);
    assert!(response.is_success());
}
```

## Test Utilities

### TestApp

Provides isolated test environment:

```rust
pub struct TestApp {
    pub addr: SocketAddr,    // Server address
    pub db: DatabaseConnection,  // Database connection
    pub client: TestClient,      // HTTP client
}
```

**Features**:
- Fresh in-memory SQLite database per test
- Random port assignment
- Automatic server lifecycle management
- Helper methods for common operations

**Usage**:

```rust
#[tokio::test]
async fn my_test() {
    let app = TestApp::new().await;
    
    // Access database
    let count = User::find().count(&app.db).await.unwrap();
    
    // Make HTTP requests
    let response = app.client
        .get(&app.url("/api/users"))
        .send()
        .await;
    
    // Assertions
    assert_eq!(response.status, 200);
}
```

### TestClient

HTTP client with convenient methods:

```rust
pub struct TestClient {
    client: reqwest::Client,
}

impl TestClient {
    // GET request
    pub async fn get(&self, url: &str) -> TestResponse;
    
    // POST with JSON
    pub async fn post(&self, url: &str) -> RequestBuilder;
    
    // Authenticated request
    pub async fn post_with_auth(&self, url: &str, token: &str, json: &str) -> TestResponse;
}
```

### Helper Methods

**Create User**:
```rust
let (token, user) = app.create_user(
    "user@example.com",
    "username",
    "password123"
).await;
```

**Login**:
```rust
let token = app.login("user@example.com", "password123").await;
```

**Get URL**:
```rust
let url = app.url("/api/posts");
// http://127.0.0.1:RANDOM_PORT/api/posts
```

## Unit Tests

Test individual functions and modules:

### Testing Models

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_post_validation() {
        let post = CreatePostRequest {
            title: "".to_string(),  // Invalid: empty
            body: "Body".to_string(),
        };
        
        let result = post.validate();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_post_response_conversion() {
        let model = post::Model {
            id: 1,
            title: "Test".to_string(),
            body: "Body".to_string(),
            created_at: Utc::now().naive_utc(),
        };
        
        let response = PostResponse::from(model);
        assert_eq!(response.id, 1);
        assert_eq!(response.title, "Test");
    }
}
```

### Testing Utilities

```rust
#[cfg(test)]
mod tests {
    use crate::auth::password;
    
    #[test]
    fn test_password_hashing() {
        let password = "secret123";
        let hash = password::hash(password).unwrap();
        
        assert!(password::verify(password, &hash).unwrap());
        assert!(!password::verify("wrong", &hash).unwrap());
    }
    
    #[test]
    fn test_jwt_generation() {
        let token = jwt::generate(123, 24).unwrap();
        assert!(!token.is_empty());
        
        let decoded = jwt::decode(&token).unwrap();
        assert_eq!(decoded.sub, 123);
    }
}
```

## Integration Tests

Test complete workflows:

### File Structure

```
your-app/
├── src/
└── tests/
    ├── auth_tests.rs
    ├── post_tests.rs
    └── integration_tests.rs
```

### Basic Integration Test

```rust
// tests/post_tests.rs
use chopin_core::TestApp;

#[tokio::test]
async fn test_post_crud_flow() {
    let app = TestApp::new().await;
    let (token, user) = app.create_user("test@example.com", "test", "password").await;
    
    // Create post
    let create_response = app.client
        .post(&app.url("/api/posts"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "title": "Test Post",
            "body": "Test body"
        }))
        .send()
        .await;
    
    assert_eq!(create_response.status, 200);
    let created: ApiResponse<PostResponse> = create_response.json().await;
    let post_id = created.data.unwrap().id;
    
    // Get post
    let get_response = app.client
        .get(&app.url(&format!("/api/posts/{}", post_id)))
        .send()
        .await;
    
    assert_eq!(get_response.status, 200);
    
    // Update post
    let update_response = app.client
        .put(&app.url(&format!("/api/posts/{}", post_id)))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "title": "Updated Title"
        }))
        .send()
        .await;
    
    assert_eq!(update_response.status, 200);
    
    // Delete post
    let delete_response = app.client
        .delete(&app.url(&format!("/api/posts/{}", post_id)))
        .bearer_auth(&token)
        .send()
        .await;
    
    assert_eq!(delete_response.status, 200);
}
```

## API Tests

### Testing Endpoints

**Success Cases**:

```rust
#[tokio::test]
async fn test_list_posts() {
    let app = TestApp::new().await;
    
    let response = app.client
        .get(&app.url("/api/posts"))
        .send()
        .await;
    
    assert_eq!(response.status, 200);
    
    let json: ApiResponse<Vec<PostResponse>> = response.json().await;
    assert!(json.success);
    assert!(json.data.is_some());
}
```

**Error Cases**:

```rust
#[tokio::test]
async fn test_get_nonexistent_post() {
    let app = TestApp::new().await;
    
    let response = app.client
        .get(&app.url("/api/posts/99999"))
        .send()
        .await;
    
    assert_eq!(response.status, 404);
    
    let json: ApiResponse<()> = response.json().await;
    assert!(!json.success);
    assert!(json.error.is_some());
}
```

### Testing Validation

```rust
#[tokio::test]
async fn test_create_post_validation() {
    let app = TestApp::new().await;
    let (token, _) = app.create_user("test@example.com", "test", "password").await;
    
    // Missing required field
    let response = app.client
        .post(&app.url("/api/posts"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "body": "Only body, no title"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 400);
}
```

### Testing Pagination

```rust
#[tokio::test]
async fn test_pagination() {
    let app = TestApp::new().await;
    let (token, _) = app.create_user("test@example.com", "test", "password").await;
    
    // Create 50 posts
    for i in 1..=50 {
        app.client
            .post(&app.url("/api/posts"))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "title": format!("Post {}", i),
                "body": "Body"
            }))
            .send()
            .await;
    }
    
    // Test pagination
    let response = app.client
        .get(&app.url("/api/posts?limit=10&offset=0"))
        .send()
        .await;
    
    let json: ApiResponse<Vec<PostResponse>> = response.json().await;
    assert_eq!(json.data.unwrap().len(), 10);
    
    // Second page
    let response = app.client
        .get(&app.url("/api/posts?limit=10&offset=10"))
        .send()
        .await;
    
    let json: ApiResponse<Vec<PostResponse>> = response.json().await;
    assert_eq!(json.data.unwrap().len(), 10);
}
```

## Database Tests

### Direct Database Access

```rust
#[tokio::test]
async fn test_database_operations() {
    let app = TestApp::new().await;
    
    // Insert directly
    let user = user::ActiveModel {
        email: Set("test@example.com".to_string()),
        username: Set("test".to_string()),
        password_hash: Set("hash".to_string()),
        created_at: Set(Utc::now().naive_utc()),
        ..Default::default()
    };
    
    let saved_user = user.insert(&app.db).await.unwrap();
    assert_eq!(saved_user.email, "test@example.com");
    
    // Query
    let found_user = User::find()
        .filter(user::Column::Email.eq("test@example.com"))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(found_user.id, saved_user.id);
}
```

### Testing Transactions

```rust
#[tokio::test]
async fn test_transaction_rollback() {
    let app = TestApp::new().await;
    
    let txn = app.db.begin().await.unwrap();
    
    // Insert in transaction
    let user = user::ActiveModel {
        email: Set("test@example.com".to_string()),
        // ... other fields
    };
    user.insert(&txn).await.unwrap();
    
    // Rollback
    txn.rollback().await.unwrap();
    
    // Verify not persisted
    let count = User::find().count(&app.db).await.unwrap();
    assert_eq!(count, 0);
}
```

### Testing Migrations

```rust
#[tokio::test]
async fn test_migrations_applied() {
    let app = TestApp::new().await;
    
    // Verify tables exist
    let result = sqlx::query("SELECT 1 FROM users LIMIT 1")
        .fetch_optional(&app.db)
        .await;
    
    assert!(result.is_ok());
}
```

## Authentication Tests

### Signup Flow

```rust
#[tokio::test]
async fn test_signup() {
    let app = TestApp::new().await;
    
    let response = app.client
        .post(&app.url("/api/auth/signup"))
        .json(&serde_json::json!({
            "email": "new@example.com",
            "username": "newuser",
            "password": "password123"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 200);
    
    let json: ApiResponse<AuthResponse> = response.json().await;
    assert!(json.success);
    assert!(json.data.unwrap().access_token.len() > 0);
}

#[tokio::test]
async fn test_signup_duplicate_email() {
    let app = TestApp::new().await;
    
    // First signup
    app.create_user("user@example.com", "user1", "password").await;
    
    // Duplicate signup
    let response = app.client
        .post(&app.url("/api/auth/signup"))
        .json(&serde_json::json!({
            "email": "user@example.com",
            "username": "user2",
            "password": "password123"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 409); // Conflict
}
```

### Login Flow

```rust
#[tokio::test]
async fn test_login_success() {
    let app = TestApp::new().await;
    let (_, user) = app.create_user("user@example.com", "user", "password123").await;
    
    let response = app.client
        .post(&app.url("/api/auth/login"))
        .json(&serde_json::json!({
            "email": "user@example.com",
            "password": "password123"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 200);
    
    let json: ApiResponse<AuthResponse> = response.json().await;
    assert!(json.success);
    assert_eq!(json.data.unwrap().user.id, user.id);
}

#[tokio::test]
async fn test_login_invalid_password() {
    let app = TestApp::new().await;
    app.create_user("user@example.com", "user", "password123").await;
    
    let response = app.client
        .post(&app.url("/api/auth/login"))
        .json(&serde_json::json!({
            "email": "user@example.com",
            "password": "wrongpassword"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 401); // Unauthorized
}
```

### Protected Endpoints

```rust
#[tokio::test]
async fn test_protected_endpoint_without_auth() {
    let app = TestApp::new().await;
    
    let response = app.client
        .post(&app.url("/api/posts"))
        .json(&serde_json::json!({
            "title": "Test",
            "body": "Body"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 401); // Unauthorized
}

#[tokio::test]
async fn test_protected_endpoint_with_auth() {
    let app = TestApp::new().await;
    let (token, _) = app.create_user("user@example.com", "user", "password").await;
    
    let response = app.client
        .post(&app.url("/api/posts"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "title": "Test",
            "body": "Body"
        }))
        .send()
        .await;
    
    assert_eq!(response.status, 200); // Success
}

#[tokio::test]
async fn test_expired_token() {
    // Set short expiry
    std::env::set_var("JWT_EXPIRY_HOURS", "0");
    
    let app = TestApp::new().await;
    let (token, _) = app.create_user("user@example.com", "user", "password").await;
    
    // Wait for expiry
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    let response = app.client
        .get(&app.url("/api/posts"))
        .bearer_auth(&token)
        .send()
        .await;
    
    assert_eq!(response.status, 401);
}
```

## Best Practices

### ✅ DO

**1. Use TestApp for Integration Tests**
```rust
#[tokio::test]
async fn test_feature() {
    let app = TestApp::new().await;
    // Each test gets isolated environment
}
```

**2. Test Both Success and Error Cases**
```rust
#[tokio::test]
async fn test_success_case() { /* ... */ }

#[tokio::test]
async fn test_invalid_input() { /* ... */ }

#[tokio::test]
async fn test_not_found() { /* ... */ }

#[tokio::test]
async fn test_unauthorized() { /* ... */ }
```

**3. Use Descriptive Test Names**
```rust
#[tokio::test]
async fn test_user_can_create_post_when_authenticated() { /* ... */ }

#[tokio::test]
async fn test_user_cannot_delete_other_users_posts() { /* ... */ }
```

**4. Clean Up Test Data**
```rust
// TestApp automatically provides fresh database
// No manual cleanup needed
```

**5. Test Edge Cases**
```rust
#[tokio::test]
async fn test_pagination_empty_results() { /* ... */ }

#[tokio::test]
async fn test_pagination_exceeds_max_limit() { /* ... */ }
```

### ❌ DON'T

**1. Share State Between Tests**
```rust
// Bad - tests affect each other
static mut SHARED_STATE: i32 = 0;

// Good - isolated per test
let app = TestApp::new().await;
```

**2. Use Real External Services**
```rust
// Bad
let stripe_client = StripeClient::new();

// Good - mock external services
let mock_stripe = MockStripe::new();
```

**3. Ignore Test Failures**
```rust
// Bad
#[tokio::test]
#[ignore]
async fn test_known_to_fail() { /* ... */ }

// Good - fix or remove
```

## Running Tests

### Run All Tests

```bash
cargo test
```

### Run Specific Test

```bash
cargo test test_signup
```

### Run Integration Tests Only

```bash
cargo test --test integration_tests
```

### Run with Output

```bash
cargo test -- --nocapture
```

### Run in Parallel

```bash
cargo test -- --test-threads=4
```

### Run with Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out html
```

## Advanced Testing

### Custom TestApp Setup

```rust
pub async fn setup_with_posts(count: usize) -> TestApp {
    let app = TestApp::new().await;
    let (token, _) = app.create_user("user@example.com", "user", "password").await;
    
    for i in 1..=count {
        app.client
            .post(&app.url("/api/posts"))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "title": format!("Post {}", i),
                "body": "Body"
            }))
            .send()
            .await;
    }
    
    app
}

#[tokio::test]
async fn test_with_fixtures() {
    let app = setup_with_posts(10).await;
    // Test with pre-populated data
}
```

### Snapshot Testing

```rust
use insta::assert_json_snapshot;

#[tokio::test]
async fn test_api_response_format() {
    let app = TestApp::new().await;
    let response = app.client.get(&app.url("/api/posts")).send().await;
    let json: serde_json::Value = response.json().await;
    
    assert_json_snapshot!(json);
}
```

## Continuous Integration

### GitHub Actions

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
      - run: cargo test
```

---

## Summary

✅ Use `TestApp` for isolated integration tests  
✅ Test success and error cases  
✅ Test authentication flows  
✅ Test database operations  
✅ Use descriptive test names  
✅ Run tests in CI/CD  

Chopin makes testing fast, reliable, and easy!
