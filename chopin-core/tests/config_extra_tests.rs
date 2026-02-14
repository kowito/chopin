use chopin_core::config::Config;

/// Build a test Config struct with known values.
fn test_config() -> Config {
    Config {
        reuseport: false,
        database_url: "sqlite::memory:".to_string(),
        jwt_secret: "test-secret".to_string(),
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

// ═══ is_dev ═══

#[test]
fn test_is_dev_true_when_development() {
    let mut config = test_config();
    config.environment = "development".to_string();
    assert!(config.is_dev());
}

#[test]
fn test_is_dev_false_for_production() {
    let mut config = test_config();
    config.environment = "production".to_string();
    assert!(!config.is_dev());
}

#[test]
fn test_is_dev_false_for_test() {
    let config = test_config(); // environment = "test"
    assert!(!config.is_dev());
}

#[test]
fn test_is_dev_false_for_staging() {
    let mut config = test_config();
    config.environment = "staging".to_string();
    assert!(!config.is_dev());
}

#[test]
fn test_is_dev_case_sensitive() {
    let mut config = test_config();
    config.environment = "Development".to_string();
    assert!(!config.is_dev(), "is_dev should be case-sensitive");
}

// ═══ has_s3 ═══

#[test]
fn test_has_s3_false_when_no_bucket() {
    let config = test_config(); // s3_bucket = None
    assert!(!config.has_s3());
}

#[test]
fn test_has_s3_true_when_bucket_set() {
    let mut config = test_config();
    config.s3_bucket = Some("my-bucket".to_string());
    assert!(config.has_s3());
}

#[test]
fn test_has_s3_true_even_without_other_s3_fields() {
    let mut config = test_config();
    config.s3_bucket = Some("bucket".to_string());
    // Other S3 fields are None
    assert!(config.has_s3());
}

// ═══ server_addr ═══

#[test]
fn test_server_addr_default() {
    let config = test_config();
    assert_eq!(config.server_addr(), "127.0.0.1:3000");
}

#[test]
fn test_server_addr_custom() {
    let mut config = test_config();
    config.server_host = "0.0.0.0".to_string();
    config.server_port = 8080;
    assert_eq!(config.server_addr(), "0.0.0.0:8080");
}

#[test]
fn test_server_addr_ipv6() {
    let mut config = test_config();
    config.server_host = "::1".to_string();
    config.server_port = 443;
    assert_eq!(config.server_addr(), "::1:443");
}

// ═══ Config clone ═══

#[test]
fn test_config_clone() {
    let config = test_config();
    let cloned = config.clone();
    assert_eq!(config.database_url, cloned.database_url);
    assert_eq!(config.jwt_secret, cloned.jwt_secret);
    assert_eq!(config.server_port, cloned.server_port);
    assert_eq!(config.environment, cloned.environment);
}

// ═══ Config debug ═══

#[test]
fn test_config_debug() {
    let config = test_config();
    let debug = format!("{:?}", config);
    assert!(debug.contains("Config"));
    assert!(debug.contains("sqlite::memory:"));
}

// ═══ Config fields ═══

#[test]
fn test_config_reuseport_default() {
    let config = test_config();
    assert!(!config.reuseport);
}

#[test]
fn test_config_max_upload_size() {
    let config = test_config();
    assert_eq!(config.max_upload_size, 10 * 1024 * 1024);
}

#[test]
fn test_config_all_s3_fields() {
    let config = Config {
        reuseport: false,
        database_url: "sqlite::memory:".to_string(),
        jwt_secret: "s".to_string(),
        jwt_expiry_hours: 1,
        server_host: "127.0.0.1".to_string(),
        server_port: 3000,
        environment: "test".to_string(),
        redis_url: None,
        upload_dir: "./uploads".to_string(),
        max_upload_size: 1024,
        s3_bucket: Some("bucket".to_string()),
        s3_region: Some("us-east-1".to_string()),
        s3_endpoint: Some("https://s3.example.com".to_string()),
        s3_access_key_id: Some("AKIA123".to_string()),
        s3_secret_access_key: Some("secret123".to_string()),
        s3_public_url: Some("https://cdn.example.com".to_string()),
        s3_prefix: Some("uploads/".to_string()),
    };
    assert!(config.has_s3());
    assert_eq!(config.s3_region.as_deref(), Some("us-east-1"));
    assert_eq!(config.s3_prefix.as_deref(), Some("uploads/"));
}
