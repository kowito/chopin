# chopin-orm

A simple, Django-like Object Relational Mapper for the `chopin2` web framework. It builds entirely on top of the extremely fast, synchronous thread-per-core `chopin-pg` PostgreSQL driver.

## Features
- **Declarative Models**: Define your schema simply by attaching `#[derive(Model)]` to your structs.
- **Active Record**: Easy `insert`, `update`, and `delete` methods straight on your struct instances.
- **Fluent Query Builder**: Construct complex `.filter(...)`, `.order_by(...)`, and `.limit(...)` clauses effortlessly.
- **Fast**: Translates directly into `chopin-pg` operations, preserving thread-per-core scalability.

## Basic Example

```rust
use chopin_orm::Model;
use chopin_pg::{PgConfig, PgPool};
use chopin_pg::types::ToParam;

// 1. Define your Model
#[derive(Model, Debug)]
#[model(table_name = "users")]
pub struct User {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub age: i32,
}

fn main() {
    // 2. Connect
    let config = PgConfig::from_url("postgres://chopin:password@localhost/postgres").unwrap();
    let mut pool = PgPool::connect(config, 4).unwrap();

    // 3. Active Record Insert
    let mut user = User {
        id: 0,
        name: "Alice".to_string(),
        age: 30,
    };
    user.insert(&mut pool).unwrap();
    println!("Inserted user with ID: {}", user.id); // primary key is populated!

    // 4. Update
    user.age = 31;
    user.update(&mut pool).unwrap();

    // 5. Query Builder Find
    let found = User::find()
        .filter("name = $1", vec!["Alice".to_param()])
        .one(&mut pool)
        .unwrap()
        .unwrap();

    println!("Found user: {:?}", found);

    // 6. Delete
    user.delete(&mut pool).unwrap();
}
```
