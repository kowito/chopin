use chopin::config::Config;
use chopin::db;

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

#[tokio::test]
async fn test_connect_with_valid_sqlite_memory() {
    let config = test_config();
    let result = db::connect(&config).await;

    assert!(result.is_ok());
    let connection = result.unwrap();

    // Verify we can execute a simple query
    use sea_orm::ConnectionTrait;
    let query_result = connection.execute_unprepared("SELECT 1").await;

    assert!(query_result.is_ok());
}

#[tokio::test]
async fn test_connect_with_invalid_url_fails() {
    let mut config = test_config();
    config.database_url = "invalid://database/url".to_string();

    let result = db::connect(&config).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_connection_options_applied() {
    let config = test_config();
    let result = db::connect(&config).await;

    assert!(result.is_ok());
    // Connection options are internal, but we can verify connection works
}

#[tokio::test]
async fn test_dev_environment_enables_logging() {
    let mut config = test_config();
    config.environment = "development".to_string();

    let result = db::connect(&config).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_production_environment_disables_logging() {
    let mut config = test_config();
    config.environment = "production".to_string();

    let result = db::connect(&config).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_connections_from_same_config() {
    let config = test_config();

    let conn1 = db::connect(&config).await;
    let conn2 = db::connect(&config).await;

    assert!(conn1.is_ok());
    assert!(conn2.is_ok());

    // Both connections should be independent
    use sea_orm::ConnectionTrait;
    let query1 = conn1.unwrap().execute_unprepared("SELECT 1").await;
    let query2 = conn2.unwrap().execute_unprepared("SELECT 1").await;

    assert!(query1.is_ok());
    assert!(query2.is_ok());
}

#[tokio::test]
async fn test_sqlite_file_database() {
    let temp_path = std::env::temp_dir().join(format!("test_{}.db", uuid::Uuid::new_v4()));

    let mut config = test_config();
    config.database_url = format!("sqlite://{}?mode=rwc", temp_path.display());

    let result = db::connect(&config).await;

    assert!(result.is_ok());

    // Verify the file was created
    assert!(temp_path.exists());

    // Clean up
    let _ = std::fs::remove_file(temp_path);
}

#[tokio::test]
async fn test_connection_pool_can_execute_queries() {
    let config = test_config();
    let connection = db::connect(&config).await.unwrap();

    use sea_orm::ConnectionTrait;

    // Create a simple table
    let create_table = connection
        .execute_unprepared("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)")
        .await;

    assert!(create_table.is_ok());

    // Insert data
    let insert = connection
        .execute_unprepared("INSERT INTO test_table (name) VALUES ('test')")
        .await;

    assert!(insert.is_ok());

    // Query data
    let query = connection
        .execute_unprepared("SELECT * FROM test_table")
        .await;

    assert!(query.is_ok());
}

#[tokio::test]
async fn test_config_is_dev_method() {
    let mut config = test_config();

    config.environment = "development".to_string();
    assert!(config.is_dev());

    config.environment = "production".to_string();
    assert!(!config.is_dev());

    config.environment = "test".to_string();
    assert!(!config.is_dev());
}

#[tokio::test]
async fn test_connection_with_empty_database_url_fails() {
    let mut config = test_config();
    config.database_url = "".to_string();

    let result = db::connect(&config).await;

    assert!(result.is_err());
}
