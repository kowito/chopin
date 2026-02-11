# Models & Database Guide

Learn how to work with databases and define models in Chopin using SeaORM.

## Table of Contents

- [Quick Start](#quick-start)
- [Generating Models](#generating-models)
- [Defining Models](#defining-models)
- [Migrations](#migrations)
- [Queries](#queries)
- [Relationships](#relationships)
- [Advanced Topics](#advanced-topics)

## Quick Start

Generate a complete model with one command:

```bash
chopin generate model Post title:string body:text published:bool
```

This creates:
- Model entity (`src/models/post.rs`)
- Database migration (`src/migrations/m*_create_posts_table.rs`)
- CRUD controller (`src/controllers/post.rs`)

Register the model:

```rust
// src/models/mod.rs
pub mod post;
```

Run the server (migrations apply automatically):

```bash
cargo run
```

## Generating Models

### Basic Syntax

```bash
chopin generate model <ModelName> <field>:<type> [field:type...]
```

### Field Types

| Type | Rust Type | Database Type | Example |
|------|-----------|---------------|---------|
| `string`, `str` | `String` | VARCHAR | `name:string` |
| `text` | `String` | TEXT | `body:text` |
| `int`, `i32` | `i32` | INTEGER | `count:i32` |
| `i64`, `bigint` | `i64` | BIGINT | `user_id:i64` |
| `f32`, `float` | `f32` | FLOAT | `rating:f32` |
| `f64`, `double` | `f64` | DOUBLE | `price:f64` |
| `bool`, `boolean` | `bool` | BOOLEAN | `active:bool` |
| `datetime`, `timestamp` | `NaiveDateTime` | TIMESTAMP | `published_at:datetime` |
| `uuid` | `Uuid` | UUID | `token:uuid` |

### Examples

**Blog post**:
```bash
chopin generate model Post \
  title:string \
  slug:string \
  body:text \
  published:bool
```

**Product catalog**:
```bash
chopin generate model Product \
  name:string \
  description:text \
  price:f64 \
  stock:i32 \
  available:bool
```

**User profile**:
```bash
chopin generate model Profile \
  user_id:i32 \
  bio:text \
  avatar_url:string \
  birth_date:datetime
```

### Auto-Generated Fields

Every model automatically includes:
- `id` — Primary key (auto-increment)
- `created_at` — Timestamp of creation
- `updated_at` — Timestamp of last update

## Defining Models

### Basic Model

Generated models look like this:

```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Post entity.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    
    pub title: String,
    pub body: String,
    pub published: bool,
    
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

### Custom Primary Key

Override default ID:

```rust
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub uuid: Uuid,
    
    // Other fields...
}
```

### Nullable Fields

Use `Option<T>`:

```rust
pub struct Model {
    pub id: i32,
    pub title: String,
    pub subtitle: Option<String>,  // Can be NULL
    pub published_at: Option<NaiveDateTime>,
}
```

### Enums

Define custom enums:

```rust
#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(Some(20))")]
pub enum PostStatus {
    #[sea_orm(string_value = "draft")]
    Draft,
    #[sea_orm(string_value = "published")]
    Published,
    #[sea_orm(string_value = "archived")]
    Archived,
}

pub struct Model {
    pub id: i32,
    pub status: PostStatus,
}
```

### Indexes

Add indexes for performance:

```rust
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    
    #[sea_orm(indexed)]
    pub slug: String,
    
    #[sea_orm(indexed)]
    pub author_id: i32,
}
```

## Migrations

### Auto-Generated Migrations

When you generate a model, a migration is created:

```rust
// src/migrations/m20260211_143022_create_posts_table.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Posts::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Posts::Title).string().not_null())
                    .col(ColumnDef::new(Posts::Body).text().not_null())
                    .col(ColumnDef::new(Posts::Published).boolean().not_null())
                    .col(ColumnDef::new(Posts::CreatedAt).timestamp().not_null())
                    .col(ColumnDef::new(Posts::UpdatedAt).timestamp().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Posts::Table).to_owned())
            .await
    }
}
```

### Running Migrations

Migrations run automatically on server startup:

```bash
cargo run
# → Running pending database migrations...
# → Migration m20260211_143022_create_posts_table applied
```

Manual migration:
```bash
chopin db migrate
```

### Creating Custom Migrations

Create a new migration file manually:

```rust
// src/migrations/m20260212_add_views_to_posts.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .add_column(
                        ColumnDef::new(Posts::ViewCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .drop_column(Posts::ViewCount)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Posts {
    Table,
    ViewCount,
}
```

Register in `src/migrations/mod.rs`:

```rust
pub use m20260212_add_views_to_posts::Migration as AddViewsToPosts;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260211_143022_create_posts_table::Migration),
            Box::new(AddViewsToPosts),
        ]
    }
}
```

## Queries

### Find All

```rust
use crate::models::post::{Entity as Post, Model as PostModel};

let posts: Vec<PostModel> = Post::find()
    .all(&db)
    .await?;
```

### Find by ID

```rust
let post: Option<PostModel> = Post::find_by_id(42)
    .one(&db)
    .await?;

// Or with error handling
let post = Post::find_by_id(42)
    .one(&db)
    .await?
    .ok_or_else(|| ChopinError::NotFound("Post not found".to_string()))?;
```

### Filter

```rust
use crate::models::post::Column as PostColumn;

// Single condition
let published_posts = Post::find()
    .filter(PostColumn::Published.eq(true))
    .all(&db)
    .await?;

// Multiple conditions (AND)
let posts = Post::find()
    .filter(PostColumn::Published.eq(true))
    .filter(PostColumn::AuthorId.eq(user_id))
    .all(&db)
    .await?;

// OR conditions
use sea_orm::Condition;

let posts = Post::find()
    .filter(
        Condition::any()
            .add(PostColumn::AuthorId.eq(user_id))
            .add(PostColumn::Published.eq(true))
    )
    .all(&db)
    .await?;
```

### Order By

```rust
// Ascending
let posts = Post::find()
    .order_by_asc(PostColumn::CreatedAt)
    .all(&db)
    .await?;

// Descending
let posts = Post::find()
    .order_by_desc(PostColumn::CreatedAt)
    .all(&db)
    .await?;
```

### Pagination

```rust
let posts = Post::find()
    .limit(20)
    .offset(40)
    .all(&db)
    .await?;
```

With Chopin's `Pagination` extractor:

```rust
use chopin_core::extractors::Pagination;

async fn list(
    State(state): State<AppState>,
    pagination: Pagination,
) -> Result<ApiResponse<Vec<PostResponse>>, ChopinError> {
    let p = pagination.clamped(); // Max 100 per page
    
    let posts = Post::find()
        .limit(p.limit)
        .offset(p.offset)
        .all(&state.db)
        .await?;
    
    Ok(ApiResponse::success(posts))
}
```

### Count

```rust
let count: u64 = Post::find()
    .filter(PostColumn::Published.eq(true))
    .count(&db)
    .await?;
```

### Insert

```rust
use sea_orm::{ActiveModelTrait, Set};
use chrono::Utc;

let now = Utc::now().naive_utc();

let new_post = post::ActiveModel {
    title: Set("Hello World".to_string()),
    body: Set("My first post".to_string()),
    published: Set(false),
    created_at: Set(now),
    updated_at: Set(now),
    ..Default::default()
};

let post: PostModel = new_post.insert(&db).await?;
```

### Update

```rust
// Find and update
let mut post: post::ActiveModel = Post::find_by_id(42)
    .one(&db)
    .await?
    .ok_or_else(|| ChopinError::NotFound("Post not found".to_string()))?
    .into();

post.title = Set("Updated Title".to_string());
post.updated_at = Set(Utc::now().naive_utc());

let updated_post: PostModel = post.update(&db).await?;
```

### Delete

```rust
// Find and delete
let post = Post::find_by_id(42)
    .one(&db)
    .await?
    .ok_or_else(|| ChopinError::NotFound("Post not found".to_string()))?;

post.delete(&db).await?;

// Bulk delete
Post::delete_many()
    .filter(PostColumn::Published.eq(false))
    .exec(&db)
    .await?;
```

## Relationships

### One-to-Many

**Post has many Comments**:

```rust
// models/post.rs
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::comment::Entity")]
    Comments,
}

impl Related<super::comment::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Comments.def()
    }
}

// models/comment.rs
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::post::Entity",
        from = "Column::PostId",
        to = "super::post::Column::Id"
    )]
    Post,
}

impl Related<super::post::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Post.def()
    }
}
```

**Query with relations**:

```rust
use sea_orm::prelude::*;

// Find post with comments
let post_with_comments = Post::find_by_id(42)
    .find_with_related(Comment)
    .all(&db)
    .await?;

// Eager loading
let posts = Post::find()
    .find_with_related(Comment)
    .all(&db)
    .await?;
```

### Many-to-Many

**Post has many Tags through PostTag**:

```rust
// models/post_tag.rs (junction table)
#[derive(DeriveEntityModel)]
#[sea_orm(table_name = "post_tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub post_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::post::Entity",
        from = "Column::PostId",
        to = "super::post::Column::Id"
    )]
    Post,
    #[sea_orm(
        belongs_to = "super::tag::Entity",
        from = "Column::TagId",
        to = "super::tag::Column::Id"
    )]
    Tag,
}
```

**Query**:

```rust
let post = Post::find_by_id(42)
    .one(&db)
    .await?
    .unwrap();

let tags: Vec<tag::Model> = post
    .find_related(Tag)
    .all(&db)
    .await?;
```

## Advanced Topics

### Transactions

```rust
use sea_orm::TransactionTrait;

let txn = db.begin().await?;

// Multiple operations
let post = new_post.insert(&txn).await?;
let comment = new_comment.insert(&txn).await?;

// Commit or rollback
txn.commit().await?;
// Or: txn.rollback().await?;
```

### Raw SQL

```rust
use sea_orm::Statement;

let posts: Vec<PostModel> = Post::find()
    .from_raw_sql(Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        r#"SELECT * FROM posts WHERE published = $1"#,
        vec![true.into()],
    ))
    .all(&db)
    .await?;
```

### JSON Fields

Store JSON data:

```rust
use serde_json::Value;

pub struct Model {
    pub id: i32,
    pub metadata: Value,  // JSON column
}

// Insert
let post = post::ActiveModel {
    metadata: Set(serde_json::json!({
        "tags": ["rust", "web"],
        "featured": true
    })),
    ..Default::default()
};
```

### Timestamps with Hooks

Auto-update timestamps:

```rust
use sea_orm::ActiveModelBehavior;

impl ActiveModelBehavior for ActiveModel {
    fn before_save(mut self, insert: bool) -> Result<Self, DbErr> {
        let now = Utc::now().naive_utc();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
```

### Soft Deletes

Mark records as deleted instead of removing:

```rust
pub struct Model {
    pub id: i32,
    pub deleted_at: Option<NaiveDateTime>,
}

// Soft delete
let mut post: post::ActiveModel = post.into();
post.deleted_at = Set(Some(Utc::now().naive_utc()));
post.update(&db).await?;

// Query only non-deleted
let posts = Post::find()
    .filter(PostColumn::DeletedAt.is_null())
    .all(&db)
    .await?;
```

## Best Practices

✅ **DO**:
- Use the CLI to generate models consistently
- Add indexes for frequently queried fields
- Use transactions for multi-step operations
- Define relationships for related data
- Use enums for fixed sets of values
- Validate data before inserting

❌ **DON'T**:
- Edit generated migration files after applying
- Store sensitive data unencrypted
- Use `SELECT *` in production (specify columns)
- Forget to handle `None` from `find_by_id`
- Ignore database connection pool limits

## Troubleshooting

### Migration Fails

**Error**: Migration already applied

**Solution**: Migrations are idempotent. If you need to change a table, create a new migration.

### Connection Pool Exhausted

**Error**: Timeout acquiring connection

**Solutions**:
- Increase `max_connections` in `DATABASE_URL`
- Ensure connections are released (use `await?`)
- Check for connection leaks

### Type Mismatch

**Error**: Column type doesn't match Rust type

**Solution**: Ensure model field types match database column types. Regenerate model or create migration to alter column.

---

## Resources

- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [SeaORM Cookbook](https://www.sea-ql.org/sea-orm-cookbook/)
- [Migration Reference](https://www.sea-ql.org/sea-orm-cookbook/migrations/01-migrations.html)
- [CLI Reference](cli.md)

Chopin makes database operations type-safe, performant, and enjoyable!
