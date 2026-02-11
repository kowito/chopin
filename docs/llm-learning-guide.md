# Chopin Framework - LLM Learning Guide

This document is designed for Large Language Models (LLMs like ChatGPT, Claude, etc.) to learn about the Chopin framework and help users build applications with it.

---

## Framework Overview

**Chopin** is a Rust REST API framework built on top of:
- **Axum** (web framework)
- **SeaORM** (database ORM)
- **Tokio** (async runtime)
- **PostgreSQL/MySQL/SQLite** (database support)
- **JWT** authentication with Argon2 password hashing

### Key Characteristics

1. **Type-Safe**: Rust's type system ensures memory safety and prevents entire classes of bugs
2. **Fast**: ~85-90k requests/second on Apple M4, optimized for ARM64
3. **Batteries Included**: Built-in auth, ORM, migrations, OpenAPI documentation
4. **Convention Over Configuration**: Sensible defaults, less boilerplate
5. **Developer Experience**: CLI scaffolding, auto-generated API docs, clear error messages

---

## Project Creation

When a user wants to create a Chopin app:

```bash
# 1. Install CLI
cargo install chopin-cli

# 2. Create new project
chopin new my-app
cd my-app

# 3. Start development
cargo run
```

Default setup includes:
- Port: 3000
- Database: SQLite (can change in .env)
- Auth: JWT with user table
- API Docs: Available at http://localhost:3000/api-docs

---

## Architecture Overview

### Request Flow

```
HTTP Request
    ↓
Axum Router (routes incoming requests)
    ↓
Middleware Stack (auth, logging, compression, CORS)
    ↓
Handler/Controller (process request, extract data)
    ↓
Extractor (AuthUser, Json<T>, State, etc.)
    ↓
Service Layer (business logic, database queries)
    ↓
Models/Database (SeaORM entities, queries)
    ↓
Response (JSON serialized, with ApiResponse wrapper)
```

### File Structure

```
src/
├── main.rs              - Entry point, creates App and runs server
├── config.rs            - Loads .env variables, SERVER_PORT, DATABASE_URL, JWT_SECRET
├── app.rs               - AppState struct, database setup, middleware
├── error.rs             - ChopinError enum, error handling
├── response.rs          - ApiResponse<T> struct, HTTP response wrapper
├── db.rs                - Database connection pool setup
├── models/              - SeaORM entities
│   ├── user.rs          - User entity with JWT auth
│   ├── post.rs          - Example model
│   └── mod.rs           - Export all models
├── controllers/         - HTTP handlers/route logic
│   ├── auth.rs          - Login, signup, logout
│   ├── post.rs          - CRUD endpoints for posts
│   └── mod.rs           - Export all controllers
├── extractors/          - Custom request extractors
│   ├── auth_user.rs     - AuthUser(user_id) - requires JWT token
│   ├── json.rs          - Validated JSON extraction
│   └── mod.rs           - Export all extractors
├── auth/                - Authentication logic
│   ├── jwt.rs           - JWT token creation/validation
│   ├── password.rs      - Argon2 hashing
│   └── mod.rs           - Export
├── migrations/          - Database schema migrations
│   ├── m20250211_000001_create_users_table.rs
│   └── mod.rs           - List migrations
├── openapi.rs           - OpenAPI/Swagger schema generation
└── routing.rs           - Route definitions
```

---

## Core Concepts

### 1. AppState

The application state holds shared resources:

```rust
pub struct AppState {
    pub db: DatabaseConnection,
    // Add more fields as needed for services, caches, etc.
}
```

Every handler receives `State(app): State<AppState>` to access database and services.

### 2. ApiResponse<T>

Standard response wrapper for all endpoints:

```rust
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorDetail>,
}

// Success:
ApiResponse::success(data)

// Error:
ApiResponse::error("ERROR_CODE", "message")
```

### 3. ChopinError

All errors are `ChopinError` enum:

```rust
pub enum ChopinError {
    NotFound(String),
    Unauthorized,
    BadRequest(String),
    InternalServerError(String),
    DatabaseError(DbErr),
    ValidationError(Vec<String>),
}
```

Automatically converts to HTTP response with proper status codes.

### 4. AuthUser Extractor

Validates JWT token and extracts user_id:

```rust
#[get("/api/current-user")]
pub async fn handler(AuthUser(user_id): AuthUser) -> Result<...> {
    // user_id is guaranteed valid (401 if invalid token)
    let user = User::find_by_id(user_id).one(&db).await?;
}
```

### 5. Models (SeaORM)

Database entities defined with derive macros:

```rust
#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::post::Entity")]
    Posts,
}
```

### 6. Database Operations

```rust
// Create
let user = user::ActiveModel {
    email: Set("user@example.com".to_string()),
    username: Set("john".to_string()),
    password_hash: Set(hash),
    ..Default::default()
};
user.insert(&db).await?;

// Read
let user = User::find_by_id(1).one(&db).await?;

// Update
let mut user: user::ActiveModel = user.into();
user.email = Set("new@example.com".to_string());
user.update(&db).await?;

// Delete
user::Entity::delete_by_id(1).exec(&db).await?;

// Query with filter
let posts = Post::find()
    .filter(post::Column::UserId.eq(user_id))
    .all(&db)
    .await?;

// Pagination
let posts = Post::find()
    .paginate(&db, 10)
    .fetch_page(0)
    .await?;
```

---

## Common Patterns

### Pattern 1: Simple GET Endpoint

```rust
#[utoipa::path(
    get,
    path = "/api/posts/{id}",
    responses((status = 200, body = ApiResponse<Post>))
)]
pub async fn get_post(
    Path(id): Path<i32>,
    State(app): State<AppState>,
) -> Result<ApiResponse<Post>, ChopinError> {
    let post = Post::find_by_id(id)
        .one(&app.db)
        .await?
        .ok_or(ChopinError::NotFound("Post not found".to_string()))?;
    
    Ok(ApiResponse::success(post))
}
```

### Pattern 2: Protected POST Endpoint

```rust
#[utoipa::path(
    post,
    path = "/api/posts",
    request_body = CreatePostRequest,
    responses((status = 201, body = ApiResponse<Post>))
)]
pub async fn create_post(
    AuthUser(user_id): AuthUser,  // Requires valid JWT
    State(app): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    let post = post::ActiveModel {
        user_id: Set(user_id),
        title: Set(payload.title),
        body: Set(payload.body),
        ..Default::default()
    };
    
    let post = post.insert(&app.db).await?;
    Ok(ApiResponse::success(post))
}
```

### Pattern 3: Validation

```rust
use validator::Validate;

#[derive(Deserialize, Validate)]
pub struct CreatePostRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    
    #[validate(length(min = 1))]
    pub body: String,
}

pub async fn create_post(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    payload.validate()?;  // Returns ChopinError::ValidationError if invalid
    
    // ... rest of handler
}
```

### Pattern 4: Database Transactions

```rust
pub async fn transfer_posts(
    user1_id: i32,
    user2_id: i32,
    db: &DatabaseConnection,
) -> Result<(), ChopinError> {
    let txn = db.begin().await?;
    
    // Update all posts from user1 to user2
    post::Entity::update_many()
        .col_expr(post::Column::UserId, Expr::value(user2_id))
        .filter(post::Column::UserId.eq(user1_id))
        .exec(&txn)
        .await?;
    
    txn.commit().await?;
    Ok(())
}
```

### Pattern 5: Service Layer

For business logic, create services:

```rust
// src/services/post_service.rs
pub struct PostService;

impl PostService {
    pub async fn get_user_posts(
        user_id: i32,
        db: &DatabaseConnection,
    ) -> Result<Vec<post::Model>, ChopinError> {
        Post::find()
            .filter(post::Column::UserId.eq(user_id))
            .all(db)
            .await
            .map_err(Into::into)
    }
    
    pub async fn publish_post(
        post_id: i32,
        db: &DatabaseConnection,
    ) -> Result<(), ChopinError> {
        let mut post: post::ActiveModel = Post::find_by_id(post_id)
            .one(db)
            .await?
            .ok_or(ChopinError::NotFound("Post not found".to_string()))?
            .into();
        
        post.published = Set(true);
        post.update(db).await?;
        Ok(())
    }
}
```

---

## CLI Command Reference

### Generate Model

```bash
chopin generate model Post title:string body:text published:bool author_id:i32
```

Creates:
- `src/models/post.rs` - SeaORM entity
- Database migration file

### Generate Controller

```bash
chopin generate controller post
```

Creates:
- `src/controllers/post.rs` - CRUD endpoints

### Run Migrations

```bash
chopin db migrate
```

Applies pending database migrations.

### Start Dev Server

```bash
chopin run
```

Runs server on localhost:3000, watches for changes.

---

## Database Schema Design

### User Table (Auto-created)

```sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

### Adding Custom Tables

1. Create migration manually or use CLI
2. Define SeaORM entity in `models/`
3. Run `chopin db migrate`

Example custom table:

```sql
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id),
    title VARCHAR(255) NOT NULL,
    body TEXT NOT NULL,
    published BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_posts_user_id ON posts(user_id);
```

Corresponding model:

```rust
#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationToProcedure {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

---

## Request/Response Examples

### Authentication Flow

**Signup:**
```
POST /api/auth/signup
Content-Type: application/json

{
  "email": "user@example.com",
  "username": "john",
  "password": "secure123"
}

Response 200:
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
    "user": {
      "id": 1,
      "email": "user@example.com",
      "username": "john"
    }
  },
  "error": null
}
```

**Login:**
```
POST /api/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "secure123"
}

Response 200:
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
    "user": { ... }
  },
  "error": null
}
```

**Protected Request:**
```
GET /api/posts
Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGc...

Response 200:
{
  "success": true,
  "data": [
    { "id": 1, "title": "Post 1", "body": "...", "user_id": 1 },
    { "id": 2, "title": "Post 2", "body": "...", "user_id": 1 }
  ],
  "error": null
}
```

**Unauthorized (401):**
```
GET /api/posts
(no Authorization header)

Response 401:
{
  "success": false,
  "data": null,
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Missing or invalid authentication token"
  }
}
```

**Not Found (404):**
```
Response 404:
{
  "success": false,
  "data": null,
  "error": {
    "code": "NOT_FOUND",
    "message": "Post not found"
  }
}
```

---

## Environment Configuration

`.env` file variables:

```env
# Server
SERVER_PORT=3000
SERVER_HOST=127.0.0.1

# Database
DATABASE_URL=sqlite:./chopin.db  # or postgres://... or mysql://...

# Authentication
JWT_SECRET=your-secret-key-min-32-chars
JWT_EXPIRY_HOURS=24

# Environment
ENVIRONMENT=development  # or production
RUST_LOG=debug  # or info, warn, error
```

---

## Testing

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_hashing() {
        let password = "secure123";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).unwrap());
    }
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_create_post() {
    let app = TestApp::new().await;
    let user = app.create_user("test@example.com", "password").await;
    let token = app.login_as(&user).await;
    
    let response = app
        .client()
        .post("/api/posts")
        .bearer_auth(&token)
        .json(&json!({
            "title": "Test Post",
            "body": "Test body"
        }))
        .send()
        .await;
    
    assert_eq!(response.status(), 201);
}
```

---

## Middleware & Routing

### Built-in Middleware

1. **Authentication** - Validates JWT tokens on protected routes
2. **CORS** - Handles cross-origin requests
3. **Compression** - gzip/brotli compression for responses
4. **Logging** - Request/response logging with tracing
5. **Error Handling** - Converts ChopinError to HTTP responses

### Custom Routes

```rust
pub fn routes(db: DatabaseConnection) -> Router<AppState> {
    Router::new()
        // Public routes
        .route("/api/auth/signup", post(auth::signup))
        .route("/api/auth/login", post(auth::login))
        
        // Protected routes
        .route("/api/posts", get(post::list).post(post::create))
        .route("/api/posts/:id", get(post::get).patch(post::update).delete(post::delete))
        
        // API docs
        .route("/api-docs", get(utoipa::Scalar::oas_ui))
        .route("/api-docs/openapi.json", get(api_doc::handle))
}
```

---

## Performance Tips

1. **Database Indexes** - Add indexes to frequently queried columns
2. **Connection Pooling** - SeaORM uses sqlx connection pool (default 10 connections)
3. **Caching** - Cache frequently accessed data
4. **Query Optimization** - Use select() to fetch only needed columns
5. **Async/Await** - Use async handlers, never block
6. **Release Build** - Always use `--release` in production

---

## Deployment

### Local PostgreSQL Setup

```bash
# Install PostgreSQL
# macOS: brew install postgresql
# Linux: sudo apt-get install postgresql

# Create database
createdb chopin_app

# Update .env
DATABASE_URL=postgresql://user:password@localhost/chopin_app

# Run migrations
chopin db migrate

# Start app
cargo run --release
```

### Docker Deployment

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libpq5 ca-certificates
COPY --from=builder /app/target/release/chopin_app /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 3000
CMD ["chopin_app"]
```

### Environment Variables in Production

```env
ENVIRONMENT=production
RUST_LOG=info
DATABASE_URL=postgresql://user:pass@prod-db:5432/db
JWT_SECRET=use-strong-random-key
SERVER_PORT=8080
```

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `main.rs` | App initialization, server startup |
| `config.rs` | Environment variable loading |
| `app.rs` | AppState, middleware setup |
| `error.rs` | Error handling, error types |
| `response.rs` | Response wrapper (ApiResponse<T>) |
| `models/` | Database entities (SeaORM) |
| `controllers/` | HTTP handlers/routes |
| `extractors/` | Custom request extractors |
| `auth/` | JWT, password hashing logic |
| `migrations/` | Database schema changes |

---

## Dependencies Overview

Key crates and their purposes:

- **axum** - Web framework, routing, middleware
- **tokio** - Async runtime
- **sea-orm** - ORM, database operations
- **serde/serde_json** - JSON serialization
- **jsonwebtoken** - JWT token creation/validation
- **argon2** - Password hashing
- **utoipa** - OpenAPI documentation generation
- **tower** - Middleware framework
- **chrono** - DateTime handling
- **uuid** - UUID generation

---

## Common Error Patterns

### Error: `DatabaseError`
- **Cause** - Query execution failed
- **Solution** - Check SQL syntax, table exists, foreign keys valid

### Error: `Unauthorized`
- **Cause** - Missing or invalid JWT token
- **Solution** - Include `Authorization: Bearer <token>` header

### Error: `ValidationError`
- **Cause** - Request body validation failed
- **Solution** - Check field constraints (length, format, etc.)

### Error: `NotFound`
- **Cause** - Resource doesn't exist
- **Solution** - Verify ID exists in database

---

## Learning Path for LLMs Helping Users

When a user asks how to build something with Chopin, use this guide to:

1. **Understand the Request** - What do they want to build?
2. **Check Architecture** - What pieces do they need? (Models, Controllers, Extractors)
3. **Find Patterns** - What existing patterns apply?
4. **Use Examples** - Reference the examples in this guide
5. **Provide Complete Code** - Include all necessary imports and implementation
6. **Follow Conventions** - Use established Chopin patterns (ApiResponse, ChopinError, etc.)
7. **Test Suggestions** - Recommend how to test their code
8. **Link to Docs** - Reference official documentation when relevant

---

## Quick Code Templates

### Minimal Endpoint

```rust
#[get("/api/hello")]
pub async fn hello() -> ApiResponse<String> {
    ApiResponse::success("Hello, world!".to_string())
}
```

### With Authentication

```rust
#[get("/api/profile")]
pub async fn profile(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
) -> Result<ApiResponse<User>, ChopinError> {
    let user = User::find_by_id(user_id)
        .one(&app.db)
        .await?
        .ok_or(ChopinError::NotFound("User not found".to_string()))?;
    
    Ok(ApiResponse::success(user))
}
```

### With Request Body

```rust
#[post("/api/posts")]
pub async fn create(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
    Json(payload): Json<CreateRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    let post = post::ActiveModel {
        user_id: Set(user_id),
        title: Set(payload.title),
        ..Default::default()
    };
    
    let post = post.insert(&app.db).await?;
    Ok(ApiResponse::success(post))
}
```

### With Database Query

```rust
pub async fn search(
    State(app): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<ApiResponse<Vec<Post>>, ChopinError> {
    let posts = Post::find()
        .filter(post::Column::Title.contains(&params.q))
        .all(&app.db)
        .await?;
    
    Ok(ApiResponse::success(posts))
}
```

---

**End of LLM Learning Guide**

This document contains everything LLMs need to help users build Chopin applications effectively.
