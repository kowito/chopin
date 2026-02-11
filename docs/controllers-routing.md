# Controllers & Routing Guide

Learn how to create API endpoints and organize routes in Chopin.

## Table of Contents

- [Quick Start](#quick-start)
- [Generating Controllers](#generating-controllers)
- [Handler Functions](#handler-functions)
- [Routing](#routing)
- [Request Handling](#request-handling)
- [Response Building](#response-building)
- [OpenAPI Documentation](#openapi-documentation)
- [Best Practices](#best-practices)

## Quick Start

Generate a controller:

```bash
chopin generate model Post title:string body:text
# Creates controller automatically

# Or standalone:
chopin generate controller analytics
```

Generated controller includes CRUD endpoints:

```rust
// src/controllers/post.rs
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_by_id))
}
```

Register routes:

```rust
// In your main router
.nest("/posts", crate::controllers/post::routes())
```

## Generating Controllers

### With Model

Generates full CRUD:

```bash
chopin generate model Product name:string price:f64 stock:i32
```

Creates:
- `GET /api/products` - List all
- `POST /api/products` - Create new
- `GET /api/products/:id` - Get one

### Standalone

For custom logic:

```bash
chopin generate controller webhooks
```

Creates template with list and get-by-id handlers.

## Handler Functions

### Basic Handler

```rust
use axum::response::IntoResponse;
use chopin_core::response::ApiResponse;

async fn hello() -> impl IntoResponse {
    ApiResponse::success("Hello, World!")
}
```

### With Path Parameter

```rust
use axum::extract::Path;

async fn get_by_id(
    Path(id): Path<i32>,
) -> Result<ApiResponse<Post>, ChopinError> {
    // Use id parameter
    Ok(ApiResponse::success(post))
}
```

### With Query Parameters

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<u64>,
}

async fn search(
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    // params.q, params.limit
}
```

### With JSON Body

```rust
use chopin_core::extractors::Json;

#[derive(Deserialize)]
struct CreateRequest {
    title: String,
    body: String,
}

async fn create(
    Json(payload): Json<CreateRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    // Use payload
}
```

### With State

Access database and config:

```rust
use axum::extract::State;

async fn handler(
    State(app): State<AppState>,
) -> impl IntoResponse {
    let db = &app.db;
    let config = &app.config;
    // Use state
}
```

### With Authentication

```rust
use chopin_core::extractors::AuthUser;

async fn protected(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
) -> Result<ApiResponse<User>, ChopinError> {
    let user = User::find_by_id(user_id)
        .one(&app.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;
    
    Ok(ApiResponse::success(user))
}
```

### Multiple Extractors

Combine extractors:

```rust
async fn create_post(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // All extractors available
}
```

## Routing

### Basic Routes

```rust
use axum::{routing::{get, post, put, delete}, Router};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/", post(create))
        .route("/:id", get(get_one))
        .route("/:id", put(update))
        .route("/:id", delete(remove))
}
```

### Nested Routes

```rust
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).put(update).delete(remove))
        .route("/:id/comments", get(get_comments))
}
```

### Route Groups

Organize related routes:

```rust
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/stats", get(get_stats))
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/posts", get(list_posts))
        .route("/about", get(about))
}

// Combine
pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/admin", admin_routes())
        .nest("/public", public_routes())
}
```

### Registering Routes

In your main application setup:

```rust
// src/main.rs or src/routing.rs
use axum::Router;

pub fn app_routes() -> Router<AppState> {
    Router::new()
        .nest("/api/posts", crate::controllers::post::routes())
        .nest("/api/comments", crate::controllers::comment::routes())
        .nest("/api/admin", crate::controllers::admin::routes())
}
```

## Request Handling

### JSON Request Body

```rust
use chopin_core::extractors::Json;
use serde::Deserialize;

#[derive(Deserialize)]
struct CreatePostRequest {
    title: String,
    body: String,
    published: bool,
}

async fn create(
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    // payload is deserialized automatically
    // Validation happens during deserialization
}
```

### With Validation

```rust
use validator::Validate;

#[derive(Deserialize, Validate)]
struct CreateUserRequest {
    #[validate(email)]
    email: String,
    
    #[validate(length(min = 3, max = 50))]
    username: String,
    
    #[validate(length(min = 8))]
    password: String,
}

async fn signup(
    Json(payload): Json<CreateUserRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    // Validate
    payload.validate()
        .map_err(|e| ChopinError::Validation(format!("{}", e)))?;
    
    // Process...
}
```

### Form Data

```rust
use axum::extract::Form;

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

async fn login(
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    // Process form data
}
```

### Headers

```rust
use axum::http::HeaderMap;

async fn handler(headers: HeaderMap) -> impl IntoResponse {
    if let Some(user_agent) = headers.get("user-agent") {
        // Use header value
    }
}
```

### Query Parameters

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct Filters {
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

async fn list(
    Query(filters): Query<Filters>,
) -> impl IntoResponse {
    // Use filters
}
```

### Pagination

Use Chopin's built-in extractor:

```rust
use chopin_core::extractors::Pagination;

async fn list(
    pagination: Pagination,
) -> Result<ApiResponse<Vec<Post>>, ChopinError> {
    let p = pagination.clamped(); // Max 100
    
    let posts = Post::find()
        .limit(p.limit)
        .offset(p.offset)
        .all(&db)
        .await?;
    
    Ok(ApiResponse::success(posts))
}
```

## Response Building

### Success Response

```rust
use chopin_core::response::ApiResponse;

async fn handler() -> impl IntoResponse {
    ApiResponse::success(data)
}

// Produces:
// {
//   "success": true,
//   "data": { ... }
// }
```

### Error Response

```rust
use chopin_core::error::ChopinError;

async fn handler() -> Result<ApiResponse<Data>, ChopinError> {
    if error_condition {
        return Err(ChopinError::NotFound("Resource not found".to_string()));
    }
    
    Ok(ApiResponse::success(data))
}

// Error produces:
// {
//   "success": false,
//   "error": {
//     "code": "NOT_FOUND",
//     "message": "Resource not found"
//   }
// }
```

### Custom Status Codes

```rust
use axum::http::StatusCode;
use axum::response::IntoResponse;

async fn handler() -> impl IntoResponse {
    (StatusCode::CREATED, ApiResponse::success(data))
}
```

### Response Headers

```rust
use axum::response::{IntoResponse, Response};
use axum::http::header;

async fn handler() -> Response {
    let mut response = ApiResponse::success(data).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "max-age=3600".parse().unwrap(),
    );
    response
}
```

### Redirects

```rust
use axum::response::Redirect;

async fn old_endpoint() -> impl IntoResponse {
    Redirect::permanent("/new-endpoint")
}
```

## OpenAPI Documentation

### Annotating Handlers

All generated handlers include OpenAPI annotations:

```rust
/// List all posts.
#[utoipa::path(
    get,
    path = "/api/posts",
    responses(
        (status = 200, description = "List of posts", body = ApiResponse<Vec<PostResponse>>),
    ),
    tag = "posts"
)]
async fn list(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<PostResponse>>, ChopinError> {
    // Handler implementation
}
```

### With Parameters

```rust
/// Get a post by ID.
#[utoipa::path(
    get,
    path = "/api/posts/{id}",
    params(
        ("id" = i32, Path, description = "Post ID")
    ),
    responses(
        (status = 200, description = "Post found", body = ApiResponse<PostResponse>),
        (status = 404, description = "Post not found")
    ),
    tag = "posts"
)]
async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // Handler implementation
}
```

### With Request Body

```rust
/// Create a new post.
#[utoipa::path(
    post,
    path = "/api/posts",
    request_body = CreatePostRequest,
    responses(
        (status = 201, description = "Post created", body = ApiResponse<PostResponse>),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "posts"
)]
async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // Handler implementation
}
```

### Schema Definitions

Derive `ToSchema` on request/response types:

```rust
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreatePostRequest {
    /// Post title
    pub title: String,
    /// Post body content
    pub body: String,
    /// Whether the post is published
    pub published: bool,
}

#[derive(Serialize, ToSchema)]
pub struct PostResponse {
    pub id: i32,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub created_at: chrono::NaiveDateTime,
}
```

## Best Practices

### ✅ DO

**1. Use Typed Extractors**
```rust
// Good
async fn handler(Json(payload): Json<Request>) -> Result<ApiResponse<Response>, ChopinError>

// Avoid
async fn handler(body: String) -> String
```

**2. Return Results**
```rust
// Good
async fn handler() -> Result<ApiResponse<Data>, ChopinError>

// Avoid direct panic
async fn handler() -> ApiResponse<Data> {
    data.unwrap() // DON'T
}
```

**3. Use OpenAPI Annotations**
```rust
#[utoipa::path(
    get,
    path = "/api/resource",
    responses(...),
    tag = "resource"
)]
async fn handler() {}
```

**4. Organize by Resource**
```
src/controllers/
├── mod.rs
├── auth.rs     # /api/auth/*
├── post.rs     # /api/posts/*
├── user.rs     # /api/users/*
└── comment.rs  # /api/comments/*
```

**5. Use Dedicated Response Types**
```rust
// Don't expose internal models directly
pub struct UserResponse {
    pub id: i32,
    pub email: String,
    // No password_hash field!
}

impl From<user::Model> for UserResponse {
    fn from(user: user::Model) -> Self {
        UserResponse {
            id: user.id,
            email: user.email,
        }
    }
}
```

### ❌ DON'T

**1. Expose Database Errors**
```rust
// Bad
User::find_by_id(id).one(&db).await // DbErr leaks to client

// Good
User::find_by_id(id)
    .one(&db)
    .await
    .map_err(|e| ChopinError::Database(e))?
```

**2. Forget Authentication**
```rust
// Bad - unprotected endpoint
async fn delete_user(Path(id): Path<i32>) {}

// Good - require auth
async fn delete_user(
    AuthUser(user_id): AuthUser,
    Path(id): Path<i32>,
) -> Result<ApiResponse<()>, ChopinError>
```

**3. Ignore Validation**
```rust
// Bad
async fn create(Json(payload): Json<Request>) {
    // No validation!
}

// Good
async fn create(Json(payload): Json<Request>) -> Result<...> {
    payload.validate()?;
}
```

## Common Patterns

### CRUD Operations

```rust
// List all
async fn list(
    State(state): State<AppState>,
    pagination: Pagination,
) -> Result<ApiResponse<Vec<Response>>, ChopinError> {
    let p = pagination.clamped();
    let items = Entity::find()
        .limit(p.limit)
        .offset(p.offset)
        .all(&state.db)
        .await?;
    Ok(ApiResponse::success(items))
}

// Get one
async fn get(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<Response>, ChopinError> {
    let item = Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Item not found".to_string()))?;
    Ok(ApiResponse::success(item))
}

// Create
async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateRequest>,
) -> Result<ApiResponse<Response>, ChopinError> {
    let new_item = ActiveModel {
        // Set fields...
        ..Default::default()
    };
    let item = new_item.insert(&state.db).await?;
    Ok(ApiResponse::success(item))
}

// Update
async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateRequest>,
) -> Result<ApiResponse<Response>, ChopinError> {
    let mut item: ActiveModel = Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Item not found".to_string()))?
        .into();
    
    // Update fields
    item.updated_at = Set(Utc::now().naive_utc());
    let updated = item.update(&state.db).await?;
    Ok(ApiResponse::success(updated))
}

// Delete
async fn delete(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<()>, ChopinError> {
    let item = Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Item not found".to_string()))?;
    
    item.delete(&state.db).await?;
    Ok(ApiResponse::success(()))
}
```

### Search & Filter

```rust
#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
    status: Option<String>,
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<ApiResponse<Vec<Response>>, ChopinError> {
    let mut query = Post::find();
    
    if let Some(search_term) = params.q {
        query = query.filter(
            Condition::any()
                .add(Column::Title.contains(&search_term))
                .add(Column::Body.contains(&search_term))
        );
    }
    
    if let Some(status) = params.status {
        query = query.filter(Column::Status.eq(status));
    }
    
    let results = query.all(&state.db).await?;
    Ok(ApiResponse::success(results))
}
```

---

## Resources

- [Axum Documentation](https://docs.rs/axum/)
- [Tower Middleware](https://docs.rs/tower/)
- [utoipa Documentation](https://docs.rs/utoipa/)
- [API Reference](api.md)

Build powerful, documented APIs with Chopin's controller system!
