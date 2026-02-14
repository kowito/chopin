/// Tests for the lib.rs re-exports to verify that public API surface is correct.
/// These are compile-time tests — if they compile, the re-exports work.

// ═══ Core type re-exports ═══

#[test]
fn test_config_reexport() {
    let _ = chopin_core::Config {
        reuseport: false,
        database_url: "sqlite::memory:".to_string(),
        jwt_secret: "s".to_string(),
        jwt_expiry_hours: 24,
        server_host: "127.0.0.1".to_string(),
        server_port: 3000,
        environment: "test".to_string(),
        redis_url: None,
        upload_dir: "./uploads".to_string(),
        max_upload_size: 1024,
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3_access_key_id: None,
        s3_secret_access_key: None,
        s3_public_url: None,
        s3_prefix: None,
    };
}

#[test]
fn test_fast_route_reexport() {
    let _ = chopin_core::FastRoute::json("/test", b"{}");
}

#[test]
fn test_chopin_error_reexport() {
    let _ = chopin_core::ChopinError::NotFound("test".into());
}

#[test]
fn test_api_response_reexport() {
    let _ = chopin_core::ApiResponse::success("hello".to_string());
}

#[test]
fn test_cache_service_reexport() {
    let _ = chopin_core::CacheService::in_memory();
}

// ═══ Axum re-exports ═══

#[test]
fn test_router_reexport() {
    let _: chopin_core::Router = chopin_core::Router::new();
}

#[test]
fn test_status_code_reexport() {
    let _ = chopin_core::StatusCode::OK;
    let _ = chopin_core::StatusCode::NOT_FOUND;
    let _ = chopin_core::StatusCode::INTERNAL_SERVER_ERROR;
}

#[test]
fn test_method_reexport() {
    let _ = chopin_core::Method::GET;
    let _ = chopin_core::Method::POST;
    let _ = chopin_core::Method::DELETE;
}

#[test]
fn test_header_map_reexport() {
    let _ = chopin_core::HeaderMap::new();
}

// ═══ Module access ═══

#[test]
fn test_auth_module_accessible() {
    // Should be able to reference auth submodules
    let _token = chopin_core::auth::create_token(1, "test-secret", 24);
}

#[test]
fn test_json_module_accessible() {
    // Should be able to use the json module
    let mut buf = Vec::new();
    let _ = chopin_core::json::to_writer(&mut buf, &"hello");
}

#[test]
fn test_storage_module_accessible() {
    let _ = chopin_core::storage::LocalStorage::new("./uploads");
}

#[test]
fn test_error_module_accessible() {
    let _ = chopin_core::error::FieldError::new("field", "message");
}

#[tokio::test]
async fn test_perf_module_accessible() {
    chopin_core::perf::init_date_cache();
    let _ = chopin_core::perf::cached_date_header();
}
