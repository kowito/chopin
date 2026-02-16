# 🎹 Chopin CLI

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-cli)](https://crates.io/crates/chopin-cli)
[![Downloads](https://img.shields.io/crates/d/chopin-cli.svg)](https://crates.io/crates/chopin-cli)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![GitHub stars](https://img.shields.io/github/stars/kowito/chopin.svg)](https://github.com/kowito/chopin)

**Scaffolding and code generation tool for the Chopin web framework.**

Generate ChopinModules following MVSR pattern (Model-View-Service-Router), manage migrations, and bootstrap new projects with sensible defaults.

## Installation

```bash
cargo install chopin-cli
```

## Quick Start

Create a new Chopin project with modular architecture:

```bash
chopin new my-app
cd my-app
cargo run
```

## Commands

### `new` — Create a new project

```bash
chopin new my-project [--template basic|api]
```

Creates a new Chopin project with:
- Modular architecture using ChopinModule trait
- MVSR pattern (Model-View-Service-Router)
- Configured `Cargo.toml` with dependencies
- Example auth module (signup, login)
- Development SQLite database
- OpenAPI documentation setup

Templates:
- `basic` — Minimal setup (default)
- `api` — Full CRUD API with posts module

### `generate module` — Scaffold a new module

```bash
chopin generate module blog
```

Generates MVSR structure:
```
src/modules/blog/
├── mod.rs           # ChopinModule implementation
├── services.rs      # Business logic (unit-testable)
├── handlers.rs      # HTTP handlers
├── models.rs        # SeaORM entities
└── migrations.rs    # Database migrations
```

### `generate` — Generate specific components

```bash
chopin generate service posts      # Service layer
chopin generate handler posts      # HTTP handler
chopin generate model post         # SeaORM entity
chopin generate migration create_posts_table
```

### `db` — Database management

```bash
chopin db migrate
chopin db reset
```

## Features

- ⚡ Zero-configuration project setup with MVSR pattern
- 📦 ChopinModule scaffolding
- 🔐 Built-in authentication scaffolding
- 🗄️ Database migration helpers
- 📚 OpenAPI documentation
- 🧪 Testing utilities included

## Documentation

For more information, see the [main repository](https://github.com/kowito/chopin):

- [**Modular Architecture Guide**](https://github.com/kowito/chopin/blob/main/docs/modular-architecture.md) — ChopinModule trait, MVSR pattern
- [Debugging & Logging](https://github.com/kowito/chopin/blob/main/docs/debugging-and-logging.md) — Enable request logging
- [Example Projects](https://github.com/kowito/chopin/tree/main/chopin-examples) — basic-api shows MVSR pattern
- [API Reference](https://docs.rs/chopin-core)

## License

WTFPL (Do What The Fuck You Want To Public License)
