use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// Helper to build the test app router (same as main.rs but without starting the server).
async fn setup() -> (axum::Router, sea_orm::DatabaseConnection) {
    use chopin_core::{config::Config, db};
    use sea_orm_migration::MigratorTrait;

    // Use in-memory SQLite for tests
    std::env::set_var("DATABASE_URL", "sqlite::memory:");
    std::env::set_var("JWT_SECRET", "test-secret-key-for-tests");

    let config = Config::from_env().unwrap();
    let database_conn = db::connect(&config).await.unwrap();

    // Run example-specific migrations
    chopin_basic_api::migrations::Migrator::up(&database_conn, None)
        .await
        .unwrap();

    let state = chopin_basic_api::AppState {
        db: database_conn.clone(),
        config,
    };

    let app = axum::Router::new()
        .merge(chopin_basic_api::controllers::posts::routes())
        .with_state(state);

    (app, database_conn)
}

#[tokio::test]
async fn test_list_posts_empty() {
    let (app, _db) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/posts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_and_get_post() {
    let (app, _db) = setup().await;

    // Create a post
    let create_body = serde_json::json!({
        "title": "Hello World",
        "body": "My first post"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/posts")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    // Fetch it back
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/posts/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_nonexistent_post() {
    let (app, _db) = setup().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/posts/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_post() {
    let (app, _db) = setup().await;

    // Create
    let create_body = serde_json::json!({ "title": "Draft", "body": "WIP" });
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/posts")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Update
    let update_body = serde_json::json!({ "title": "Published!", "published": true });
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/posts/1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&update_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_delete_post() {
    let (app, _db) = setup().await;

    // Create
    let create_body = serde_json::json!({ "title": "To Delete", "body": "Bye" });
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/posts")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Delete
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/posts/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify gone
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/posts/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
