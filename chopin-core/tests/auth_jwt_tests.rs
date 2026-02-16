use chopin_core::auth::jwt::{create_token, validate_token, Claims};
use std::thread;
use std::time::Duration;

#[test]
fn test_create_and_validate_token() {
    let user_id = 42;
    let secret = "test-secret-key";
    let expiry_hours = 24;

    let token =
        create_token(&user_id.to_string(), secret, expiry_hours).expect("Failed to create token");
    assert!(!token.is_empty());

    let claims = validate_token(&token, secret).expect("Failed to validate token");
    assert_eq!(claims.sub, user_id.to_string());
}

#[test]
fn test_token_with_different_user_ids() {
    let secret = "test-secret";

    for user_id in [1, 100, 999, 12345] {
        let token = create_token(&user_id.to_string(), secret, 1).expect("Failed to create token");
        let claims = validate_token(&token, secret).expect("Failed to validate token");
        assert_eq!(claims.sub, user_id.to_string());
    }
}

#[test]
fn test_token_with_wrong_secret_fails() {
    let user_id = 1;
    let correct_secret = "correct-secret";
    let wrong_secret = "wrong-secret";

    let token =
        create_token(&user_id.to_string(), correct_secret, 1).expect("Failed to create token");

    let result = validate_token(&token, wrong_secret);
    assert!(result.is_err());
}

#[test]
fn test_invalid_token_format_fails() {
    let secret = "test-secret";

    let invalid_tokens = vec![
        "not.a.token",
        "random_string",
        "",
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.invalid",
    ];

    for token in invalid_tokens {
        let result = validate_token(token, secret);
        assert!(result.is_err(), "Should fail for invalid token: {}", token);
    }
}

#[test]
fn test_claims_structure() {
    let user_id = 100;
    let secret = "test-secret";

    let token = create_token(&user_id.to_string(), secret, 24).expect("Failed to create token");
    let claims = validate_token(&token, secret).expect("Failed to validate");

    assert_eq!(claims.sub, "100");
    assert!(claims.exp > claims.iat);
    assert!(claims.iat > 0);
    assert!(claims.exp > 0);
}

#[test]
fn test_token_expiry_time() {
    let user_id = 1;
    let secret = "test-secret";
    let expiry_hours = 2;

    let before = chrono::Utc::now().timestamp() as usize;
    let token =
        create_token(&user_id.to_string(), secret, expiry_hours).expect("Failed to create token");
    let after = chrono::Utc::now().timestamp() as usize;

    let claims = validate_token(&token, secret).expect("Failed to validate");

    // Expiry should be approximately 2 hours from now
    let expected_exp = before + (expiry_hours as usize * 3600);
    let exp_diff = (claims.exp as i64 - expected_exp as i64).abs();

    // Allow 2 second tolerance
    assert!(
        exp_diff < 2,
        "Expiry time difference too large: {}",
        exp_diff
    );

    // iat should be within test execution time
    assert!(claims.iat >= before);
    assert!(claims.iat <= after);
}

#[test]
fn test_expired_token_still_validates() {
    // Note: jsonwebtoken validates signature but doesn't enforce expiry by default
    // This test confirms the token structure is preserved even if "expired"
    let user_id = 1;
    let secret = "test-secret";

    // Create token that "expires" immediately (0 hours)
    let token = create_token(&user_id.to_string(), secret, 0).expect("Failed to create token");

    // Sleep a bit to ensure time has passed
    thread::sleep(Duration::from_millis(100));

    // Validation still works (because default Validation doesn't check exp)
    let claims = validate_token(&token, secret).expect("Should validate");
    assert_eq!(claims.sub, user_id.to_string());
}

#[test]
fn test_multiple_tokens_same_user() {
    let user_id = 42;
    let secret = "test-secret";

    let token1 = create_token(&user_id.to_string(), secret, 1).expect("Failed to create token 1");
    thread::sleep(Duration::from_secs(2)); // Sleep 2 seconds to ensure different iat timestamp
    let token2 = create_token(&user_id.to_string(), secret, 1).expect("Failed to create token 2");

    // Tokens should be different (different iat)
    assert_ne!(token1, token2);

    // Both should validate
    let claims1 = validate_token(&token1, secret).expect("Failed to validate token 1");
    let claims2 = validate_token(&token2, secret).expect("Failed to validate token 2");

    assert_eq!(claims1.sub, claims2.sub);
    assert!(claims2.iat >= claims1.iat);
}

#[test]
fn test_token_with_very_long_expiry() {
    let user_id = 1;
    let secret = "test-secret";
    let expiry_hours = 8760; // 1 year

    let token =
        create_token(&user_id.to_string(), secret, expiry_hours).expect("Failed to create token");
    let claims = validate_token(&token, secret).expect("Failed to validate");

    assert_eq!(claims.sub, user_id.to_string());

    let now = chrono::Utc::now().timestamp() as usize;
    let expected_exp = now + (expiry_hours as usize * 3600);
    let exp_diff = (claims.exp as i64 - expected_exp as i64).abs();

    assert!(exp_diff < 2);
}

#[test]
fn test_claims_serialization() {
    let claims = Claims {
        sub: "123".to_string(),
        exp: 9999999999,
        iat: 1234567890,
    };

    let json = serde_json::to_string(&claims).expect("Failed to serialize");
    assert!(json.contains("\"sub\":\"123\""));
    assert!(json.contains("\"exp\":9999999999"));
    assert!(json.contains("\"iat\":1234567890"));

    let deserialized: Claims = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.sub, claims.sub);
    assert_eq!(deserialized.exp, claims.exp);
    assert_eq!(deserialized.iat, claims.iat);
}

#[test]
fn test_token_with_special_characters_in_secret() {
    let user_id = 1;
    let secrets = vec![
        "simple",
        "with-dashes",
        "with_underscores",
        "with.dots",
        "CamelCase123",
        "with!@#$%special",
    ];

    for secret in secrets {
        let token = create_token(&user_id.to_string(), secret, 1).expect("Failed to create token");
        let claims = validate_token(&token, secret).expect("Failed to validate");
        assert_eq!(claims.sub, user_id.to_string());
    }
}
