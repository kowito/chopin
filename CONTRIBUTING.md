# Contributing to Chopin

Thank you for your interest in contributing to Chopin! This guide will help you get started.

## Development Setup

### Prerequisites

- Rust 1.75+ (stable)
- SQLite (for development/testing)
- Git

### Clone and Build

```bash
git clone https://github.com/yourusername/chopin.git
cd chopin
cargo build
```

### Run Tests

```bash
cargo test
```

Tests use an in-memory SQLite database automatically — no external setup needed.

### Project Structure

```
chopin/
├── chopin-core/          # Main framework library
│   ├── src/
│   │   ├── app.rs        # Application struct & server
│   │   ├── auth/         # JWT + password hashing
│   │   ├── config.rs     # Environment configuration
│   │   ├── controllers/  # Built-in auth controllers
│   │   ├── db.rs         # Database connection
│   │   ├── error.rs      # Error types
│   │   ├── extractors/   # Axum extractors (Json, AuthUser, Pagination)
│   │   ├── migrations/   # SeaORM migrations
│   │   ├── models/       # Built-in User model
│   │   ├── openapi.rs    # OpenAPI/Swagger setup
│   │   ├── response.rs   # ApiResponse wrapper
│   │   ├── routing.rs    # Route builder
│   │   └── testing.rs    # Test utilities (TestApp, TestClient)
│   └── tests/            # Integration tests
├── chopin-cli/           # CLI scaffolding tool
├── chopin-examples/      # Example projects
└── docs/                 # Documentation
```

## How to Contribute

### Reporting Issues

- Search existing issues before opening a new one
- Include steps to reproduce, expected behavior, and actual behavior
- Include Rust version (`rustc --version`) and OS

### Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass: `cargo test`
6. Ensure no warnings: `cargo clippy`
7. Format code: `cargo fmt`
8. Commit with clear messages
9. Push and open a Pull Request

### Commit Messages

Use clear, descriptive commit messages:

```
feat: add rate limiting middleware
fix: handle empty email in signup validation
docs: update API reference for pagination
test: add integration tests for login flow
refactor: extract JWT logic into auth module
```

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Follow Rust naming conventions
- Add doc comments (`///`) for public APIs
- Keep functions focused and small

## Areas for Contribution

### Good First Issues

- Adding more field type mappings to the CLI generator
- Improving error messages
- Adding more test coverage
- Documentation improvements

### Feature Areas

- **Permissions system**: Role-based access control
- **Caching**: Redis integration layer
- **Background jobs**: Async task queue
- **File uploads**: Storage abstraction
- **Rate limiting**: Request throttling middleware
- **GraphQL**: Alternative API layer

## Testing Guidelines

- All new features must include tests
- Use `TestApp` for integration tests
- Test both success and error paths
- Test edge cases (empty inputs, duplicates, invalid tokens)

Example test:

```rust
use chopin_core::TestApp;

#[tokio::test]
async fn test_my_feature() {
    let app = TestApp::new().await;

    let res = app.client.get(&app.url("/api/my-endpoint")).await;

    assert_eq!(res.status, 200);
    assert!(res.is_success());
}
```

## License

By contributing to Chopin, you agree that your contributions will be licensed under the MIT License.
