use chopin_core::config::Config;
use std::env;

// Note: Config tests may fail if run in parallel due to shared environment state.
// In production, run: cargo test -- --test-threads=1

#[test]
#[ignore] // Ignore by default due to env var conflicts when running in parallel
fn test_config_defaults() {
    // Clear any existing env vars
    env::remove_var("DATABASE_URL");
    env::remove_var("JWT_SECRET");
    env::remove_var("JWT_EXPIRY_HOURS");
    env::remove_var("SERVER_HOST");
    env::remove_var("SERVER_PORT");
    env::remove_var("ENVIRONMENT");
    env::remove_var("REUSEPORT");

    let config = Config::from_env().expect("Failed to load config");

    assert_eq!(config.database_url, "sqlite://chopin.db?mode=rwc");
    assert_eq!(config.jwt_secret, "chopin-dev-secret-change-me");
    assert_eq!(config.jwt_expiry_hours, 24);
    assert_eq!(config.server_host, "127.0.0.1");
    assert_eq!(config.server_port, 3000);
    assert_eq!(config.environment, "development");
    assert!(!config.reuseport);
}

#[test]
#[ignore] // Ignore by default due to env var conflicts when running in parallel
fn test_config_from_env() {
    env::set_var("DATABASE_URL", "postgres://user:pass@localhost/testdb");
    env::set_var("JWT_SECRET", "test-secret-key");
    env::set_var("JWT_EXPIRY_HOURS", "48");
    env::set_var("SERVER_HOST", "0.0.0.0");
    env::set_var("SERVER_PORT", "8080");
    env::set_var("ENVIRONMENT", "production");
    env::set_var("REUSEPORT", "true");

    let config = Config::from_env().expect("Failed to load config");

    assert_eq!(config.database_url, "postgres://user:pass@localhost/testdb");
    assert_eq!(config.jwt_secret, "test-secret-key");
    assert_eq!(config.jwt_expiry_hours, 48);
    assert_eq!(config.server_host, "0.0.0.0");
    assert_eq!(config.server_port, 8080);
    assert_eq!(config.environment, "production");
    assert!(config.reuseport);

    // Cleanup
    env::remove_var("DATABASE_URL");
    env::remove_var("JWT_SECRET");
    env::remove_var("JWT_EXPIRY_HOURS");
    env::remove_var("SERVER_HOST");
    env::remove_var("SERVER_PORT");
    env::remove_var("ENVIRONMENT");
    env::remove_var("REUSEPORT");
}

#[test]
fn test_reuseport_variations() {
    let test_cases = vec![
        ("true", true),
        ("True", true),
        ("TRUE", true),
        ("1", true),
        ("yes", true),
        ("YES", true),
        ("false", false),
        ("0", false),
        ("no", false),
        ("", false),
    ];

    for (value, expected) in test_cases {
        env::set_var("REUSEPORT", value);
        let config = Config::from_env().expect("Failed to load config");
        assert_eq!(
            config.reuseport, expected,
            "REUSEPORT={} should be {}",
            value, expected
        );
    }

    env::remove_var("REUSEPORT");
}

#[test]
fn test_optional_redis_url() {
    env::remove_var("REDIS_URL");
    let config = Config::from_env().expect("Failed to load config");
    assert!(config.redis_url.is_none());

    env::set_var("REDIS_URL", "redis://localhost:6379");
    let config = Config::from_env().expect("Failed to load config");
    assert_eq!(config.redis_url, Some("redis://localhost:6379".to_string()));

    env::remove_var("REDIS_URL");
}

#[test]
fn test_upload_settings() {
    env::set_var("UPLOAD_DIR", "/var/uploads");
    env::set_var("MAX_UPLOAD_SIZE", "52428800"); // 50MB

    let config = Config::from_env().expect("Failed to load config");
    assert_eq!(config.upload_dir, "/var/uploads");
    assert_eq!(config.max_upload_size, 52428800);

    env::remove_var("UPLOAD_DIR");
    env::remove_var("MAX_UPLOAD_SIZE");
}

#[test]
fn test_s3_configuration() {
    env::set_var("S3_BUCKET", "my-bucket");
    env::set_var("S3_REGION", "us-west-2");
    env::set_var("S3_ENDPOINT", "https://s3.example.com");
    env::set_var("S3_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
    env::set_var(
        "S3_SECRET_ACCESS_KEY",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    );
    env::set_var("S3_PUBLIC_URL", "https://cdn.example.com");
    env::set_var("S3_PREFIX", "uploads/");

    let config = Config::from_env().expect("Failed to load config");
    assert_eq!(config.s3_bucket, Some("my-bucket".to_string()));
    assert_eq!(config.s3_region, Some("us-west-2".to_string()));
    assert_eq!(
        config.s3_endpoint,
        Some("https://s3.example.com".to_string())
    );
    assert_eq!(
        config.s3_access_key_id,
        Some("AKIAIOSFODNN7EXAMPLE".to_string())
    );
    assert_eq!(
        config.s3_secret_access_key,
        Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string())
    );
    assert_eq!(
        config.s3_public_url,
        Some("https://cdn.example.com".to_string())
    );
    assert_eq!(config.s3_prefix, Some("uploads/".to_string()));

    // Cleanup
    env::remove_var("S3_BUCKET");
    env::remove_var("S3_REGION");
    env::remove_var("S3_ENDPOINT");
    env::remove_var("S3_ACCESS_KEY_ID");
    env::remove_var("S3_SECRET_ACCESS_KEY");
    env::remove_var("S3_PUBLIC_URL");
    env::remove_var("S3_PREFIX");
}

#[test]
fn test_invalid_port_uses_default() {
    env::set_var("SERVER_PORT", "invalid");
    let config = Config::from_env().expect("Failed to load config");
    assert_eq!(config.server_port, 3000); // Should use default
    env::remove_var("SERVER_PORT");
}

#[test]
fn test_invalid_jwt_expiry_uses_default() {
    env::set_var("JWT_EXPIRY_HOURS", "not_a_number");
    let config = Config::from_env().expect("Failed to load config");
    assert_eq!(config.jwt_expiry_hours, 24); // Should use default
    env::remove_var("JWT_EXPIRY_HOURS");
}
