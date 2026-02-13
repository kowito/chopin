# 🎹 Chopin CLI

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

- [Getting Started Guide](https://github.com/kowito/chopin/blob/main/docs/getting-started.md)
- [CLI Cheatsheet](https://github.com/kowito/chopin/blob/main/docs/cli.md)
- [Example Projects](https://github.com/kowito/chopin/tree/main/chopin-examples)

## License

MIT
