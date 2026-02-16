# Chopin Basic API Example

A complete CRUD REST API demonstrating **MVSR pattern** (Model-View-Service-Router) and modular architecture.

This example shows:
- ChopinModule composition
- Service layer (100% unit-testable business logic)
- Handler layer (thin HTTP adapters)
- OpenAPI documentation
- Pagination and validation
- Integration tests

## MVSR Architecture

```
┌─────────────┐
│   Router    │ ← Routes: /posts, /posts/:id
└──────┬──────┘
       │
┌──────▼──────┐
│  Handlers   │ ← HTTP layer: list_posts(), create_post()
│   (View)    │   Extracts State, validates input, calls services
└──────┬──────┘
       │
┌──────▼──────┐
│  Services   │ ← Business logic: get_posts(db, page), create_post(db, data)
│             │   Pure functions, no HTTP dependencies
└──────┬──────┘
       │
┌──────▼──────┐
│   Models    │ ← Database entities: Post
└─────────────┘
```

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

**Note:** This example calls `init_logging()` to enable console output showing server startup, database migrations, and HTTP request traces. See [Debugging & Logging Guide](../../docs/debugging-and-logging.md) for more details.

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

## Project Structure (MVSR Pattern)

```
basic-api/
├── src/
│   ├── main.rs              # Server setup, ChopinModule mounting
│   ├── module.rs            # PostModule (implements ChopinModule trait)
│   ├── services/
│   │   └── posts.rs         # Business logic (100% unit-testable)
│   ├── handlers/
│   │   └── posts.rs         # HTTP handlers (thin adapters)
│   ├── models/
│   │   └── post.rs          # SeaORM entity + DTOs
│   └── migrations/
│       ├── mod.rs           # Migrator
│       └── m20250101_*.rs   # Create posts table
└── tests/
    ├── services_tests.rs    # Unit tests for business logic
    └── integration_tests.rs # Integration tests for API endpoints
```

## Learning Path

1. **Read the code** — Start with `src/main.rs` to see ChopinModule mounting
2. **Services first** — Check `services/posts.rs` for pure business logic
3. **Handlers next** — See `handlers/posts.rs` for HTTP adapters
4. **Run tests** — `cargo test -p chopin-basic-api` to see unit + integration tests

## References

- [Modular Architecture Guide](../../docs/modular-architecture.md) — Complete MVSR pattern details
- [ARCHITECTURE.md](../../ARCHITECTURE.md) — System design and principles
- [Debugging & Logging](../../docs/debugging-and-logging.md) — Enable request traces
