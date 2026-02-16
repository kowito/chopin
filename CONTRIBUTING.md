# Contributing to Chopin

Thank you for contributing to Chopin! This guide will help you understand the project structure and development workflow.

## Getting Started

```bash
git clone https://github.com/your-org/chopin.git
cd chopin
cargo build
cargo test
```

## Workspace Structure

```
chopin/
├── chopin-core/         # Framework library (ChopinModule trait, auth, routing, etc.)
├── chopin-cli/          # CLI tool for scaffolding modules and projects
├── chopin-examples/     # Example applications showing module patterns
│   ├── hello-world/     # Minimal example
│   ├── basic-api/       # Full CRUD with MVSR pattern
│   └── benchmark/       # Performance benchmark
└── docs/                # Documentation (architecture, guides, tutorials)
    ├── modular-architecture.md  # ChopinModule guide
    └── ARCHITECTURE.md           # System design
```

## Architecture Principles

Chopin follows a **modular hub-and-spoke architecture** inspired by Django:

1. **ChopinModule trait** — All features are composable modules
2. **Hub-and-spoke** — Modules depend on core, never on each other
3. **MVSR pattern** — Model-View-Service-Router separation for testability

Read [docs/modular-architecture.md](docs/modular-architecture.md) before contributing features.

## Development Workflow

### Build

```bash
cargo build                              # Debug build
cargo build --release                    # Release build
cargo build --release --features perf    # With mimalloc + SIMD JSON
```

### Test

```bash
cargo test                               # All tests (310+ tests)
cargo test -p chopin-core                # Core library only
cargo test -p chopin-basic-api           # Example tests
cargo test --test auth_tests             # Specific test file
```

We maintain comprehensive test coverage with **310+ tests across 24 test files**.

### Run Examples

```bash
# Hello World
cargo run -p chopin-hello-world

# Basic API (shows MVSR pattern)
cargo run -p chopin-basic-api

# Benchmark (with performance features)
REUSEPORT=true cargo run -p chopin-benchmark --release --features chopin/perf
```

**Note:** All examples automatically enable logging. You'll see request traces, database migrations, and server startup logs. To adjust log levels, use the `RUST_LOG` environment variable:

```bash
# Debug level (verbose)
RUST_LOG=debug cargo run -p chopin-hello-world

# Warn level (minimal)
RUST_LOG=warn cargo run -p chopin-hello-world
```

See [docs/debugging-and-logging.md](docs/debugging-and-logging.md) for more details.
## Code Style

- Use `rustfmt` for formatting: `cargo fmt --all`
- Use `clippy` for linting: `cargo clippy --all --all-targets -- -D warnings`
- Follow Rust naming conventions
- Add doc comments to all public items
- All code must pass clippy with zero warnings
- Services should be 100% unit-testable (no HTTP dependencies)

## Developing a New Module

Chopin modules follow the **MVSR pattern** (Model-View-Service-Router):

### 1. Define the Module

```rust
pub struct BlogModule;

#[async_trait]
impl ChopinModule for BlogModule {
    fn name(&self) -> &'static str {
        "blog"
    }

    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/posts", get(handlers::list_posts).post(handlers::create_post))
            .route("/posts/:id", get(handlers::get_post))
    }

    async fn migrations(&self) -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20240101_create_posts::Migration)]
    }
}
```

### 2. Create Services (business logic)

```rust
// blog/services.rs
pub async fn get_posts(
    db: &DatabaseConnection,
    page: u64,
    per_page: u64,
) -> Result<Vec<Post>, ChopinError> {
    Post::find()
        .order_by_desc(post::Column::CreatedAt)
        .paginate(db, per_page)
        .fetch_page(page)
        .await
        .map_err(Into::into)
}

// ✅ 100% unit-testable - no HTTP, no State extraction
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_get_posts() {
        let db = test_db().await;
        let posts = get_posts(&db, 0, 10).await.unwrap();
        assert_eq!(posts.len(), 0);
    }
}
```

### 3. Create Handlers (HTTP layer)

```rust
// blog/handlers.rs
pub async fn list_posts(
    State(state): State<AppState>,
    Pagination { page, per_page }: Pagination,
) -> Result<ApiResponse<Vec<PostDto>>, ChopinError> {
    let posts = services::get_posts(&state.db, page, per_page).await?;
    Ok(ApiResponse::success(posts.into_iter().map(Into::into).collect()))
}
```

### 4. Write Tests

```rust
// tests/blog_tests.rs
#[tokio::test]
async fn test_list_posts_api() {
    let app = TestApp::new().await.unwrap();
    
    let response = app.get("/posts").send().await;
    assert_eq!(response.status(), 200);
}
```

### Guidelines

- **Services**: Pure business logic, no HTTP dependencies
- **Handlers**: Thin HTTP adapters that call services
- **Models**: SeaORM entities
- **Migrations**: Self-contained, idempotent
- **Tests**: Unit tests for services, integration tests for handlers

See [docs/modular-architecture.md](docs/modular-architecture.md) for complete patterns.

## Testing

We maintain comprehensive test coverage with 310+ tests across 24 test files:

```bash
cargo test --all                         # Run all tests
cargo test -p chopin-core                # Core library only  
cargo test --test auth_tests             # Specific test file
cargo clippy --all --all-targets -- -D warnings  # Lint check
```

### Test Patterns

**Unit Tests (Services):**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_post() {
        let db = setup_test_db().await;
        let post = services::create_post(&db, "Title", "Content").await.unwrap();
        assert_eq!(post.title, "Title");
    }
}
```

**Integration Tests (Handlers):**
```rust
#[tokio::test]
async fn test_create_post_api() {
    let app = TestApp::new().await.unwrap();
    
    let response = app
        .post("/posts")
        .json(&json!({ "title": "Test", "content": "Body" }))
        .send()
        .await;
    
    assert_eq!(response.status(), 201);
}
```

## Adding a Feature

### Core Features (in chopin-core/)

1. Implement the feature following MVSR pattern
2. Add feature flag to `chopin-core/Cargo.toml` if optional
3. Gate the code with `#[cfg(feature = "...")]` if conditional
4. Add comprehensive tests (services + handlers)
5. Update `docs/modular-architecture.md` if introducing new patterns
6. Run `cargo clippy --all --all-targets -- -D warnings`
7. Run `cargo fmt --all`

### Vendor Modules (not in core)

If your feature is optional/vendor-specific (e.g., payment processor, analytics):

1. Create a separate crate: `chopin-stripe/`, `chopin-analytics/`
2. Implement `ChopinModule` trait
3. Document in the crate's README
4. List in main README's "Vendor Modules" section

Example structure:
```
chopin-stripe/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs          # pub struct StripeModule
    ├── services.rs     # Business logic
    ├── handlers.rs     # HTTP handlers
    └── models.rs       # Data models
```

## Pull Request Process

1. Create a branch from `main`
2. Make your changes
3. Run `cargo test` and `cargo clippy`
4. Update documentation
5. Submit a PR with a clear description

## Feature Flags

| Feature | Purpose |
|---------|---------|
| `redis` | Redis caching backend |
| `graphql` | async-graphql integration |
| `s3` | AWS S3 file storage |
| `perf` | mimalloc global allocator |
