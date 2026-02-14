# Getting Started

**Last Updated:** February 2026

## Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **SQLite** (included by default) or PostgreSQL/MySQL for production

## Installation

### Install the CLI

```bash
cargo install chopin-cli
```

### Create a New Project

```bash
chopin new my-app
cd my-app
```

This generates:

```
my-app/
├── Cargo.toml
├── .env
├── src/
│   ├── main.rs
│   ├── controllers/
│   │   └── mod.rs
│   ├── models/
│   │   └── mod.rs
│   └── migrations/
│       └── mod.rs
└── tests/
    └── integration_tests.rs
```

### Run the Server

```bash
cargo run
```

Output:

```
🎹 Chopin server is running!
   → Mode:    standard
   → Server:  http://127.0.0.1:3000
   → API docs: http://127.0.0.1:3000/api-docs
```

### Built-in Endpoints

Every Chopin app ships with:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Welcome JSON response |
| `/api/auth/signup` | POST | Create a new user |
| `/api/auth/login` | POST | Login and get JWT token |
| `/api-docs` | GET | Interactive Scalar API docs |
| `/api-docs/openapi.json` | GET | Raw OpenAPI 3.1 spec |

In **performance mode**, additional zero-allocation endpoints are available:

| Endpoint | Method | Description |
|----------|--------|-----------|
| `/json` | GET | `{"message":"Hello, World!"}` (raw hyper, zero-alloc) |
| `/plaintext` | GET | `Hello, World!` (raw hyper, zero-alloc) |

**Enable performance mode:**
```bash
SERVER_MODE=performance cargo run --release --features perf
```

## Configuration

Create a `.env` file in your project root:

```env
DATABASE_URL=sqlite://app.db?mode=rwc
JWT_SECRET=change-me-in-production
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
ENVIRONMENT=development
```

See [Configuration](configuration.md) for all options.

## Your First Controller

Generate a controller with the CLI:

```bash
chopin generate controller posts
```

Or create one manually:

```rust
use axum::{Router, routing::get, extract::State};
use chopin_core::{ApiResponse, controllers::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts", get(list_posts))
}

async fn list_posts(State(state): State<AppState>) -> ApiResponse<Vec<String>> {
    ApiResponse::success(vec!["Hello from Chopin!".to_string()])
}
```

Register it in `src/main.rs`:

```rust
let app = chopin_core::App::new().await?;
// The router is built automatically with auth routes + OpenAPI docs
app.run().await?;
```

## Your First Model

Generate a model:

```bash
chopin generate model post title:string body:text published:boolean
```

This creates both the SeaORM entity and a migration. See [Models & Database](models-database.md) for details.

## Next Steps

- [Architecture](architecture.md) — Understand how Chopin works
- [Controllers & Routing](controllers-routing.md) — Build your API endpoints
- [Performance](performance.md) — Enable performance mode for benchmarks
- [Testing](testing.md) — Write integration tests
