use chopin_core::TestApp;

#[tokio::test]
async fn test_list_posts_empty() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api/posts")).await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());

    let data = res.data();
    assert!(data.is_array());
    assert_eq!(data.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_post_requires_auth() {
    let app = TestApp::new().await;

    let body = serde_json::json!({
        "title": "Test Post",
        "body": "Test body"
    });

    let res = app
        .client
        .post(&app.url("/api/posts"), &body.to_string())
        .await;

    // Should fail without authentication
    assert_eq!(res.status, 401);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_create_post_with_auth() {
    let app = TestApp::new().await;

    // Create and auth a user
    let (token, _user) = app
        .create_user("author@example.com", "author", "password123")
        .await;

    let body = serde_json::json!({
        "title": "My First Post",
        "body": "This is the content of my post."
    });

    let res = app
        .client
        .post_with_auth(&app.url("/api/posts"), &token, &body.to_string())
        .await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());

    let data = res.data();
    assert_eq!(data["title"], "My First Post");
    assert_eq!(data["body"], "This is the content of my post.");
    assert!(data["id"].is_number());
}

#[tokio::test]
async fn test_create_post_validation() {
    let app = TestApp::new().await;

    let (token, _user) = app
        .create_user("val@example.com", "val", "password123")
        .await;

    // Empty title should fail
    let body = serde_json::json!({
        "title": "",
        "body": "Some content"
    });

    let res = app
        .client
        .post_with_auth(&app.url("/api/posts"), &token, &body.to_string())
        .await;

    assert_eq!(res.status, 422);
    assert!(!res.is_success());
}

#[tokio::test]
async fn test_list_posts_with_pagination() {
    let app = TestApp::new().await;

    let (token, _user) = app
        .create_user("paginate@example.com", "paginate", "password123")
        .await;

    // Create several posts
    for i in 1..=5 {
        let body = serde_json::json!({
            "title": format!("Post {}", i),
            "body": format!("Content {}", i)
        });

        app.client
            .post_with_auth(&app.url("/api/posts"), &token, &body.to_string())
            .await;
    }

    // List with limit
    let res = app.client.get(&app.url("/api/posts?limit=3")).await;

    assert_eq!(res.status, 200);
    let data = res.data();
    assert!(data.is_array());
    assert_eq!(data.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_get_post_by_id() {
    let app = TestApp::new().await;

    let (token, _user) = app
        .create_user("getpost@example.com", "getpost", "password123")
        .await;

    // Create a post
    let body = serde_json::json!({
        "title": "Specific Post",
        "body": "Specific content"
    });

    let create_res = app
        .client
        .post_with_auth(&app.url("/api/posts"), &token, &body.to_string())
        .await;

    let post_id = create_res.data()["id"].as_i64().unwrap();

    // Get the post by ID
    let res = app
        .client
        .get(&app.url(&format!("/api/posts/{}", post_id)))
        .await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());
    assert_eq!(res.data()["title"], "Specific Post");
}

#[tokio::test]
async fn test_get_nonexistent_post() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api/posts/99999")).await;

    assert_eq!(res.status, 404);
    assert!(!res.is_success());
}
