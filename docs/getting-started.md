# Getting Started with Chopin

## Prerequisites

- **Rust** (1.75 or later): [Install Rust](https://www.rust-lang.org/tools/install)
- **SQLite** (included by default) or **PostgreSQL/MySQL** for production

## Installation

Install the Chopin CLI:

```bash
cargo install chopin-cli
```

## Create Your First Project

```bash
chopin new my-api
cd my-api
```

This creates:

```
my-api/
├── src/
│   ├── main.rs           # Entry point
│   ├── models/           # Database models
│   └── controllers/      # API controllers
├── .cargo/config.toml    # ARM optimization flags
├── Cargo.toml            # Dependencies
├── .env                  # Environment config
├── .env.example          # Template
└── README.md
```

## Start the Server

```bash
cargo run
```

The server starts at `http://127.0.0.1:5000`.

API documentation is served at `http://127.0.0.1:5000/api-docs`.

## Built-in Authentication

Chopin ships with a complete authentication system out of the box.

### Sign Up

```bash
curl -X POST http://localhost:5000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "username": "john",
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
      "email": "user@example.com",
      "username": "john",
      "is_active": true,
      "created_at": "2026-02-11T00:00:00"
    }
  }
}
```

### Log In

```bash
curl -X POST http://localhost:5000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "secret123"
  }'
```

### Using the Token

Include the JWT token in the `Authorization` header for protected endpoints:

```bash
curl -X GET http://localhost:5000/api/posts \
  -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGc..."
```

## Generate a Model

Chopin's CLI generates models, migrations, and controllers in one command:

```bash
chopin generate model Post title:string body:text author_id:i32
```

This creates three files:
- `src/models/post.rs` — SeaORM entity with all fields
- `src/controllers/post.rs` — CRUD endpoints with utoipa annotations
- `src/migrations/m<timestamp>_create_posts_table.rs` — Database migration

### Supported Field Types

| Shorthand               | Rust Type       |
|--------------------------|-----------------|
| `string`, `str`          | `String`        |
| `text`                   | `String`        |
| `i32`, `int`, `integer`  | `i32`           |
| `i64`, `bigint`          | `i64`           |
| `f32`, `float`           | `f32`           |
| `f64`, `double`          | `f64`           |
| `bool`, `boolean`        | `bool`          |
| `datetime`, `timestamp`  | `NaiveDateTime` |
| `uuid`                   | `Uuid`          |

### Register Generated Code

After generating, add the module to your `src/models/mod.rs`:

```rust
pub mod post;
```

And add the routes in `src/routing.rs` (or `src/controllers/mod.rs`).

## Generate a Controller (Standalone)

```bash
chopin generate controller comments
```

Creates `src/controllers/comments.rs` with list and get-by-id endpoints.

## Database Migrations

Migrations run **automatically on startup**. You can also trigger them manually:

```bash
chopin db migrate
```

### Supported Databases

| Database   | Connection URL                              |
|------------|---------------------------------------------|
| SQLite     | `sqlite://app.db?mode=rwc`                  |
| PostgreSQL | `postgres://user:pass@localhost/mydb`        |
| MySQL      | `mysql://user:pass@localhost/mydb`           |

Set `DATABASE_URL` in your `.env` file.

## API Documentation

Chopin auto-generates OpenAPI documentation from your handler annotations.

- **Interactive docs**: `http://localhost:5000/api-docs`
- **OpenAPI JSON**: `http://localhost:5000/api-docs/openapi.json`

Export the spec to a file:

```bash
chopin docs export --format json --output openapi.json
chopin docs export --format yaml --output openapi.yaml
```

## Configuration

All configuration is via environment variables (`.env` file):

```env
# Database
DATABASE_URL=sqlite://app.db?mode=rwc

# JWT
JWT_SECRET=your-secret-key-here
JWT_EXPIRY_HOURS=24

# Server
SERVER_PORT=5000
SERVER_HOST=127.0.0.1

# Environment (development, production, test)
ENVIRONMENT=development
```

## Testing

Chopin provides test utilities for integration testing:

```rust
use chopin_core::TestApp;

#[tokio::test]
async fn test_create_post() {
    let app = TestApp::new().await;

    // Create and authenticate a user
    let (token, _user) = app.create_user("test@example.com", "testuser", "password123").await;

    // Make authenticated requests
    let res = app.client
        .post_with_auth(
            &app.url("/api/posts"),
            &token,
            r#"{"title": "Hello", "body": "World"}"#,
        )
        .await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());
}
```

`TestApp` automatically:
- Creates an in-memory SQLite database
- Runs all migrations
- Starts the server on a random port
- Provides helper methods for auth flows

## Build for Production

```bash
cargo build --release
```

On Apple Silicon, the release build includes:
- Full Link-Time Optimization (LTO)
- ARM NEON SIMD instructions for JSON
- Hardware AES acceleration for JWT
- Native CPU targeting

## Next Steps

- [API Reference](api.md) — Request/response format, error codes, pagination
- [Examples](../chopin-examples/) — Working example projects
