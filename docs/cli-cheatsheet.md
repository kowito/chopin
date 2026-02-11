# Chopin CLI Cheat Sheet

Quick reference for common Chopin CLI commands.

## Project Setup

```bash
# Install CLI
cargo install --path chopin-cli

# Create new project
chopin new my-api
cd my-api

# Start server
chopin run
# → Server: http://127.0.0.1:3000
# → Docs: http://127.0.0.1:3000/api-docs
```

## Code Generation

### Models

```bash
# Basic model
chopin generate model Post title:string body:text

# With multiple fields
chopin generate model Product \
  name:string \
  description:text \
  price:f64 \
  stock:i32 \
  available:bool

# With relationships
chopin generate model Comment \
  post_id:i32 \
  author_id:i32 \
  content:text
```

### Controllers

```bash
# Standalone controller
chopin generate controller analytics
chopin generate controller webhooks
```

## Field Types

| Shorthand | Rust Type | Database | Example |
|-----------|-----------|----------|---------|
| `string`, `str` | `String` | VARCHAR | `name:string` |
| `text` | `String` | TEXT | `body:text` |
| `int`, `i32` | `i32` | INTEGER | `count:int` |
| `i64`, `bigint` | `i64` | BIGINT | `user_id:i64` |
| `f32`, `float` | `f32` | FLOAT | `rating:f32` |
| `f64`, `double` | `f64` | DOUBLE | `price:f64` |
| `bool` | `bool` | BOOLEAN | `active:bool` |
| `datetime` | `NaiveDateTime` | TIMESTAMP | `expires_at:datetime` |
| `uuid` | `Uuid` | UUID | `token:uuid` |

## Database

```bash
# Migrations run automatically on startup
cargo run

# Manual migration (rarely needed)
chopin db migrate

# View migration status
chopin db status

# Rollback last migration
chopin db rollback

# Reset database (drops all tables, re-runs migrations)
chopin db reset

# Seed database with test data
chopin db seed
```

## Documentation

```bash
# Export OpenAPI spec
chopin docs export                                    # → openapi.json
chopin docs export --format yaml                      # → openapi.json (as YAML)
chopin docs export --output api-spec.yaml --format yaml
```

## User Management

```bash
# Create a superuser account
chopin createsuperuser

# Create with command-line options
chopin createsuperuser \
  --email admin@example.com \
  --password SecurePass123 \
  --name "Admin User"
```

## Diagnostics

```bash
# Show project information
chopin info

# Displays:
# - Project name
# - Framework version
# - Database config
# - Server config
# - Available features
```

## Common Workflows

### Blog API

```bash
chopin new blog-api && cd blog-api
chopin generate model Post title:string slug:string body:text published:bool
chopin generate model Comment post_id:i32 author:string content:text
chopin generate model Tag name:string
# Register models & controllers in mod.rs files
cargo run
```

### E-commerce API

```bash
chopin new shop-api && cd shop-api
chopin generate model Product name:string price:f64 stock:i32
chopin generate model Order user_id:i32 total:f64 status:string
chopin generate model OrderItem order_id:i32 product_id:i32 quantity:i32
cargo run
```

### Task Manager

```bash
chopin new tasks-api && cd tasks-api
chopin generate model Task title:string description:text completed:bool priority:i32
chopin generate model Project name:string description:text
chopin generate model Label name:string color:string
cargo run
```

## Project Structure

```
my-api/
├── Cargo.toml
├── .env                    # Environment config (gitignored)
├── .env.example            # Template
├── src/
│   ├── main.rs
│   ├── models/
│   │   ├── mod.rs         # pub mod post;
│   │   └── post.rs        # Generated
│   ├── controllers/
│   │   ├── mod.rs         # pub mod post;
│   │   └── post.rs        # Generated
│   └── migrations/
│       ├── mod.rs
│       └── m*_*.rs        # Generated
└── README.md
```

## Environment Variables

```bash
# .env file
DATABASE_URL=sqlite://app.db?mode=rwc          # SQLite
# DATABASE_URL=postgres://user:pass@host/db    # PostgreSQL
# DATABASE_URL=mysql://user:pass@host/db       # MySQL

JWT_SECRET=your-secret-key-change-in-production
JWT_EXPIRY_HOURS=24

SERVER_PORT=3000
SERVER_HOST=127.0.0.1
ENVIRONMENT=development

# Logging level: trace, debug, info, warn, error
RUST_LOG=debug

# Optional: S3-compatible object storage
# S3_BUCKET=my-bucket
# S3_REGION=us-east-1
# S3_ENDPOINT=https://account.r2.cloudflarestorage.com  # R2/MinIO
# S3_ACCESS_KEY_ID=your-key
# S3_SECRET_ACCESS_KEY=your-secret
# S3_PUBLIC_URL=https://cdn.example.com
```

## After Generating

### 1. Register Model

Edit `src/models/mod.rs`:
```rust
pub mod post;
```

### 2. Register Controller

Edit `src/controllers/mod.rs`:
```rust
pub mod post;
```

### 3. Add Routes

In your routing setup (main.rs or routing.rs):
```rust
.nest("/posts", crate::controllers::post::routes())
```

### 4. Run Server

```bash
cargo run
# Migrations apply automatically
```

## Testing Endpoints

```bash
# Interactive docs
open http://localhost:3000/api-docs

# CLI testing
curl http://localhost:3000/api/posts

# With httpie
http GET :3000/api/posts

# Create post
http POST :3000/api/posts \
  Authorization:"Bearer $TOKEN" \
  title="Hello World" \
  body="My first post"
```

## Tips

- Use **PascalCase** for models: `Post`, `UserProfile`
- Use **snake_case** for fields: `created_at`, `user_id`
- Models auto-pluralize: `Post` → `posts` table, `/api/posts` route
- Timestamps (`created_at`, `updated_at`) added automatically
- Never edit applied migrations
- Use `.env` for secrets (gitignored)
- Check `.env.example` for configuration options

## Development Tools

```bash
# Auto-reload on file changes
cargo install cargo-watch
cargo watch -x run

# Database inspection
# SQLite: DB Browser for SQLite
# PostgreSQL: pgAdmin
# MySQL: MySQL Workbench

# API testing
# - Swagger UI (built-in): http://localhost:3000/api-docs
# - HTTPie: https://httpie.io
# - Postman: https://postman.com
```

## Getting Help

```bash
chopin --help                    # All commands
chopin new --help                # Command help
chopin generate model --help     # Subcommand help
```

📚 **Full Documentation**: [cli.md](cli.md)
