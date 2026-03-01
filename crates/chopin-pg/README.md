# chopin-pg

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-pg)](https://crates.io/crates/chopin-pg)
[![Downloads](https://img.shields.io/crates/d/chopin-pg.svg)](https://crates.io/crates/chopin-pg)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-pg` provides the high‑performance PostgreSQL driver used by the Chopin suite. It offers zero‑allocation query handling and per‑core connection pools.

## 🛠️ Usage Example

```rust
use chopin_pg::{PgConfig, PgConnection, PgValue};

fn main() -> PgResult<()> {
    let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;
    let mut conn = PgConnection::connect(&config)?;

    // Execute simple query
    let rows = conn.query_simple("SELECT current_database()")?;
    println!("Database: {}", rows[0].get(0)?);

    // Prepared statement (Extended Query Protocol)
    let rows = conn.query(
        "SELECT username FROM users WHERE id = $1",
        &[&42i32]
    )?;

    if let Some(row) = rows.first() {
        let name: String = row.get(0)?;
        println!("User: {}", name);
    }

    Ok(())
}
```
