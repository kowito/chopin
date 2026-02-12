# Chopin Documentation

> The high-performance Rust web framework for perfectionists with deadlines.

## Table of Contents

### Getting Started

- [Getting Started](getting-started.md) — Install, create a project, run your first server
- [Architecture](architecture.md) — How Chopin is structured internally
- [Configuration](configuration.md) — Environment variables and server modes

### Core Concepts

- [Controllers & Routing](controllers-routing.md) — Define endpoints and handle requests
- [Models & Database](models-database.md) — SeaORM entities, migrations, queries
- [API Responses](api.md) — Consistent JSON responses and error handling

### Security

- [Security](security.md) — JWT authentication, Argon2id passwords
- [Roles & Permissions](roles-permissions.md) — Role-based access control

### Features

- [Caching](caching.md) — In-memory and Redis caching
- [File Uploads](file-uploads.md) — Local filesystem and S3-compatible storage
- [GraphQL](graphql.md) — Optional async-graphql integration
- [Testing](testing.md) — Integration testing utilities

### Performance

- [Performance](performance.md) — Server modes, mimalloc, SO_REUSEPORT, benchmarks
- [Deployment](deployment.md) — Production deployment guide

### Tooling

- [CLI](cli.md) — The `chopin` command-line tool
- [CLI Cheatsheet](cli-cheatsheet.md) — Quick reference for all CLI commands

### AI / LLM

- [LLM Learning Guide](llm-learning-guide.md) — Complete framework reference for AI assistants

---

## Quick Start

```bash
# Install the CLI
cargo install chopin-cli

# Create a new project
chopin new my-app
cd my-app

# Run in development mode
cargo run

# Run in performance mode
SERVER_MODE=performance cargo run --release --features perf
```

Your API is live at `http://127.0.0.1:3000` with interactive docs at `/api-docs`.
