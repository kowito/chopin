use axum::{
    body::Body,
    extract::FromRequestParts,
    http::Request,
};
use std::sync::Arc;

use chopin::auth;
use chopin::config::Config;
use chopin::error::ChopinError;
use chopin::extractors::auth_user::AuthUser;

fn test_config() -> Config {
    Config {
        reuseport: false,
        database_url: "sqlite::memory:".to_string(),
        jwt_secret: "test-secret-key".to_string(),
        jwt_expiry_hours: 24,
        server_host: "127.0.0.1".to_string(),
        server_port: 3000,
        environment: "test".to_string(),
        redis_url: None,
        upload_dir: "./uploads".to_string(),
        max_upload_size: 10 * 1024 * 1024,
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3_access_key_id: None,
        s3_secret_access_key: None,
        s3_public_url: None,
        s3_prefix: None,
    }
}

#[tokio::test]
async fn test_valid_bearer_token_extracts_user_id() {
    let config = Arc::new(test_config());
    let user_id = 42;
    let token = auth::create_token(user_id, &config.jwt_secret, 1).expect("Failed to create token");

    let mut req = Request::builder()
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_ok());
    let AuthUser(extracted_id) = result.unwrap();
    assert_eq!(extracted_id, user_id);
}

#[tokio::test]
async fn test_missing_authorization_header_fails() {
    let config = Arc::new(test_config());

    let mut req = Request::builder().body(Body::empty()).unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ChopinError::Unauthorized(msg) => {
            assert!(msg.contains("Missing Authorization header"));
        }
        _ => panic!("Expected Unauthorized error"),
    }
}

#[tokio::test]
async fn test_invalid_bearer_format_fails() {
    let config = Arc::new(test_config());

    let mut req = Request::builder()
        .header("Authorization", "InvalidFormat token123")
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ChopinError::Unauthorized(msg) => {
            assert!(msg.contains("Invalid Authorization header format"));
        }
        _ => panic!("Expected Unauthorized error"),
    }
}

#[tokio::test]
async fn test_bearer_without_token_fails() {
    let config = Arc::new(test_config());

    let mut req = Request::builder()
        .header("Authorization", "Bearer ")
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_token_fails() {
    let config = Arc::new(test_config());

    let mut req = Request::builder()
        .header("Authorization", "Bearer invalid.token.here")
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ChopinError::Unauthorized(_) => {},
        _ => panic!("Expected Unauthorized error"),
    }
}

#[tokio::test]
async fn test_token_with_wrong_secret_fails() {
    let config = Arc::new(test_config());
    let user_id = 42;
    
    // Create token with different secret
    let token = auth::create_token(user_id, "wrong-secret", 1).expect("Failed to create token");

    let mut req = Request::builder()
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_missing_config_in_extensions_fails() {
    let user_id = 42;
    let token = auth::create_token(user_id, "test-secret", 1).expect("Failed to create token");

    let req = Request::builder()
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Don't insert config into extensions

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ChopinError::Internal(msg) => {
            assert!(msg.contains("Config not found"));
        }
        _ => panic!("Expected Internal error"),
    }
}

#[tokio::test]
async fn test_different_user_ids_extract_correctly() {
    let config = Arc::new(test_config());

    let user_ids = vec![1, 42, 999, 10000];

    for user_id in user_ids {
        let token = auth::create_token(user_id, &config.jwt_secret, 1).expect("Failed to create token");

        let mut req = Request::builder()
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        req.extensions_mut().insert(config.clone());

        let (mut parts, _body) = req.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_ok());
        let AuthUser(extracted_id) = result.unwrap();
        assert_eq!(extracted_id, user_id);
    }
}

#[tokio::test]
async fn test_auth_user_is_clone_and_debug() {
    let auth_user = AuthUser(42);
    let cloned = auth_user.clone();
    
    assert_eq!(auth_user.0, cloned.0);
    
    let debug_str = format!("{:?}", auth_user);
    assert!(debug_str.contains("AuthUser"));
    assert!(debug_str.contains("42"));
}

#[tokio::test]
async fn test_case_sensitive_bearer_prefix() {
    let config = Arc::new(test_config());
    let user_id = 42;
    let token = auth::create_token(user_id, &config.jwt_secret, 1).expect("Failed to create token");

    // Try with lowercase "bearer"
    let mut req = Request::builder()
        .header("Authorization", format!("bearer {}", token))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    // Should fail because prefix is case-sensitive
    assert!(result.is_err());
}

#[tokio::test]
async fn test_expired_token_fails() {
    let config = Arc::new(test_config());
    let user_id = 42;
    
    // Create token with 0 hour expiry (expires immediately)
    let token = auth::create_token(user_id, &config.jwt_secret, 0).expect("Failed to create token");

    let mut req = Request::builder()
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_authorization_header_with_extra_spaces() {
    let config = Arc::new(test_config());
    let user_id = 42;
    let token = auth::create_token(user_id, &config.jwt_secret, 1).expect("Failed to create token");

    // Extra spaces in the header value
    let mut req = Request::builder()
        .header("Authorization", format!("Bearer  {}", token))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(config);

    let (mut parts, _body) = req.into_parts();

    let result = AuthUser::from_request_parts(&mut parts, &()).await;

    // Should fail because of extra space
    assert!(result.is_err());
}
