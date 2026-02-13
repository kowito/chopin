# Controllers & Routing

**Last Updated:** February 2026

## Overview

Controllers in Chopin are Axum handler functions organized into modules. Routes are defined using Axum's `Router` and composed together.

## Defining a Controller

```rust
use axum::{Router, routing::{get, post}, extract::State};
use chopin_core::{ApiResponse, controllers::AppState};
use serde::{Deserialize, Serialize};

// Response type
#[derive(Serialize)]
pub struct PostResponse {
    pub id: i32,
    pub title: String,
}

// Request type
#[derive(Deserialize)]
pub struct CreatePostRequest {
    pub title: String,
    pub body: String,
}

// Define routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts", get(list_posts).post(create_post))
        .route("/api/posts/:id", get(get_post))
}

// Handlers
async fn list_posts(State(state): State<AppState>) -> ApiResponse<Vec<PostResponse>> {
    // Query database...
    ApiResponse::success(vec![])
}

async fn create_post(
    State(state): State<AppState>,
    chopin_core::extractors::Json(body): chopin_core::extractors::Json<CreatePostRequest>,
) -> ApiResponse<PostResponse> {
    // Insert into database...
    ApiResponse::created(PostResponse { id: 1, title: body.title })
}

async fn get_post(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<ApiResponse<PostResponse>, chopin_core::ChopinError> {
    // Find by ID or return 404
    Err(chopin_core::ChopinError::NotFound("Post not found".into()))
}
```

## Registering Routes

In your `main.rs`, merge your routes with the Chopin app:

```rust
// The Chopin app automatically includes:
// - / (welcome)
// - /api/auth/* (signup, login)
// - /api-docs (Scalar UI)
// - /api-docs/openapi.json

// To add your own routes, modify the router in your app setup
let state = AppState { db, config, cache };

let app = Router::new()
    .merge(controllers::posts::routes())
    .merge(controllers::comments::routes())
    .with_state(state);
```

## AppState

All handlers receive shared state:

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,   // SeaORM connection pool
    pub config: Arc<Config>,      // Application configuration
    pub cache: CacheService,      // Cache backend
}
```

## Extractors

Chopin provides custom extractors:

### Json Extractor

Uses `sonic-rs` for ARM NEON optimized deserialization:

```rust
use chopin_core::extractors::Json;

async fn create(Json(body): Json<CreateRequest>) -> ApiResponse<Item> {
    // body is deserialized with sonic-rs
}
```

### AuthUser Extractor

Extracts the authenticated user from the JWT `Authorization` header:

```rust
use chopin_core::extractors::AuthUser;

async fn me(user: AuthUser) -> ApiResponse<UserInfo> {
    // user.user_id is available
    // user.role is the user's Role enum
}
```

### Pagination Extractor

Parses `?page=1&per_page=20` query parameters:

```rust
use chopin_core::extractors::{Pagination, PaginatedResponse};

async fn list(
    State(state): State<AppState>,
    pagination: Pagination,
) -> ApiResponse<PaginatedResponse<Vec<Item>>> {
    let offset = pagination.offset();
    let limit = pagination.limit();
    // Query with LIMIT/OFFSET...
    let response = pagination.response(items, total_count);
    ApiResponse::success(response)
}
```

### Role-Based Access

Restrict endpoints to specific roles:

```rust
use chopin_core::extractors::AuthUserWithRole;
use chopin_core::models::user::Role;

async fn admin_only(user: AuthUserWithRole<{ Role::Admin as u8 }>) -> ApiResponse<String> {
    ApiResponse::success("Admin access granted".into())
}
```

Or use the middleware:

```rust
use chopin_core::extractors::require_role;
use chopin_core::models::user::Role;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/users", get(list_users))
        .layer(axum::middleware::from_fn(require_role(Role::Admin)))
}
```

## Response Types

### Success Responses

```rust
ApiResponse::success(data)           // 200 OK
ApiResponse::created(data)           // 201 Created
ApiResponse::success_message("Done") // 200 with message only
```

### Error Responses

```rust
use chopin_core::ChopinError;

ChopinError::NotFound("User not found".into())
ChopinError::Unauthorized("Invalid token".into())
ChopinError::Forbidden("Admin only".into())
ChopinError::BadRequest("Invalid input".into())
ChopinError::Validation(validator_errors)
ChopinError::Internal("Something broke".into())
```

All responses use a consistent JSON format:

```json
{
  "success": true,
  "data": { ... },
  "message": null
}
```

```json
{
  "success": false,
  "error": "Not found",
  "message": "User not found"
}
```

## OpenAPI Documentation

Add `#[utoipa::path]` attributes to auto-document your endpoints:

```rust
#[utoipa::path(
    get,
    path = "/api/posts",
    tag = "posts",
    responses(
        (status = 200, description = "List of posts", body = ApiResponse<Vec<PostResponse>>)
    )
)]
async fn list_posts(...) -> ApiResponse<Vec<PostResponse>> { ... }
```

Register schemas in your OpenAPI doc:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(list_posts, create_post),
    components(schemas(PostResponse, CreatePostRequest))
)]
pub struct ApiDoc;
```

## Built-in Auth Routes

Chopin provides these auth endpoints automatically:

### POST `/api/auth/signup`

```json
{
  "email": "user@example.com",
  "username": "alice",
  "password": "secret123"
}
```

Returns:

```json
{
  "success": true,
  "data": {
    "access_token": "eyJ...",
    "user": { "id": 1, "email": "user@example.com", "username": "alice", "role": "user" }
  }
}
```

### POST `/api/auth/login`

```json
{
  "email": "user@example.com",
  "password": "secret123"
}
```

Returns the same format as signup.
