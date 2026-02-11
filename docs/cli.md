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
    - [migrate](#chopin-db-migrate) - Run migrations
    - [rollback](#chopin-db-rollback) - Rollback migrations
    - [status](#chopin-db-status) - Show migration status
    - [reset](#chopin-db-reset) - Reset database
    - [seed](#chopin-db-seed) - Seed with sample data
  - [chopin createsuperuser](#chopin-createsuperuser) - Create admin account
  - [chopin info](#chopin-info) - Show project information
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
chopin db migrate                      # Run migrations
chopin db rollback                     # Rollback last migration
chopin db rollback --steps 3           # Rollback 3 migrations
chopin db status                       # Show migration status
chopin db reset                        # Reset entire database
chopin db seed                         # Seed with sample data

# User Management
chopin createsuperuser                 # Create admin account interactively

# Project Info
chopin info                            # Show project status

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

#### `chopin db rollback`

Rollback database migrations.

**Usage:**

```bash
chopin db rollback [--steps <number>]
```

**Options:**

- `--steps <number>` - Number of migrations to rollback (default: 1)

**Examples:**

```bash
# Rollback the last migration
chopin db rollback

# Rollback the last 3 migrations
chopin db rollback --steps 3

# Rollback all migrations
chopin db reset
```

**Important:**
- Requires `down()` methods in migration files
- Data may be lost during rollback
- Always backup production databases before rolling back

#### `chopin db status`

Show the status of your migrations.

**Usage:**

```bash
chopin db status
```

**Output:**

```
🎹 Migration status:

  Found 3 migration(s):
    📄 m20250211_000001_create_users_table.rs
    📄 m20250211_120530_create_posts_table.rs
    📄 m20250212_093045_create_comments_table.rs

  Hint: Run `chopin db migrate` to apply pending migrations.
  Hint: Run `chopin db rollback` to rollback the last migration.
```

#### `chopin db reset`

Reset the entire database by dropping all tables and re-running migrations.

**Usage:**

```bash
chopin db reset
```

**Warning:** This is destructive! It will:
1. Delete your SQLite database file (or drop all tables)
2. Re-run all migrations from scratch
3. **All data will be lost**

**Interactive confirmation:**

```bash
🎹 Resetting database...
  ⚠  This will drop all tables and re-run all migrations!

  Are you sure? (yes/no): yes
```

**Use cases:**
- Development: Fresh start after schema changes
- Testing: Clean slate between test runs
- **Never** use in production without backups!

#### `chopin db seed`

Seed the database with sample data.

**Usage:**

```bash
chopin db seed
```

**Setup:**

Create a `src/seed.rs` file:

```rust
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

pub async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Insert sample data here
    Ok(())
}
```

**Example seed file:**

```rust
use sea_orm::{DatabaseConnection, DbErr, ActiveModelTrait, Set};
use crate::models::user;

pub async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {
    let now = chrono::Utc::now().naive_utc();
    
    // Create sample users
    let user1 = user::ActiveModel {
        email: Set("alice@example.com".to_string()),
        username: Set("alice".to_string()),
        password_hash: Set("hashed_password".to_string()),
        role: Set("user".to_string()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    user1.insert(db).await?;
    
    Ok(())
}
```

---

### `chopin createsuperuser`

Create an admin (superuser) account interactively.

**Usage:**

```bash
chopin createsuperuser
```

**Interactive prompts:**

```
🎹 Creating superuser account...

  Email: admin@example.com
  Username: admin
  Password: ********
  Confirm password: ********

  ✓ Superuser 'admin' created successfully!
```

**What it creates:**
- A user account with `role = "superuser"`
- Password is automatically hashed with Argon2
- Account is set as active (`is_active = true`)

**Use cases:**
- Initial admin account setup
- Creating admin users for production
- Testing role-based access control

**Role hierarchy:**
- **user** (0) - Default role, basic access
- **admin** (1) - Administrative access
- **superuser** (2) - Full system access

---

### `chopin info`

Show project information and status.

**Usage:**

```bash
chopin info
```

**Output:**

```
🎹 Chopin Project Info

  Project: my-api
  Version: 0.1.0
  Config:  .env ✓
  Models:  3 model file(s)
  Ctrls:   2 controller file(s)
  Migrs:   5 migration(s)
  DB:      app.db (245 KB)
```

**Displays:**
- Project name and version (from Cargo.toml)
- Configuration status (.env file presence)
- Number of models, controllers, and migrations
- Database file size (if SQLite)

**Use cases:**
- Quick project overview
- Verify project structure
- Check if migrations exist

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
