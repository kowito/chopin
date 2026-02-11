# Chopin Basic API Example

A complete example application built with the Chopin web framework, demonstrating:

- **User authentication** (JWT-based signup/login)
- **Posts CRUD** (create, read, list with pagination)
- **Protected endpoints** (using AuthUser extractor)
- **Database migrations** (SeaORM)
- **Auto-generated API documentation** (OpenAPI/Swagger)
- **Custom error handling**
- **Integration tests**

## Quick Start

```bash
# Install dependencies
cargo build

# Run the server
cargo run
```

The server starts at `http://127.0.0.1:3000`.

View API documentation at `http://127.0.0.1:3000/api-docs`.

## API Endpoints

### Authentication (from Chopin core)

- `POST /api/auth/signup` — Create a new user account
- `POST /api/auth/login` — Login with credentials

### Posts (example endpoints)

- `GET /api/posts` — List all posts (with pagination)
- `POST /api/posts` — Create a new post (requires authentication)
- `GET /api/posts/{id}` — Get a single post

## Usage Examples

### 1. Sign up a user

```bash
curl -X POST http://localhost:3000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "alice@example.com",
    "username": "alice",
    "password": "secret123"
  }'
```

Response:
```json
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
    "user": {
      "id": 1,
      "email": "alice@example.com",
      "username": "alice",
      "is_active": true,
      "created_at": "2026-02-11T10:30:00"
    }
  }
}
```

### 2. Create a post (authenticated)

```bash
curl -X POST http://localhost:3000/api/posts \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN_HERE" \
  -d '{
    "title": "Hello Chopin!",
    "body": "This is my first post using the Chopin framework."
  }'
```

### 3. List posts (with pagination)

```bash
curl http://localhost:3000/api/posts?limit=10&offset=0
```

### 4. Get a specific post

```bash
curl http://localhost:3000/api/posts/1
```

## Project Structure

```
basic-api/
├── src/
│   ├── main.rs              # Entry point, router setup
│   ├── models/
│   │   ├── mod.rs
│   │   └── post.rs          # Post entity (SeaORM)
│   ├── controllers/
│   │   ├── mod.rs
│   │   └── posts.rs         # Post CRUD handlers
│   └── migrations/
│       ├── mod.rs
│       └── m*_create_posts_table.rs
├── tests/
│   └── integration_tests.rs # Integration tests
├── Cargo.toml
├── .env                     # Environment configuration
└── README.md
```

## Run Tests

```bash
cargo test
```

Tests use an in-memory SQLite database.

## Key Features Demonstrated

### 1. Authentication with AuthUser Extractor

```rust
async fn create_post(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,  // Automatically validates JWT
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // user_id is extracted from the JWT token
}
```

### 2. Custom Error Handling

```rust
if payload.title.is_empty() {
    return Err(ChopinError::Validation("Title is required".to_string()));
}
```

All errors are automatically converted to JSON responses.

### 3. Pagination Support

```rust
async fn list_posts(
    State(state): State<AppState>,
    pagination: Pagination,  // Extracts ?limit=X&offset=Y
) -> Result<ApiResponse<Vec<PostResponse>>, ChopinError> {
    let p = pagination.clamped();  // Max 100
    // Use p.limit and p.offset in queries
}
```

### 4. OpenAPI Documentation

All endpoints are annotated with `#[utoipa::path]` macros and automatically appear in the Swagger UI at `/api-docs`.

## Configuration

Edit `.env` to configure:

- Database URL (SQLite, PostgreSQL, or MySQL)
- JWT secret
- Server host and port
- Environment (development, production, test)

## Learn More

- [Chopin Documentation](../../docs/getting-started.md)
- [API Reference](../../docs/api.md)
- [Framework Source](../../chopin-core/)
