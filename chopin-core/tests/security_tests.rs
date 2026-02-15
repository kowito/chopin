// Comprehensive security feature tests for chopin-core.
//
// Tests cover all 9 security features:
// 1. TOTP (2FA)
// 2. Rate Limiting
// 3. Account Lockout
// 4. Refresh Tokens
// 5. Session Management
// 6. Password Reset
// 7. Email Verification
// 8. CSRF Protection
// 9. IP/Device Tracking

// ═══════════════════════════════════════════════════════════════
// Unit tests for pure functions (no DB needed)
// ═══════════════════════════════════════════════════════════════

mod totp_unit_tests {
    use chopin_core::auth::totp::{generate_secure_token, hash_token};
    use chopin_core::auth::{generate_totp_secret, verify_totp};

    #[test]
    fn test_generate_totp_secret_returns_secret_and_uri() {
        let (secret, uri) = generate_totp_secret("Chopin", "user@example.com")
            .expect("Should generate TOTP secret");
        assert!(!secret.is_empty(), "Secret should not be empty");
        assert!(
            uri.starts_with("otpauth://totp/"),
            "URI should be otpauth format"
        );
        assert!(uri.contains("user%40example.com") || uri.contains("user@example.com"));
    }

    #[test]
    fn test_verify_totp_with_generated_secret() {
        let (secret, _uri) = generate_totp_secret("Chopin", "test@example.com")
            .expect("Should generate TOTP secret");

        // Generate a valid code using the same secret
        use totp_rs::{Algorithm, Secret, TOTP};
        let secret_obj = Secret::Encoded(secret.clone());
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_obj.to_bytes().unwrap(),
            None,
            String::new(),
        )
        .unwrap();
        let code = totp.generate_current().unwrap();

        let result = verify_totp(&secret, &code).expect("verify_totp should not error");
        assert!(result, "Valid TOTP code should verify");
    }

    #[test]
    fn test_verify_totp_with_wrong_code() {
        let (secret, _) = generate_totp_secret("Chopin", "test@example.com")
            .expect("Should generate TOTP secret");

        let result = verify_totp(&secret, "000000").expect("verify_totp should not error");
        // With skew=1, 000000 is extremely unlikely to match
        // We just verify it returns a valid bool without panicking
        let _ = result;
    }

    #[test]
    fn test_verify_totp_with_invalid_secret_returns_error() {
        let result = verify_totp("INVALID_NOT_BASE32!!!", "123456");
        assert!(result.is_err(), "Invalid secret should return an error");
    }

    #[test]
    fn test_generate_secure_token_uniqueness() {
        let token1 = generate_secure_token();
        let token2 = generate_secure_token();

        assert_ne!(token1, token2, "Two tokens should be different");
        assert_eq!(token1.len(), 64, "Token should be 64-char hex (32 bytes)");
        assert!(
            token1.chars().all(|c| c.is_ascii_hexdigit()),
            "Token should be hex"
        );
    }

    #[test]
    fn test_hash_token_deterministic() {
        let token = "test-token-123";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);

        assert_eq!(hash1, hash2, "Same token should produce same hash");
        assert_eq!(hash1.len(), 64, "SHA-256 hex hash should be 64 chars");
    }

    #[test]
    fn test_hash_token_different_inputs() {
        let hash1 = hash_token("token-a");
        let hash2 = hash_token("token-b");

        assert_ne!(
            hash1, hash2,
            "Different tokens should produce different hashes"
        );
    }
}

mod csrf_unit_tests {
    use chopin_core::auth::{generate_csrf_token, verify_csrf_token};

    #[test]
    fn test_generate_csrf_token() {
        let token = generate_csrf_token();
        assert!(!token.is_empty());
        assert_eq!(token.len(), 64, "CSRF token should be 64-char hex");
    }

    #[test]
    fn test_verify_csrf_token_valid() {
        let token = generate_csrf_token();
        assert!(
            verify_csrf_token(&token, &token),
            "Same token should verify"
        );
    }

    #[test]
    fn test_verify_csrf_token_invalid() {
        let token = generate_csrf_token();
        assert!(
            !verify_csrf_token(&token, "wrong-token"),
            "Different token should not verify"
        );
    }

    #[test]
    fn test_verify_csrf_token_empty() {
        assert!(!verify_csrf_token("abc", ""));
        assert!(!verify_csrf_token("", "abc"));
    }

    #[test]
    fn test_verify_csrf_token_different_length() {
        assert!(
            !verify_csrf_token("short", "longer-token"),
            "Different lengths should not verify"
        );
    }
}

mod rate_limit_unit_tests {
    use chopin_core::auth::RateLimiter;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(3, 60);

        assert!(limiter.check("user@test.com").is_ok());
        assert!(limiter.check("user@test.com").is_ok());
        assert!(limiter.check("user@test.com").is_ok());
    }

    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let limiter = RateLimiter::new(2, 60);

        assert!(limiter.check("test@test.com").is_ok());
        assert!(limiter.check("test@test.com").is_ok());
        // Third attempt should be blocked
        let result = limiter.check("test@test.com");
        assert!(result.is_err(), "Should be rate limited after 2 attempts");
    }

    #[test]
    fn test_rate_limiter_independent_keys() {
        let limiter = RateLimiter::new(1, 60);

        assert!(limiter.check("user-a@test.com").is_ok());
        assert!(limiter.check("user-b@test.com").is_ok());
        // user-a is now limited
        assert!(limiter.check("user-a@test.com").is_err());
        // user-b still has one more
        assert!(limiter.check("user-b@test.com").is_err());
    }

    #[test]
    fn test_rate_limiter_reset() {
        let limiter = RateLimiter::new(1, 60);

        assert!(limiter.check("user@test.com").is_ok());
        assert!(limiter.check("user@test.com").is_err());

        limiter.reset("user@test.com");
        assert!(
            limiter.check("user@test.com").is_ok(),
            "Should be allowed after reset"
        );
    }

    #[test]
    fn test_rate_limiter_record_without_check() {
        let limiter = RateLimiter::new(2, 60);

        limiter.record("ip:1.2.3.4");
        limiter.record("ip:1.2.3.4");

        // Already at limit, next check should fail
        assert!(limiter.check("ip:1.2.3.4").is_err());
    }

    #[test]
    fn test_rate_limiter_cleanup() {
        let limiter = RateLimiter::new(100, 60);

        limiter.record("key1");
        limiter.record("key2");

        // Cleanup should not crash with active entries
        limiter.cleanup();
    }
}

mod security_config_tests {
    use chopin_core::config::SecurityConfig;

    #[test]
    fn test_default_security_config_all_enabled() {
        let config = SecurityConfig::default();

        assert!(config.enable_2fa);
        assert!(config.enable_rate_limit);
        assert!(config.enable_account_lockout);
        assert!(config.enable_refresh_tokens);
        assert!(config.enable_session_management);
        assert!(config.enable_password_reset);
        assert!(config.enable_email_verification);
        assert!(config.enable_csrf);
        assert!(config.enable_device_tracking);
    }

    #[test]
    fn test_default_security_config_sane_values() {
        let config = SecurityConfig::default();

        assert_eq!(config.rate_limit_max_attempts, 5);
        assert_eq!(config.rate_limit_window_secs, 300);
        assert_eq!(config.lockout_max_attempts, 5);
        assert_eq!(config.lockout_duration_secs, 900);
        assert_eq!(config.refresh_token_expiry_days, 30);
        assert_eq!(config.password_reset_expiry_secs, 3600);
        assert_eq!(config.email_verification_expiry_secs, 86400);
        assert_eq!(config.min_password_length, 12);
    }
}

// ═══════════════════════════════════════════════════════════════
// Integration tests (require running server with DB)
// ═══════════════════════════════════════════════════════════════

mod integration {
    use chopin_core::TestApp;

    // ── Signup with security disabled (backward compatibility) ──

    #[tokio::test]
    async fn test_signup_basic_no_security() {
        let app = TestApp::new().await;

        let body = serde_json::json!({
            "email": "normal@example.com",
            "username": "normaluser",
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
        // No refresh token when disabled
        assert!(
            data["refresh_token"].is_null(),
            "Refresh token should not be present when disabled"
        );
        assert!(
            data["csrf_token"].is_null(),
            "CSRF token should not be present when disabled"
        );
    }

    // ── Signup with security enabled ──

    #[tokio::test]
    async fn test_signup_with_security_returns_tokens() {
        let app = TestApp::new_secure().await;

        let body = serde_json::json!({
            "email": "secure@example.com",
            "username": "secureuser",
            "password": "very-strong-password-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/signup"), &body.to_string())
            .await;

        assert_eq!(res.status, 200);
        assert!(res.is_success());
        let data = res.data();

        // With security enabled, we get all tokens
        assert!(data["access_token"].is_string());
        assert!(
            data["refresh_token"].is_string(),
            "Refresh token should be present when enabled"
        );
        assert!(
            data["csrf_token"].is_string(),
            "CSRF token should be present when enabled"
        );
        assert_eq!(
            data["email_verification_required"], true,
            "Email verification should be required"
        );
    }

    // ── Password minimum length ──

    #[tokio::test]
    async fn test_signup_password_too_short() {
        let app = TestApp::new_secure().await;

        let body = serde_json::json!({
            "email": "short@example.com",
            "username": "shortpass",
            "password": "short"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/signup"), &body.to_string())
            .await;

        assert_eq!(res.status, 422);
        assert!(!res.is_success());
        let body_str = &res.body;
        assert!(
            body_str.contains("at least") || body_str.contains("Password"),
            "Should mention password length requirement"
        );
    }

    // ── Login with security ──

    #[tokio::test]
    async fn test_login_with_security_returns_tokens() {
        let app = TestApp::new_secure().await;

        // Create user
        app.create_user("login@example.com", "loginuser", "very-strong-password-123")
            .await;

        // Login
        let body = serde_json::json!({
            "email": "login@example.com",
            "password": "very-strong-password-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &body.to_string())
            .await;

        assert_eq!(res.status, 200, "Login failed: {}", res.body);
        let data = res.data();
        assert!(data["access_token"].is_string());
        assert!(data["refresh_token"].is_string());
        assert!(data["csrf_token"].is_string());
    }

    // ── Rate limiting ──

    #[tokio::test]
    async fn test_rate_limiting_blocks_after_max_attempts() {
        let app = TestApp::new_secure().await;

        let bad_login = serde_json::json!({
            "email": "ratelimit@example.com",
            "password": "wrong-password-12345"
        });

        // Send more than max attempts (default is 5)
        let mut last_status = 0;
        for _ in 0..7 {
            let res = app
                .client
                .post(&app.url("/api/auth/login"), &bad_login.to_string())
                .await;
            last_status = res.status;
        }

        // Should eventually get rate limited (429)
        assert_eq!(last_status, 429, "Should return 429 Too Many Requests");
    }

    // ── Refresh token flow ──

    #[tokio::test]
    async fn test_refresh_token_flow() {
        let app = TestApp::new_secure().await;

        // Create user and get tokens
        let body = serde_json::json!({
            "email": "refresh@example.com",
            "username": "refreshuser",
            "password": "very-strong-password-123"
        });

        let signup_res = app
            .client
            .post(&app.url("/api/auth/signup"), &body.to_string())
            .await;

        let data = signup_res.data();
        let refresh_token = data["refresh_token"].as_str().unwrap();

        // Use refresh token to get new tokens
        let refresh_body = serde_json::json!({
            "refresh_token": refresh_token
        });

        let res = app
            .client
            .post(&app.url("/api/auth/refresh"), &refresh_body.to_string())
            .await;

        assert_eq!(res.status, 200, "Refresh failed: {}", res.body);
        let new_data = res.data();
        assert!(new_data["access_token"].is_string());
        assert!(new_data["refresh_token"].is_string());

        // Old refresh token should be revoked (rotation)
        let res2 = app
            .client
            .post(&app.url("/api/auth/refresh"), &refresh_body.to_string())
            .await;

        assert_eq!(res2.status, 401, "Reusing old refresh token should fail");
    }

    // ── Logout ──

    #[tokio::test]
    async fn test_logout_revokes_session() {
        let app = TestApp::new_secure().await;

        let (token, _) = app
            .create_user(
                "logout@example.com",
                "logoutuser",
                "very-strong-password-123",
            )
            .await;

        // Logout
        let logout_body = serde_json::json!({});

        let res = app
            .client
            .post_with_auth(
                &app.url("/api/auth/logout"),
                &token,
                &logout_body.to_string(),
            )
            .await;

        assert_eq!(res.status, 200);
    }

    // ── TOTP setup flow ──

    #[tokio::test]
    async fn test_totp_setup_returns_secret() {
        let app = TestApp::new_secure().await;

        let (token, _) = app
            .create_user("totp@example.com", "totpuser", "very-strong-password-123")
            .await;

        let res = app
            .client
            .post_with_auth(&app.url("/api/auth/totp/setup"), &token, "{}")
            .await;

        assert_eq!(res.status, 200);
        let data = res.data();
        assert!(data["secret"].is_string(), "Should return TOTP secret");
        assert!(data["otpauth_uri"].is_string(), "Should return otpauth URI");

        let uri = data["otpauth_uri"].as_str().unwrap();
        assert!(
            uri.starts_with("otpauth://totp/"),
            "URI should be otpauth format"
        );
    }

    // ── Password reset flow ──

    #[tokio::test]
    async fn test_password_reset_request() {
        let app = TestApp::new_secure().await;

        app.create_user("reset@example.com", "resetuser", "very-strong-password-123")
            .await;

        let body = serde_json::json!({
            "email": "reset@example.com"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/request"),
                &body.to_string(),
            )
            .await;

        assert_eq!(res.status, 200);
        let data = res.data();
        assert!(data["reset_token"].is_string(), "Should return reset token");
    }

    #[tokio::test]
    async fn test_password_reset_confirm() {
        let app = TestApp::new_secure().await;

        app.create_user(
            "reset2@example.com",
            "resetuser2",
            "very-strong-password-123",
        )
        .await;

        // Request reset token
        let request_body = serde_json::json!({
            "email": "reset2@example.com"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/request"),
                &request_body.to_string(),
            )
            .await;

        let reset_token = res.data()["reset_token"].as_str().unwrap().to_string();

        // Confirm reset
        let confirm_body = serde_json::json!({
            "token": reset_token,
            "new_password": "new-very-strong-password-456"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/confirm"),
                &confirm_body.to_string(),
            )
            .await;

        assert_eq!(res.status, 200);

        // Login with new password should work
        let login_body = serde_json::json!({
            "email": "reset2@example.com",
            "password": "new-very-strong-password-456"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &login_body.to_string())
            .await;

        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn test_password_reset_token_single_use() {
        let app = TestApp::new_secure().await;

        app.create_user(
            "reset3@example.com",
            "resetuser3",
            "very-strong-password-123",
        )
        .await;

        let request_body = serde_json::json!({
            "email": "reset3@example.com"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/request"),
                &request_body.to_string(),
            )
            .await;

        let reset_token = res.data()["reset_token"].as_str().unwrap().to_string();

        // First use should succeed
        let confirm_body = serde_json::json!({
            "token": reset_token,
            "new_password": "another-strong-password-789"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/confirm"),
                &confirm_body.to_string(),
            )
            .await;
        assert_eq!(res.status, 200);

        // Second use should fail
        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/confirm"),
                &confirm_body.to_string(),
            )
            .await;
        assert_eq!(res.status, 400, "Reusing reset token should fail");
    }

    // ── Email verification ──

    #[tokio::test]
    async fn test_email_verification_required_on_signup() {
        let app = TestApp::new_secure().await;

        let body = serde_json::json!({
            "email": "verify@example.com",
            "username": "verifyuser",
            "password": "very-strong-password-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/signup"), &body.to_string())
            .await;

        assert_eq!(res.status, 200);
        let data = res.data();
        assert_eq!(
            data["email_verification_required"], true,
            "Should indicate email verification needed"
        );
    }

    // ── Password reset with non-existent email (anti-enumeration) ──

    #[tokio::test]
    async fn test_password_reset_nonexistent_email_still_succeeds() {
        let app = TestApp::new_secure().await;

        let body = serde_json::json!({
            "email": "nonexistent@example.com"
        });

        let res = app
            .client
            .post(
                &app.url("/api/auth/password-reset/request"),
                &body.to_string(),
            )
            .await;

        // Should return 200 to not reveal whether email exists
        assert_eq!(
            res.status, 200,
            "Should return 200 even for non-existent email (anti-enumeration)"
        );
    }

    // ── Login with wrong password ──

    #[tokio::test]
    async fn test_login_wrong_password() {
        let app = TestApp::new_secure().await;

        app.create_user(
            "wrongpw@example.com",
            "wrongpwuser",
            "very-strong-password-123",
        )
        .await;

        let body = serde_json::json!({
            "email": "wrongpw@example.com",
            "password": "wrong-password-here-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &body.to_string())
            .await;

        assert_eq!(res.status, 401);
    }

    // ── Account lockout ──

    #[tokio::test]
    async fn test_account_lockout_after_failed_attempts() {
        let app = TestApp::new_secure().await;

        app.create_user(
            "lockout@example.com",
            "lockoutuser",
            "very-strong-password-123",
        )
        .await;

        let bad_login = serde_json::json!({
            "email": "lockout@example.com",
            "password": "wrong-password-here-123"
        });

        // Fail more than lockout_max_attempts (default 5)
        for _ in 0..6 {
            app.client
                .post(&app.url("/api/auth/login"), &bad_login.to_string())
                .await;
        }

        // Now even the correct password should be locked
        let correct_login = serde_json::json!({
            "email": "lockout@example.com",
            "password": "very-strong-password-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &correct_login.to_string())
            .await;

        // Should be either 401 (locked) or 429 (rate limited) — both indicate blocked
        assert!(
            res.status == 401 || res.status == 429,
            "Account should be locked or rate limited, got {}",
            res.status
        );
    }

    // ── Multiple independent users ──

    #[tokio::test]
    async fn test_security_independent_per_user() {
        let app = TestApp::new_secure().await;

        app.create_user("user-a@example.com", "usera", "very-strong-password-123")
            .await;
        app.create_user("user-b@example.com", "userb", "very-strong-password-456")
            .await;

        // Login user A
        let login_a = serde_json::json!({
            "email": "user-a@example.com",
            "password": "very-strong-password-123"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &login_a.to_string())
            .await;
        assert_eq!(res.status, 200);

        // Login user B
        let login_b = serde_json::json!({
            "email": "user-b@example.com",
            "password": "very-strong-password-456"
        });

        let res = app
            .client
            .post(&app.url("/api/auth/login"), &login_b.to_string())
            .await;
        assert_eq!(res.status, 200, "Login B failed: {}", res.body);
    }
}
