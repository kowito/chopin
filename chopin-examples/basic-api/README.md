# Chopin Basic API Example

A complete CRUD REST API built with the Chopin framework.

Demonstrates controllers, models, migrations, pagination, OpenAPI docs, and integration tests.

## Endpoints

| Method   | Path              | Description           |
|----------|-------------------|-----------------------|
| `GET`    | `/api/posts`      | List posts (paginated)|
| `POST`   | `/api/posts`      | Create a new post     |
| `GET`    | `/api/posts/{id}` | Get a single post     |
| `PUT`    | `/api/posts/{id}` | Update a post         |
| `DELETE` | `/api/posts/{id}` | Delete a post         |
| `GET`    | `/api-docs`       | Scalar API explorer   |

## Quick Start

```bash
# From the workspace root
export DATABASE_URL="sqlite:./basic-api.db?mode=rwc"
export JWT_SECRET="dev-secret"

cargo run -p chopin-basic-api
```

Open http://localhost:3000/api-docs for the interactive API explorer, then try:

```bash
# Create a post
curl -X POST http://localhost:3000/api/posts \
  -H "Content-Type: application/json" \
  -d '{"title": "Hello", "body": "World"}'

# List posts with pagination
curl "http://localhost:3000/api/posts?limit=10&offset=0"

# Update a post
curl -X PUT http://localhost:3000/api/posts/1 \
  -H "Content-Type: application/json" \
  -d '{"title": "Updated Title", "published": true}'

# Delete a post
curl -X DELETE http://localhost:3000/api/posts/1
```

## Run Tests

```bash
cargo test -p chopin-basic-api
```

## Project Structure

```
basic-api/
├── src/
│   ├── main.rs            # Server setup, OpenAPI config
│   ├── controllers/
│   │   ├── mod.rs          # Module declarations
│   │   └── posts.rs        # CRUD handlers + request DTOs
│   ├── models/
│   │   ├── mod.rs          # Module declarations
│   │   └── post.rs         # SeaORM entity + response DTO
│   └── migrations/
│       ├── mod.rs          # Migrator
│       └── m20250101_*.rs  # Create posts table
└── tests/
    └── integration_tests.rs
```
