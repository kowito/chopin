# CLI (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Installation

```bash
cargo install chopin-cli
```

## Commands

### `chopin new <name>`

Create a new Chopin project:

```bash
chopin new my-app
cd my-app
cargo run
```

Generates a complete project with:
- `Cargo.toml` with `chopin-core` dependency
- `src/main.rs` with server setup
- `src/controllers/mod.rs`
- `src/models/mod.rs`
- `src/migrations/mod.rs`
- `.env` with development defaults
- `tests/integration_tests.rs`

### `chopin generate model <name> [fields...]`

Generate a SeaORM model with migration:

```bash
chopin generate model post title:string body:text published:boolean author_id:integer
```

Supported field types:
- `string` → `String` / `VARCHAR`
- `text` → `String` / `TEXT`
- `integer` → `i32` / `INTEGER`
- `bigint` → `i64` / `BIGINT`
- `float` → `f32` / `FLOAT`
- `double` → `f64` / `DOUBLE`
- `boolean` → `bool` / `BOOLEAN`
- `datetime` → `chrono::NaiveDateTime` / `TIMESTAMP`
- `date` → `chrono::NaiveDate` / `DATE`
- `uuid` → `uuid::Uuid` / `UUID`

### `chopin generate controller <name>`

Generate a controller with CRUD routes:

```bash
chopin generate controller posts
```

Creates `src/controllers/posts.rs` with:
- `routes()` function
- `list`, `create`, `get`, `update`, `delete` handlers
- OpenAPI annotations

### `chopin db migrate`

Run all pending migrations:

```bash
chopin db migrate
```

### `chopin db rollback`

Rollback the last migration:

```bash
chopin db rollback
```

### `chopin db reset`

Drop all tables and re-run all migrations:

```bash
chopin db reset
```

### `chopin db seed`

Run seed data (if configured):

```bash
chopin db seed
```

### `chopin db status`

Show migration status:

```bash
chopin db status
```

### `chopin run`

Start the development server:

```bash
chopin run
```

Equivalent to `cargo run` with development defaults.

### `chopin createsuperuser`

Create an admin user interactively:

```bash
chopin createsuperuser
# Enter email: admin@example.com
# Enter username: admin
# Enter password: ********
```

### `chopin docs export`

Export the OpenAPI spec to a file:

```bash
chopin docs export openapi.json
chopin docs export openapi.yaml
```

### `chopin info`

Display framework and environment information:

```bash
chopin info
```

Output:

```
Chopin Framework v0.1.1
━━━━━━━━━━━━━━━━━━━━━━
Rust version: 1.82.0
Edition: 2021
Features: redis, graphql, s3, perf
```

## Global Options

| Flag | Description |
|------|-------------|
| `--help` / `-h` | Show help |
| `--version` / `-V` | Show version |
