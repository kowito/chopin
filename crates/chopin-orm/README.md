# chopin-orm

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-orm)](https://crates.io/crates/chopin-orm)
[![Downloads](https://img.shields.io/crates/d/chopin-orm.svg)](https://crates.io/crates/chopin-orm)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-orm` provides a high‑performance ORM layer built on top of `chopin-pg`. It offers zero‑allocation query construction and type‑safe mapping of Rust structs to PostgreSQL tables.

## 🛠️ Usage Example

```rust
use chopin_orm::{Model, PgPool, PgValue};

#[derive(Model, Debug)]
#[model(table_name = "users")]
struct User {
    #[model(primary_key)]
    id: i32,
    username: String,
    email: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;
    let mut pool = PgPool::connect(config, 5)?;

    // Create
    let mut user = User { id: 0, username: "virtuoso".into(), email: "paganini@example.com".into() };
    user.insert(&mut pool)?;

    // Retrieve
    let found = User::find()
        .filter("id = $1", vec![PgValue::Int4(user.id)])
        .one(&mut pool)?;

    // Update
    if let Some(mut u) = found {
        u.username = "chopin".into();
        u.update(&mut pool)?;
    }

    Ok(())
}
```
