# chopin-orm

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-orm)](https://crates.io/crates/chopin-orm)
[![Downloads](https://img.shields.io/crates/d/chopin-orm.svg)](https://crates.io/crates/chopin-orm)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-orm` is a high‑performance, type‑safe ORM built on top of `chopin-pg`. It provides derive-macro-driven model mapping, a fluent query DSL with type-safe column expressions, relationships, auto-migration, pagination, and ActiveModel partial updates.

## Features

- **Derive macro** — `#[derive(Model)]` generates `FromRow`, column enum, and full CRUD methods
- **Type-safe column DSL** — `UserColumn::name.eq("Alice")` instead of raw strings
- **Relationships** — `has_many` / `belongs_to` with lazy loading and JOIN support
- **Auto-migration** — `sync_schema()` diffs and migrates table columns automatically
- **Pagination** — `.paginate(page_size).page(n).fetch()` returns `Page<M>` with total counts
- **ActiveModel** — partial updates tracking only changed fields
- **Validation** — `Validate` trait with default pass-through; implement custom rules
- **Upsert** — INSERT ... ON CONFLICT UPDATE for idempotent writes
- **Aggregations** — `.count()`, `ColumnTrait::sum()`, `.max()`, `.min()` with GROUP BY / HAVING
- **Mock executor** — `MockExecutor` + `mock_row!` for unit testing without a database
- **Logged executor** — `LoggedExecutor` wraps any executor for SQL tracing
- **Migration system** — `MigrationManager` with `up`/`down` for production schema management

## 🛠️ Quick Start

```rust
use chopin_orm::{Model, PgPool, Validate, builder::ColumnTrait};
use chopin_pg::PgConfig;

#[derive(Model, Debug, Clone)]
#[model(table_name = "users")]
struct User {
    #[model(primary_key)]
    id: i32,
    name: String,
    email: String,
    age: Option<i32>,
}

impl Validate for User {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.email.is_empty() {
            errors.push("Email cannot be empty".to_string());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;
    let mut pool = PgPool::connect(config, 5)?;

    // Auto-migrate table schema
    User::sync_schema(&mut pool)?;

    // Insert (id auto-populated from RETURNING)
    let mut user = User { id: 0, name: "Alice".into(), email: "alice@example.com".into(), age: Some(30) };
    user.insert(&mut pool)?;
    println!("Inserted user id: {}", user.id);

    // Type-safe query with column DSL
    use UserColumn::*;
    let users = User::find()
        .filter(name.eq("Alice"))
        .filter(age.gt(25))
        .all(&mut pool)?;

    // Partial update (only specified columns)
    let mut u = users[0].clone();
    u.name = "Alice Updated".into();
    u.update_columns(&mut pool, &["name"])?;

    // Count
    let count = User::find().count(&mut pool)?;
    println!("Total users: {}", count);

    // Delete
    u.delete(&mut pool)?;

    Ok(())
}
```

## 🔗 Relationships

```rust
#[derive(Model, Debug, Clone)]
#[model(table_name = "users", has_many(Post, fk = "user_id"))]
struct User {
    #[model(primary_key)]
    id: i32,
    name: String,
}

#[derive(Model, Debug, Clone)]
#[model(table_name = "posts")]
struct Post {
    #[model(primary_key)]
    id: i32,
    title: String,
    #[model(belongs_to(User))]
    user_id: i32,
}

impl Validate for User {}
impl Validate for Post {}

// Lazy loading
let posts = user.fetch_posts(&mut pool)?;        // has_many
let author = post.fetch_user_id(&mut pool)?;      // belongs_to

// JOIN queries
let users_with_posts = User::find()
    .join_child::<Post>()
    .all(&mut pool)?;
```

## 📦 ActiveModel (Partial Updates)

```rust
use chopin_orm::ActiveModel;

let user = User::find().filter(UserColumn::id.eq(1)).one(&mut pool)?.unwrap();
let mut active = ActiveModel::from_model(user);
active.set("name", "New Name");
active.save(&mut pool)?;  // UPDATE users SET name = $1 WHERE id = $2
```

## 📄 Pagination

```rust
let page = User::find()
    .filter(UserColumn::age.gt(18))
    .order_by("name ASC")
    .paginate(20)        // 20 items per page
    .page(1)             // page 1
    .fetch(&mut pool)?;

println!("Page {}/{}, {} items", page.page, page.total_pages, page.items.len());
```

## 🧪 Testing with MockExecutor

```rust
use chopin_orm::MockExecutor;
use chopin_pg::Row;

let mut mock = MockExecutor::new();
mock.push_result(vec![
    mock_row!("id" => 1, "name" => "Alice", "email" => "a@b.com", "age" => 30),
]);

let users = User::find().all(&mut mock)?;
assert_eq!(users.len(), 1);
assert_eq!(mock.executed_queries[0].0, "SELECT id, name, email, age FROM users");
```
