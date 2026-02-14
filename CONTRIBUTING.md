# Contributing to Chopin

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
├── chopin-core/         # Framework library
├── chopin-cli/          # CLI tool
├── chopin-examples/     # Example applications
│   ├── hello-world/     # Minimal example
│   ├── basic-api/       # Full CRUD example
│   └── benchmark/       # Performance benchmark
└── website/             # Documentation website
```

## Development

### Build

```bash
cargo build                              # Debug build
cargo build --release                    # Release build
cargo build --release --features perf    # With mimalloc
```

### Test

```bash
cargo test                               # All tests
cargo test -p chopin-core                # Core library only
cargo test -p chopin-basic-api           # Example tests
```

### Run Examples

```bash
# Hello World
cargo run -p chopin-hello-world

# Basic API
cargo run -p chopin-basic-api

# Benchmark
REUSEPORT=true cargo run -p chopin-benchmark --release --features chopin-core/perf
```

## Code Style

- Use `rustfmt` for formatting
- Use `clippy` for linting: `cargo clippy --all-targets`
- Follow Rust naming conventions
- Add doc comments to all public items

## Adding a Feature

1. Add the feature flag to `chopin-core/Cargo.toml`
2. Gate the code with `#[cfg(feature = "...")]`
3. Update documentation in `website/`
4. Add tests
5. Update the LLM learning guide if significant

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
