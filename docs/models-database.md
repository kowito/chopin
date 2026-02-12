# Models & Database (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Overview

Chopin uses **SeaORM 1.x** for database operations. It supports SQLite, PostgreSQL, and MySQL out of the box.

## Database Connection

Configure via the `DATABASE_URL` environment variable:

```env
# SQLite (default)
DATABASE_URL=sqlite://app.db?mode=rwc

# PostgreSQL
DATABASE_URL=postgres://user:pass@localhost:5432/mydb

# MySQL
DATABASE_URL=mysql://user:pass@localhost:3306/mydb
```

The connection pool is created at startup in `db.rs`:

```rust
// chopin-core handles this automatically
let db = chopin_core::db::connect(&config).await?;
```

## Defining a Model

A SeaORM model consists of an entity file with three parts:

### 1. Entity Definition

```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub body: String,
    pub published: bool,
    pub author_id: i32,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

### 2. Response DTO

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct PostResponse {
    pub id: i32,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub author_id: i32,
    pub created_at: String,
}

impl From<Model> for PostResponse {
    fn from(m: Model) -> Self {
        PostResponse {
            id: m.id,
            title: m.title,
            body: m.body,
            published: m.published,
            author_id: m.author_id,
            created_at: m.created_at.to_string(),
        }
    }
}
```

## Migrations

### Generate a Migration

```bash
chopin generate model post title:string body:text published:boolean author_id:integer
```

### Manual Migration

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(
            Table::create()
                .table(Posts::Table)
                .if_not_exists()
                .col(ColumnDef::new(Posts::Id).integer().not_null().auto_increment().primary_key())
                .col(ColumnDef::new(Posts::Title).string().not_null())
                .col(ColumnDef::new(Posts::Body).text().not_null())
                .col(ColumnDef::new(Posts::Published).boolean().not_null().default(false))
                .col(ColumnDef::new(Posts::AuthorId).integer().not_null())
                .col(ColumnDef::new(Posts::CreatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                .col(ColumnDef::new(Posts::UpdatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                .to_owned(),
        ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Posts::Table).to_owned()).await
    }
}

#[derive(Iden)]
enum Posts {
    Table,
    Id,
    Title,
    Body,
    Published,
    AuthorId,
    CreatedAt,
    UpdatedAt,
}
```

### Register Migrations

```rust
// src/migrations/mod.rs
use sea_orm_migration::prelude::*;

mod m20250211_000001_create_posts_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250211_000001_create_posts_table::Migration),
        ]
    }
}
```

### Run Migrations

```bash
# Via CLI
chopin db migrate

# Chopin also runs migrations automatically on startup
cargo run
```

## CRUD Operations

### Create

```rust
use sea_orm::{ActiveModelTrait, Set};

let post = posts::ActiveModel {
    title: Set(body.title.clone()),
    body: Set(body.body.clone()),
    published: Set(false),
    author_id: Set(user.user_id),
    ..Default::default()
};
let result = post.insert(&state.db).await?;
```

### Read (Find)

```rust
use sea_orm::EntityTrait;

// Find by ID
let post = posts::Entity::find_by_id(id)
    .one(&state.db)
    .await?
    .ok_or(ChopinError::NotFound("Post not found".into()))?;

// Find all
let posts = posts::Entity::find()
    .all(&state.db)
    .await?;

// With pagination
let posts = posts::Entity::find()
    .offset(pagination.offset())
    .limit(pagination.limit())
    .all(&state.db)
    .await?;

let total = posts::Entity::find().count(&state.db).await?;
```

### Update

```rust
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

let mut post: posts::ActiveModel = existing_post.into_active_model();
post.title = Set(new_title);
let updated = post.update(&state.db).await?;
```

### Delete

```rust
use sea_orm::{EntityTrait, ModelTrait};

let post = posts::Entity::find_by_id(id).one(&state.db).await?;
if let Some(post) = post {
    post.delete(&state.db).await?;
}
```

## Built-in User Model

Chopin provides a `User` model with roles:

```rust
// chopin_core::models::user
pub enum Role {
    User = 0,
    Moderator = 1,
    Admin = 2,
    SuperAdmin = 3,
}

pub struct Model {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}
```

The users table is auto-migrated on startup.

## Database CLI Commands

```bash
chopin db migrate           # Run pending migrations
chopin db rollback          # Rollback last migration
chopin db reset             # Drop and recreate all tables
chopin db seed              # Run seed data
chopin db status            # Show migration status
chopin createsuperuser      # Create an admin user interactively
```
