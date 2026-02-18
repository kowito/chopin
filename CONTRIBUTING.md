# Contributing to Chopin

Thank you for contributing to Chopin! This guide will help you get started.

## Quick Start

```bash
git clone https://github.com/kowito/chopin.git
cd chopin
cargo build
cargo test
```

## Architecture Principles

Chopin follows a **modular hub-and-spoke architecture** inspired by Django:

1. **ChopinModule trait** — All features are composable modules
2. **Hub-and-spoke** — Modules depend on core, never on each other  
3. **MVSR pattern** — Model-View-Service-Router separation for testability

**Read these before contributing features:**
- [docs/modular-architecture.md](docs/modular-architecture.md) — Complete guide with examples
- [ARCHITECTURE.md](ARCHITECTURE.md) — System design and design principles

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
cargo test --test auth_tests             # Specific test file
```

See [docs/QUICK_START.md](docs/QUICK_START.md) for running examples.

## Code Style

- Use `rustfmt` for formatting: `cargo fmt --all`
- Use `clippy` for linting: `cargo clippy --all --all-targets -- -D warnings`
- Follow Rust naming conventions
- Add doc comments to all public items
- All code must pass clippy with zero warnings
- Services should be 100% unit-testable (no HTTP dependencies)

## Developing a New Module

Chopin modules follow the **MVSR pattern** (Model-View-Service-Router).

See [docs/modular-architecture.md](docs/modular-architecture.md) for complete examples and guidelines:
- Module trait implementation
- Service layer (business logic)
- Handler layer (HTTP adapters)
- Test patterns (unit + integration)

## Testing

We maintain comprehensive test coverage with 310+ tests across 24 test files:

```bash
cargo test --all                         # Run all tests
cargo test -p chopin-core                # Core library only  
cargo test --test auth_tests             # Specific test file
```

## Adding a Feature

### Core Features (in chopin-core/)

1. Implement the feature following MVSR pattern (see [docs/modular-architecture.md](docs/modular-architecture.md))
2. Add feature flag to `Cargo.toml` if optional
3. Gate code with `#[cfg(feature = "...")]` if conditional
4. Add comprehensive tests  
5. Run `cargo clippy --all --all-targets -- -D warnings`
6. Run `cargo fmt --all`

### Vendor Modules (optional/separate crates)

If your feature is optional/vendor-specific (e.g., payment processor, analytics):

1. Create a separate crate: `chopin-stripe/`, `chopin-analytics/`
2. Implement `ChopinModule` trait
3. Document in the crate's README
4. List in main README's "Vendor Modules" section

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
