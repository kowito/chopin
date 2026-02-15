# 🎹 Chopin CLI

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-cli)](https://crates.io/crates/chopin-cli)
[![Downloads](https://img.shields.io/crates/d/chopin-cli.svg)](https://crates.io/crates/chopin-cli)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![GitHub stars](https://img.shields.io/github/stars/kowito/chopin.svg)](https://github.com/kowito/chopin)

**Scaffolding and code generation tool for the Chopin web framework.**

The Chopin CLI helps you quickly bootstrap new Chopin projects with sensible defaults, generate boilerplate code, and manage database migrations.

## Installation

```bash
cargo install chopin-cli
```

## Quick Start

Create a new Chopin project:

```bash
chopin new my-app
cd my-app
cargo run
```

## Commands

### `new` — Create a new project

```bash
chopin new my-project
```

Creates a new Chopin project with:
- Configured `Cargo.toml` with all dependencies
- Basic project structure (controllers, models, migrations)
- Example auth endpoints (signup, login)
- Development SQLite database
- OpenAPI documentation setup

### `generate` — Generate boilerplate code

```bash
chopin generate controller users
chopin generate model user
chopin generate migration create_users_table
```

### `db` — Database management

```bash
chopin db migrate
chopin db reset
```

## Features

- ⚡ Zero-configuration project setup
- 📦 Workspace-ready structure
- 🔐 Built-in authentication scaffolding
- 🗄️ Database migration helpers
- 📚 OpenAPI documentation
- 🧪 Testing utilities included

## Documentation

For more information, see the [main repository](https://github.com/kowito/chopin):

- [Debugging & Logging Guide](https://github.com/kowito/chopin/blob/main/docs/debugging-and-logging.md) — Enable request logging (essential for development!)
- [Example Projects](https://github.com/kowito/chopin/tree/main/chopin-examples)
- [API Reference](https://docs.rs/chopin-core)

## License

WTFPL (Do What The Fuck You Want To Public License)
