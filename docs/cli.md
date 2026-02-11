# Chopin CLI Reference

The `chopin` command-line interface helps you scaffold and manage your Chopin projects.

## Table of Contents

- [Installation](#installation)
- [Quick Reference](#quick-reference)
- [Commands](#commands)
  - [chopin new](#chopin-new) - Create a new project
  - [chopin generate](#chopin-generate) - Scaffold components
    - [model](#chopin-generate-model) - Generate model, migration, and controller
    - [controller](#chopin-generate-controller) - Generate standalone controller
  - [chopin db](#chopin-db) - Database operations
  - [chopin docs](#chopin-docs) - OpenAPI documentation
  - [chopin run](#chopin-run) - Start development server
- [Complete Workflow Example](#complete-workflow-example)
- [Tips & Best Practices](#tips--best-practices)
- [Troubleshooting](#troubleshooting)

## Quick Reference

```bash
# Project Management
chopin new my-api                     # Create new project
chopin run                             # Start dev server

# Code Generation
chopin generate model Post title:string body:text published:bool
chopin generate controller analytics

# Database
chopin db migrate                      # Run migrations (rarely needed)

# Documentation
chopin docs export                     # Export as JSON
chopin docs export --format yaml       # Export as YAML

# Help
chopin --help                          # Show all commands
chopin <command> --help                # Command-specific help
```

## Installation

```bash
cargo install --path chopin-cli
```

Or from your project directory:

```bash
cd chopin/chopin-cli
cargo install --path .
```

Verify installation:

```bash
chopin --version
```

## Commands

### `chopin new`

Create a new Chopin project with a complete project structure.

**Usage:**

```bash
chopin new <project-name>
```

**Example:**

```bash
chopin new my-api
cd my-api
cargo run
```

**What it creates:**

```
my-api/
├── Cargo.toml
├── .env
├── .env.example
├── .gitignore
├── .cargo/
│   └── config.toml
├── README.md
└── src/
    ├── main.rs
    ├── models/
    ├── controllers/
    └── migrations/
```

The generated project includes:
- Basic Chopin application setup
- Environment configuration (.env)
- SQLite database by default
- JWT authentication configured
- Development server on port 3000
- Apple Silicon optimizations (.cargo/config.toml)

**After creation:**

```bash
cd <project-name>
cargo run
```

The server starts at `http://127.0.0.1:3000` with API docs at `http://127.0.0.1:3000/api-docs`.

---

### `chopin generate`

Scaffold new components for your application.

#### `chopin generate model`

Generate a complete CRUD stack: model entity, database migration, and REST controller.

**Usage:**

```bash
chopin generate model <ModelName> <field:type> [field:type...]
```

**Field Types:**

| Type | Rust Type | Database Type | Description |
|------|-----------|---------------|-------------|
| `string`, `str` | `String` | string | Variable-length text |
| `text` | `String` | text | Long text (no length limit) |
| `int`, `integer`, `i32` | `i32` | integer | 32-bit integer |
| `i64`, `bigint` | `i64` | big_integer | 64-bit integer |
| `f32`, `float` | `f32` | float | 32-bit floating point |
| `f64`, `double` | `f64` | double | 64-bit floating point |
| `bool`, `boolean` | `bool` | boolean | Boolean value |
| `datetime`, `timestamp` | `NaiveDateTime` | timestamp | Date and time |
| `uuid` | `Uuid` | uuid | UUID |

**Examples:**

```bash
# Blog post with title and body
chopin generate model Post title:string body:text published:bool

# Product with pricing
chopin generate model Product name:string price:f64 stock:i32

# User profile
chopin generate model Profile user_id:i32 bio:text avatar_url:string
```

**Generated Files:**

1. **Model** (`src/models/post.rs`):
   - SeaORM entity
   - Model struct with all fields
   - Response struct (safe for API responses)
   - Automatic timestamps (`created_at`, `updated_at`)

2. **Migration** (`src/migrations/m<timestamp>_create_posts_table.rs`):
   - Up migration (creates table)
   - Down migration (drops table)
   - All fields with proper types

3. **Controller** (`src/controllers/post.rs`):
   - `GET /api/posts` - List all
   - `POST /api/posts` - Create new
   - `GET /api/posts/:id` - Get by ID
   - Full OpenAPI documentation

**Next Steps:**

After generating, register your new modules:

```rust
// src/models/mod.rs
pub mod post;

// src/controllers/mod.rs
pub mod post;

// src/routing.rs or main route configuration
.nest("/posts", crate::controllers::post::routes())
```

Then run migrations:

```bash
cargo run  # Migrations run automatically on startup
```

---

#### `chopin generate controller`

Generate a standalone controller without a model (useful for custom endpoints).

**Usage:**

```bash
chopin generate controller <name>
```

**Example:**

```bash
chopin generate controller analytics
```

**Generated File:**

`src/controllers/analytics.rs` with:
- Basic route structure
- List and get-by-id handler templates
- OpenAPI path documentation
- TODO comments for custom implementation

**Next Steps:**

1. Register the controller in `src/controllers/mod.rs`:
   ```rust
   pub mod analytics;
   ```

2. Add routes to your router configuration:
   ```rust
   .nest("/analytics", crate::controllers::analytics::routes())
   ```

3. Implement your custom logic in the handler functions

---

### `chopin db`

Database operations and migration management.

#### `chopin db migrate`

Run pending database migrations.

**Usage:**

```bash
chopin db migrate
```

**Note:** Chopin automatically runs migrations on server startup, so this command is rarely needed. It's equivalent to running:

```bash
cargo run
```

Migrations are applied in order based on their timestamp. Each migration file includes both `up()` (apply) and `down()` (rollback) functions.

---

### `chopin docs`

OpenAPI documentation operations.

#### `chopin docs export`

Export your API's OpenAPI specification to a file.

**Usage:**

```bash
chopin docs export [--format <format>] [--output <file>]
```

**Options:**

- `--format <format>` - Output format: `json` or `yaml` (default: `json`)
- `--output <file>` - Output file path (default: `openapi.json`)

**Examples:**

```bash
# Export as JSON (default)
chopin docs export

# Export as YAML
chopin docs export --format yaml --output api-spec.yaml

# Export to specific location
chopin docs export --output docs/openapi.json
```

**Use Cases:**

- Share API specifications with frontend teams
- Generate client SDKs
- Import into API testing tools (Postman, Insomnia)
- Documentation hosting (Swagger UI, Redoc)

---

### `chopin run`

Start the development server.

**Usage:**

```bash
chopin run
```

This is a convenience command equivalent to `cargo run`. It:
- Compiles your application
- Runs pending database migrations
- Starts the HTTP server
- Displays server URLs with emoji indicator

**Output Example:**

```
🎹 Chopin server is running!
   → Server: http://127.0.0.1:3000
   → API docs: http://127.0.0.1:3000/api-docs
```

**Environment Variables:**

Configure your server via `.env`:

```bash
DATABASE_URL=sqlite://app.db?mode=rwc
JWT_SECRET=your-secret-key
SERVER_PORT=3000
SERVER_HOST=127.0.0.1
ENVIRONMENT=development
```

---

## Complete Workflow Example

Here's a complete example of building a blog API:

```bash
# 1. Create new project
chopin new blog-api
cd blog-api

# 2. Generate Post model
chopin generate model Post title:string slug:string body:text published:bool

# 3. Generate Comment model
chopin generate model Comment post_id:i32 author:string content:text

# 4. Register models (edit src/models/mod.rs)
# Add:
#   pub mod post;
#   pub mod comment;

# 5. Register controllers (edit src/controllers/mod.rs)
# Add:
#   pub mod post;
#   pub mod comment;

# 6. Configure routes (edit your routing setup)
# Add:
#   .nest("/posts", crate::controllers::post::routes())
#   .nest("/comments", crate::controllers::comment::routes())

# 7. Run the server (migrations apply automatically)
cargo run

# 8. Test your API
curl http://localhost:3000/api-docs

# 9. Export OpenAPI spec
chopin docs export --format yaml --output api-spec.yaml
```

---

## Tips & Best Practices

### Model Naming Conventions

- Use **PascalCase** for model names: `Post`, `UserProfile`, `OrderItem`
- The CLI automatically:
  - Converts to snake_case for file names: `post.rs`, `user_profile.rs`
  - Pluralizes for table names: `posts`, `user_profiles`
  - Creates plural routes: `/api/posts`, `/api/user_profiles`

### Field Naming

- Use **snake_case** for field names: `created_at`, `user_id`, `is_published`
- Follow Rust conventions
- Avoid SQL reserved keywords

### Migration Files

- Generated with timestamps: `m20260211_143022_create_posts_table.rs`
- Applied in chronological order
- Never edit applied migrations
- Create new migrations for schema changes

### Project Structure

Keep your project organized:

```
src/
├── main.rs              # Application entry point
├── models/
│   ├── mod.rs          # Export all models
│   ├── user.rs
│   ├── post.rs
│   └── comment.rs
├── controllers/
│   ├── mod.rs          # Export all controllers
│   ├── auth.rs
│   ├── post.rs
│   └── comment.rs
└── migrations/
    ├── mod.rs
    └── m*_*.rs         # Generated migrations
```

### Environment Configuration

- Use `.env.example` for documentation
- Git ignore `.env` (secrets)
- Override in production with environment variables
- Example production config:
  ```bash
  DATABASE_URL=postgres://user:pass@host/db
  JWT_SECRET=<strong-random-secret>
  SERVER_PORT=8080
  SERVER_HOST=0.0.0.0
  ENVIRONMENT=production
  ```

### Development Tips

1. **Hot Reload**: Use `cargo-watch` for auto-restart:
   ```bash
   cargo install cargo-watch
   cargo watch -x run
   ```

2. **Database GUI**: Use tools to inspect your database:
   - SQLite: [DB Browser for SQLite](https://sqlitebrowser.org/)
   - PostgreSQL: [pgAdmin](https://www.pgadmin.org/)

3. **API Testing**: Test endpoints with:
   - Built-in Swagger UI: `http://localhost:3000/api-docs`
   - [HTTPie](https://httpie.io/): `http POST :3000/api/posts title="Hello"`
   - [Postman](https://www.postman.com/)

4. **Generate Multiple Models**: Script it:
   ```bash
   #!/bin/bash
   chopin generate model Post title:string body:text
   chopin generate model Comment post_id:i32 content:text
   chopin generate model Tag name:string
   ```

---

## Troubleshooting

### `chopin: command not found`

**Solution:** Add Cargo bin to your PATH:

```bash
# In ~/.zshrc or ~/.bashrc
export PATH="$HOME/.cargo/bin:$PATH"

# Reload
source ~/.zshrc
```

### `Failed to write model file`

**Cause:** Not in a Chopin project directory or missing `src/` folder.

**Solution:** Ensure you're in a project created with `chopin new`.

### Migrations Not Applied

**Check:**
1. Database connection in `.env` is correct
2. Database file/server is accessible
3. Check logs when running `cargo run`

**Manual Fix:**
```bash
# Delete database and restart
rm app.db
cargo run
```

### Dependency Version Conflicts

If you see errors about `chopin-core` versions:

**Solution:** Update your `Cargo.toml`:

```toml
[dependencies]
chopin-core = { path = "../path/to/chopin/chopin-core" }
```

Or wait for crates.io publication and use:

```toml
chopin-core = "0.1.0"
```

---

## Getting Help

- Run `chopin --help` for quick command reference
- Run `chopin <command> --help` for specific command help
- Check project [README](../README.md)
- Read the [Getting Started Guide](getting-started.md)
- See [API Documentation](api.md)

---

## Version

This documentation is for Chopin CLI v0.1.0.

Check your version:

```bash
chopin --version
```
