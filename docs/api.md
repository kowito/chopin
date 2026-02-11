# Chopin API Reference

## Request/Response Format

### Standard Response

All Chopin endpoints return a standardized JSON format:

**Success:**
```json
{
  "success": true,
  "data": { /* response payload */ }
}
```

**Error:**
```json
{
  "success": false,
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Email is required"
  }
}
```

The `data` field is omitted on errors; the `error` field is omitted on success.

---

## Authentication

Chopin uses **JWT (JSON Web Tokens)** for stateless authentication.

### Flow

1. Client signs up or logs in → receives a JWT `access_token`
2. Client sends the token in the `Authorization: Bearer <token>` header
3. Server validates the token via the `AuthUser` extractor
4. Token contains the user's ID and expiration time

### Endpoints

#### `POST /api/auth/signup`

Create a new user account.

**Request:**
```json
{
  "email": "user@example.com",
  "username": "john",
  "password": "secret123"
}
```

**Validation:**
- `email`, `username`, `password` are all required (non-empty)
- `password` must be at least 8 characters
- `email` and `username` must be unique

**Response (200):**
```json
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "user": {
      "id": 1,
      "email": "user@example.com",
      "username": "john",
      "is_active": true,
      "created_at": "2026-02-11T10:30:00"
    }
  }
}
```

**Errors:**
- `409 CONFLICT` — User with email or username already exists
- `422 VALIDATION_ERROR` — Missing or invalid fields

#### `POST /api/auth/login`

Authenticate with existing credentials.

**Request:**
```json
{
  "email": "user@example.com",
  "password": "secret123"
}
```

**Response (200):**
```json
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "user": {
      "id": 1,
      "email": "user@example.com",
      "username": "john",
      "is_active": true,
      "created_at": "2026-02-11T10:30:00"
    }
  }
}
```

**Errors:**
- `401 UNAUTHORIZED` — Invalid email/password or account deactivated

---

## Error Codes

| HTTP Status | Error Code         | Description                          |
|-------------|-------------------|--------------------------------------|
| 400         | `BAD_REQUEST`      | Malformed request                    |
| 401         | `UNAUTHORIZED`     | Missing or invalid authentication    |
| 403         | `FORBIDDEN`        | Authenticated but not permitted      |
| 404         | `NOT_FOUND`        | Resource not found                   |
| 409         | `CONFLICT`         | Resource already exists              |
| 422         | `VALIDATION_ERROR` | Input validation failed              |
| 500         | `INTERNAL_ERROR`   | Unexpected server error              |
| 500         | `DATABASE_ERROR`   | Database operation failed            |

All errors follow this format:

```json
{
  "success": false,
  "error": {
    "code": "NOT_FOUND",
    "message": "Post with id 42 not found"
  }
}
```

---

## Pagination

List endpoints support pagination via query parameters:

| Parameter | Type | Default | Max | Description            |
|-----------|------|---------|-----|------------------------|
| `limit`   | u64  | 20      | 100 | Number of items        |
| `offset`  | u64  | 0       | —   | Number of items to skip|

**Example:**
```
GET /api/posts?limit=10&offset=20
```

In handlers, use the `Pagination` extractor:

```rust
use chopin_core::extractors::Pagination;

async fn list_posts(pagination: Pagination) -> impl IntoResponse {
    // pagination.limit = 10 (clamped to max 100)
    // pagination.offset = 20
}
```

---

## Extractors

### `Json<T>`

Deserializes the request body using **sonic-rs** (ARM NEON optimized).

```rust
use chopin_core::extractors::Json;

async fn create(Json(payload): Json<CreateRequest>) -> impl IntoResponse {
    // payload is deserialized from the request body
}
```

### `AuthUser`

Extracts the authenticated user ID from the JWT token in the `Authorization` header.

```rust
use chopin_core::extractors::AuthUser;

async fn me(AuthUser(user_id): AuthUser) -> impl IntoResponse {
    // user_id: i32 — the authenticated user's ID
}
```

Requires the `Authorization: Bearer <token>` header. Returns `401 UNAUTHORIZED` if missing or invalid.

### `Pagination`

Extracts pagination parameters from the query string.

```rust
use chopin_core::extractors::Pagination;

async fn list(pagination: Pagination) -> impl IntoResponse {
    let p = pagination.clamped(); // limit capped at 100
}
```

---

## OpenAPI / Swagger

Chopin auto-generates OpenAPI documentation from `#[utoipa::path]` annotations.

### Accessing Documentation

| URL                           | Description                    |
|-------------------------------|--------------------------------|
| `GET /api-docs`               | Interactive Swagger UI (Scalar)|
| `GET /api-docs/openapi.json`  | OpenAPI 3.x spec (JSON)        |

### CLI Export

```bash
# Export as JSON
chopin docs export --format json --output openapi.json

# Export as YAML
chopin docs export --format yaml --output openapi.yaml
```

### Annotating Handlers

All generated controllers include utoipa macros automatically:

```rust
#[utoipa::path(
    post,
    path = "/api/posts",
    request_body = CreatePostRequest,
    responses(
        (status = 201, description = "Post created", body = ApiResponse<PostResponse>),
        (status = 400, description = "Invalid input")
    ),
    tag = "posts"
)]
async fn create(Json(payload): Json<CreatePostRequest>) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // ...
}
```

### Security

JWT Bearer authentication is pre-configured in the OpenAPI spec. All endpoints document the `bearer_auth` security scheme.

---

## Database

### Supported Databases

| Database   | Feature          | Connection URL Example                |
|------------|------------------|---------------------------------------|
| SQLite     | `sqlx-sqlite`    | `sqlite://app.db?mode=rwc`            |
| PostgreSQL | `sqlx-postgres`  | `postgres://user:pass@localhost/mydb`  |
| MySQL      | `sqlx-mysql`     | `mysql://user:pass@localhost/mydb`     |

### Connection Pool

Default pool settings:
- Max connections: 100
- Min connections: 5
- Connect timeout: 8s
- Idle timeout: 8s

### Migrations

Migrations run automatically when the server starts. They are idempotent (safe to run multiple times).

---

## Models

Define models using SeaORM's derive macros:

```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub title: String,
    pub body: String,
    pub author_id: i32,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}
```

Use `chopin generate model` to scaffold these automatically.
