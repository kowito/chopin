# Chopin Usage Guide

A practical guide to using the Chopin HTTP framework and its companion crates.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Routing](#routing)
3. [Request & Extractors](#request--extractors)
4. [Response](#response)
5. [Middleware](#middleware)
6. [ORM (`chopin-orm`)](#orm-chopin-orm)
7. [Authentication (`chopin-auth`)](#authentication-chopin-auth)
8. [Multipart / File Uploads](#multipart--file-uploads)

---

## Getting Started

Add the crates to your `Cargo.toml`:

```toml
[dependencies]
chopin-core = "0.5.21"
chopin-orm  = "0.5.21"   # optional — PostgreSQL ORM
chopin-auth = "0.5.21"   # optional — JWT auth
```

### Minimal server

```rust
use chopin_core::{get, Context, Response, Chopin};

#[get("/")]
fn index(_ctx: Context) -> Response {
    Response::text("Hello, world!")
}

fn main() {
    Chopin::new()
        .mount_all_routes()
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

`mount_all_routes()` collects every handler annotated with a route macro and registers them automatically. Call `serve()` to start the multi-threaded server.

### Manual router (no macros)

```rust
use chopin_core::{Context, Response, Router, Server};

fn hello(_ctx: Context) -> Response {
    Response::text("Hello!")
}

fn main() {
    let mut router = Router::new();
    router.get("/", hello);

    Server::bind("0.0.0.0:8080")
        .workers(4)   // defaults to number of logical CPUs
        .serve(router)
        .unwrap();
}
```

---

## Routing

### Route macros

Annotate any `fn(Context) -> Response` with a method macro and provide the path as a string literal.

```rust
use chopin_core::{get, post, put, delete, patch, Context, Response};

#[get("/users")]
fn list_users(_ctx: Context) -> Response { /* ... */ }

#[post("/users")]
fn create_user(ctx: Context) -> Response { /* ... */ }

#[get("/users/:id")]
fn get_user(ctx: Context) -> Response { /* ... */ }

#[put("/users/:id")]
fn update_user(ctx: Context) -> Response { /* ... */ }

#[delete("/users/:id")]
fn delete_user(ctx: Context) -> Response { /* ... */ }

#[patch("/users/:id")]
fn patch_user(ctx: Context) -> Response { /* ... */ }
```

Supported macros: `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`, `#[head]`, `#[options]`, `#[trace]`, `#[connect]`.

### Path parameters

Named segments beginning with `:` are captured as parameters.

```rust
#[get("/posts/:year/:slug")]
fn show_post(ctx: Context) -> Response {
    let year = ctx.param("year").unwrap_or("unknown");
    let slug = ctx.param("slug").unwrap_or("unknown");
    Response::text(format!("{}/{}", year, slug))
}
```

### Wildcard routes

A segment starting with `*` matches the rest of the path.

```rust
router.get("/static/*path", serve_static);
```

### Manual route registration

```rust
router.add(Method::Get, "/ping", ping_handler);
// or with the shorthand methods:
router.get("/ping", ping_handler);
router.post("/items", create_item);
```

---

## Request & Extractors

### `Context`

Every handler receives a `Context` which wraps the parsed request.

```rust
pub struct Context<'a> {
    pub req: Request<'a>,   // raw request data
    // ...
}
```

#### Reading headers

```rust
fn handler(ctx: Context) -> Response {
    if let Some(ct) = ctx.header("content-type") {
        // use ct
    }
    Response::text("ok")
}
```

#### Reading path parameters

```rust
let id = ctx.param("id").unwrap_or("0");
```

#### Reading the raw body

```rust
let raw: &[u8] = ctx.req.body;
let text = std::str::from_utf8(raw).unwrap_or("");
```

### JSON body extractor

Use `ctx.extract::<Json<T>>()` where `T: serde::Deserialize`. Returns `400 Bad Request` automatically on malformed JSON.

```rust
use chopin_core::{Context, Response};
use serde::Deserialize;
use chopin_core::extract::Json;

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

fn create_user(ctx: Context) -> Response {
    let Json(body) = match ctx.extract::<Json<CreateUser>>() {
        Ok(j) => j,
        Err(res) => return res,   // 400 Bad Request
    };
    Response::text(format!("Created: {}", body.name))
}
```

### Query string extractor

Use `ctx.extract::<Query<T>>()` where `T: serde::Deserialize`.

```rust
use serde::Deserialize;
use chopin_core::extract::Query;

#[derive(Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
}

fn list_items(ctx: Context) -> Response {
    let Query(q) = match ctx.extract::<Query<Pagination>>() {
        Ok(q) => q,
        Err(res) => return res,
    };
    let page = q.page.unwrap_or(1);
    Response::text(format!("page={}", page))
}
```

---

## Response

### Common constructors

| Method | Status | Content-Type |
|---|---|---|
| `Response::text(body)` | 200 | `text/plain` |
| `Response::text_static(b"...")` | 200 | `text/plain` (zero-copy) |
| `Response::json(&value)` | 200 | `application/json` |
| `Response::json_bytes(bytes)` | 200 | `application/json` |
| `Response::file("path/to/file")` | 200 | inferred from extension |
| `Response::stream(iter)` | 200 | `application/octet-stream` |
| `Response::not_found()` | 404 | `text/plain` |
| `Response::bad_request()` | 400 | `text/plain` |
| `Response::unauthorized()` | 401 | `text/plain` |
| `Response::forbidden()` | 403 | `text/plain` |
| `Response::server_error()` | 500 | `text/plain` |
| `Response::new(status)` | any | `text/plain`, empty body |

### JSON responses

```rust
use kowito_json::KJson;

#[derive(KJson)]
struct User {
    id: i32,
    name: String,
}

fn get_user(_ctx: Context) -> Response {
    let user = User { id: 1, name: "Alice".into() };
    Response::json(&user)
}
```

> `KJson` is Chopin's schema-JIT serializer. It is faster than `serde_json` for outgoing responses.

### Custom status code

```rust
let mut res = Response::json(&new_item);
res.status = 201; // Created
res
```

### Custom headers

`with_header` is a builder method — chain as many as needed.

```rust
Response::json(&data)
    .with_header("X-Request-Id", "abc-123")
    .with_header("Cache-Control", "no-store")
```

The value accepts `&'static str`, `String`, or any integer type.

### Static file serving

Content-Type is inferred from the file extension. Returns `404` if the file cannot be opened.

```rust
fn serve_index(_ctx: Context) -> Response {
    Response::file("public/index.html")
}
```

Supported extensions include: `html`, `css`, `js`, `json`, `png`, `jpg`, `gif`, `webp`, `svg`, `woff2`, `mp4`, `wasm`, `pdf`, and more.

### Streaming response

```rust
fn big_stream(_ctx: Context) -> Response {
    let chunks = (0..100u32).map(|i| format!("chunk-{}\n", i).into_bytes());
    Response::stream(chunks)
}
```

### Zero-copy file range (sendfile)

For advanced use cases (e.g. `Range` header support):

```rust
use std::os::unix::io::IntoRawFd;

let file = std::fs::File::open("video.mp4").unwrap();
let fd = file.into_raw_fd();
let len = /* file size */;
Response::sendfile(fd, 0, len, "video/mp4")
```

### `IntoResponse` trait

Handlers may return any type that implements `IntoResponse`. `String`, `&'static str`, and `Result<T, E>` (where both `T` and `E` implement `IntoResponse`) are implemented out of the box.

```rust
fn maybe_ok(ctx: Context) -> Result<Response, Response> {
    if ctx.param("id").is_some() {
        Ok(Response::text("found"))
    } else {
        Err(Response::not_found())
    }
}
```

---

## Middleware

Middleware has the signature `fn(Context, BoxedHandler) -> Response`.

```rust
use chopin_core::{Context, Response, router::BoxedHandler};

fn logging(ctx: Context, next: BoxedHandler) -> Response {
    let path = ctx.req.path.to_string();
    let res = next(ctx);
    println!("{} -> {}", path, res.status);
    res
}
```

### Global middleware

Applied to every route on the router:

```rust
router.layer(logging);
```

### Route-scoped middleware

Apply to a subtree of routes:

```rust
router.use_middleware("/admin", require_admin);
```

### Middleware with the macro router

Middleware must be registered on the `Router` before or after `mount_all_routes()`:

```rust
Chopin::new()
    .mount_all_routes()  // registers macro-annotated routes
    // Use Server directly for middleware with the macro workflow:
    // ...
```

For the imperative `Router` API:

```rust
let mut router = Router::new();
router.layer(logging);
router.get("/users", list_users);
```

---

## ORM (`chopin-orm`)

`chopin-orm` provides a type-safe ORM layer on top of the `chopin-pg` synchronous PostgreSQL driver, with derive-macro-driven models, a fluent query DSL, relationships, and auto-migration.

**Cargo.toml:**
```toml
[dependencies]
chopin-orm = "0.5.21"
chopin-pg  = "0.5.21"
```

### Defining a model

Derive `Model` on a struct and implement `Validate`. The first `i32`/`i64` field marked `#[model(primary_key)]` is auto-generated (serial). Override the table name with `#[model(table_name = "...")]`.

```rust
use chopin_orm::{Model, Validate, builder::ColumnTrait};

#[derive(Model, Debug, Clone)]
#[model(table_name = "users")]
struct User {
    #[model(primary_key)]
    id: i32,
    name: String,
    email: String,
    active: bool,
}

impl Validate for User {}  // default: always passes
```

The `#[derive(Model)]` macro generates:
- `impl Model for User` — full CRUD (`insert`, `update`, `delete`, `upsert`, `update_columns`)
- `impl FromRow for User` — automatic row-to-struct mapping
- `enum UserColumn` — type-safe column identifiers with `impl ColumnTrait<User>`
- `User::find()` — returns a `QueryBuilder<User>` for fluent queries
- `User::sync_schema()` — automatic table creation and column migration
- `User::create_table_stmt()` — raw DDL generation

### Connecting to PostgreSQL

```rust
use chopin_pg::{PgConfig, PgPool};

let config = PgConfig::new("localhost", 5432, "myuser", "mypassword", "mydb");
let mut pool = PgPool::connect(config, 10)?; // 10 connections, eager pre-connect
```

### CRUD operations

All operations accept any `Executor` — a `PgPool`, `Transaction`, or `MockExecutor`.

#### Insert

```rust
let mut user = User {
    id: 0,  // auto-populated from RETURNING
    name: "Alice".into(),
    email: "alice@example.com".into(),
    active: true,
};

user.insert(&mut pool)?;
println!("New user id: {}", user.id);
```

#### Update

```rust
user.name = "Alice Smith".into();
user.update(&mut pool)?;
```

#### Partial Update (specific columns only)

```rust
user.name = "Alice Updated".into();
user.update_columns(&mut pool, &["name"])?;
```

#### Delete

```rust
user.delete(&mut pool)?;
```

#### Upsert (insert or update on conflict)

```rust
user.upsert(&mut pool)?;
```

### Type-Safe Query DSL

The preferred way to query is with the generated `UserColumn` enum and `ColumnTrait`:

```rust
use chopin_orm::builder::ColumnTrait;
use UserColumn::*;

// Fetch with type-safe filters
let active_users = User::find()
    .filter(active.eq(true))
    .filter(name.like("Ali%"))
    .order_by("name ASC")
    .limit(20)
    .all(&mut pool)?;

// Fetch a single record
let user = User::find()
    .filter(email.eq("alice@example.com"))
    .one(&mut pool)?;

// Count rows
let count = User::find()
    .filter(active.eq(true))
    .count(&mut pool)?;

// Pagination
let page = User::find()
    .filter(active.eq(true))
    .order_by("name ASC")
    .paginate(20)
    .page(1)
    .fetch(&mut pool)?;
println!("Page {}/{}, {} items", page.page, page.total_pages, page.items.len());
```

### Raw queries

Use `Executor::execute` and `Executor::query` for SQL that doesn't map to a model.

```rust
use chopin_orm::Executor;

// Execute (INSERT / UPDATE / DELETE)
pool.execute(
    "UPDATE users SET active = $1 WHERE id = $2",
    &[&false, &42i32],
)?;

// Query rows
let rows = pool.query("SELECT id, name FROM users WHERE active = $1", &[&true])?;
for row in &rows {
    let id: i32 = row.get_typed(0)?;
    let name: String = row.get_typed(1)?;
    println!("{}: {}", id, name);
}
```

### Transactions

```rust
use chopin_orm::Transaction;

let mut conn = pool.get()?;
let mut tx = Transaction::begin(&mut conn)?;

let mut user = User { id: 0, name: "Bob".into(), email: "bob@example.com".into(), active: true };
user.insert(&mut tx)?;

tx.commit()?;  // or tx.rollback()?;
```

### Relationships

```rust
#[derive(Model, Debug, Clone)]
#[model(table_name = "users", has_many(Post, fk = "user_id"))]
struct User {
    #[model(primary_key)]
    id: i32,
    name: String,
}
impl Validate for User {}

#[derive(Model, Debug, Clone)]
#[model(table_name = "posts")]
struct Post {
    #[model(primary_key)]
    id: i32,
    title: String,
    #[model(belongs_to(User))]
    user_id: i32,
}
impl Validate for Post {}

// Lazy loading
let posts = user.fetch_posts(&mut pool)?;
let author = post.fetch_user_id(&mut pool)?;

// JOIN queries
let users = User::find().join_child::<Post>().all(&mut pool)?;
```

### Supported Rust → PostgreSQL type mappings

| Rust type | PostgreSQL wire type |
|---|---|
| `i16` | `SMALLINT` |
| `i32` | `INTEGER` |
| `i64` | `BIGINT` |
| `f32` | `REAL` |
| `f64` | `DOUBLE PRECISION` |
| `bool` | `BOOLEAN` |
| `String` | `TEXT`, `VARCHAR` |
| `Vec<u8>` | `BYTEA` |
| `Option<T>` | nullable version of the inner type |
| `Vec<T>` | `ARRAY` (scalar types) |
| `IpAddr` | `INET` |

---

## Authentication (`chopin-auth`)

`chopin-auth` provides JWT signing/verification and a request extractor for Bearer tokens.

**Cargo.toml:**
```toml
[dependencies]
chopin-auth = "0.5.21"
```

### Setup — `JwtManager`

Initialize the manager once per worker thread (thread-local storage is used internally):

```rust
use chopin_auth::{JwtManager, extractor::init_jwt_manager};

// In your server startup / worker init:
let manager = JwtManager::new(b"my-super-secret-key");
init_jwt_manager(manager);
```

### Signing a token

```rust
use serde::{Serialize, Deserialize};
use chopin_auth::JwtManager;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    role: String,
}

let manager = JwtManager::new(b"secret");
let claims = Claims {
    sub: "user-42".into(),
    exp: 9999999999,
    role: "admin".into(),
};
let token = manager.encode(&claims).unwrap();
```

### Verifying a token

```rust
let claims: Claims = manager.decode(&token).unwrap();
println!("Subject: {}", claims.sub);
```

### `Auth<T>` extractor

Reads the `Authorization: Bearer <token>` header, verifies the JWT, and deserializes the claims into `T`. Returns `401` if the header is missing or invalid.

```rust
use chopin_auth::Auth;
use serde::Deserialize;

#[derive(Deserialize)]
struct Claims {
    sub: String,
    role: String,
}

fn protected(ctx: Context) -> Response {
    let Auth { claims } = match ctx.extract::<Auth<Claims>>() {
        Ok(a) => a,
        Err(res) => return res,  // 401 Unauthorized
    };
    Response::text(format!("Hello, {}", claims.sub))
}
```

### Role-based middleware

Use the `require_role_middleware!` macro to generate a zero-allocation middleware function for a specific role.

```rust
use chopin_auth::{require_role_middleware, middleware::Role};
use serde::Deserialize;

#[derive(Deserialize, PartialEq)]
enum AppRole { Admin, User }
impl Role for AppRole {}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    role: AppRole,
}

fn claims_has_role(claims: &Claims, required: &AppRole) -> bool {
    &claims.role == required
}

// Generates `fn require_admin(ctx: Context, next: BoxedHandler) -> Response`
require_role_middleware!(require_admin, Claims, AppRole::Admin, claims_has_role);

// Register as route middleware:
router.use_middleware("/admin", require_admin);
```

---

## Multipart / File Uploads

Parse `multipart/form-data` via `ctx.multipart()`. Returns `None` if the request's `Content-Type` is not multipart.

```rust
fn upload(ctx: Context) -> Response {
    let Some(parts) = ctx.multipart() else {
        return Response::bad_request();
    };

    for part in parts {
        let Ok(part) = part else { continue };

        let name = part.name.unwrap_or("unnamed");
        let filename = part.filename.unwrap_or("");
        let content_type = part.content_type.unwrap_or("application/octet-stream");
        let data: &[u8] = part.body;

        println!(
            "field={} file={} content_type={} size={}",
            name, filename, content_type, data.len()
        );
    }

    Response::text("uploaded")
}
```

Each `Part` exposes:
- `name: Option<&str>` — form field name from `Content-Disposition`
- `filename: Option<&str>` — file name from `Content-Disposition`
- `content_type: Option<&str>` — part-level `Content-Type`
- `body: &[u8]` — raw part bytes (zero-copy slice into the request body)

---

## Database (`chopin-pg` + `chopin-orm`)

Chopin ships with a synchronous, zero-dependency PostgreSQL driver (`chopin-pg`) and
a derive-macro ORM (`chopin-orm`) that sits on top of it.

### Direct driver usage (`chopin-pg`)

```toml
[dependencies]
chopin-pg = "0.5.21"
```

#### Connecting

```rust
use chopin_pg::{PgConfig, PgPool};

// From individual parameters
let config = PgConfig::new("localhost", 5432, "myuser", "mypassword", "mydb");
let mut pool = PgPool::connect(config, 10)?; // 10 connections

// From a URL
let config = PgConfig::from_url("postgres://user:pass@localhost:5432/mydb")?;
let mut pool = PgPool::connect(config, 4)?;
```

#### Queries

```rust
let mut conn = pool.get()?;

// Simple query (text protocol)
let rows = conn.query("SELECT id, name FROM users WHERE active = $1", &[&true])?;
for row in &rows {
    let id: i32 = row.get(0);
    let name: &str = row.get(1);
    println!("{}: {}", id, name);
}

// Execute (returns affected row count)
let affected = conn.execute("UPDATE users SET active = $1 WHERE id = $2", &[&false, &42i32])?;
```

#### Prepared statements

Statements are cached automatically — the first execution prepares the statement
and subsequent calls reuse it via an FNV-1a hash lookup.

#### Transactions

```rust
let mut conn = pool.get()?;
let mut tx = conn.transaction()?;

tx.execute("INSERT INTO orders (user_id, total) VALUES ($1, $2)", &[&1i32, &99.99f64])?;
tx.execute("UPDATE inventory SET qty = qty - 1 WHERE item_id = $1", &[&42i32])?;

tx.commit()?;  // or tx.rollback()?
```

#### COPY protocol

```rust
// Bulk import
let mut conn = pool.get()?;
conn.copy_in("COPY users (name, email) FROM STDIN WITH (FORMAT csv)", |writer| {
    writer.write_all(b"Alice,alice@example.com\n")?;
    writer.write_all(b"Bob,bob@example.com\n")?;
    Ok(())
})?;
```

#### LISTEN / NOTIFY

```rust
let mut conn = pool.get()?;
conn.execute("LISTEN my_channel", &[])?;

// Poll for notifications (non-blocking)
if let Some(notification) = conn.poll_notification()? {
    println!("channel={} payload={}", notification.channel, notification.payload);
}
```

### ORM usage (`chopin-orm`)

The ORM section above (under [ORM (`chopin-orm`)](#orm-chopin-orm)) covers model
definitions, CRUD, queries, and relationships. Here are additional database-specific
patterns:

#### Batch insert

Insert many records in a single round-trip:

```rust
use chopin_orm::batch_insert;

let mut users = vec![
    User { id: 0, name: "Alice".into(), email: "a@ex.com".into(), active: true },
    User { id: 0, name: "Bob".into(),   email: "b@ex.com".into(), active: true },
];
batch_insert(&mut users, &mut pool)?;
// users[0].id and users[1].id are now populated from RETURNING
```

#### Soft delete

Implement the `SoftDelete` trait for models with a `deleted_at` column:

```rust
use chopin_orm::SoftDelete;

impl SoftDelete for User {}

// Soft-delete a record (sets deleted_at = NOW())
User::soft_delete(42, &mut pool)?;

// Restore a soft-deleted record
User::restore(42, &mut pool)?;

// Query only active records (WHERE deleted_at IS NULL)
let active = User::find_active().all(&mut pool)?;

// Include soft-deleted records
let all = User::find_with_trashed().all(&mut pool)?;
```

#### Auto-migration

```rust
User::sync_schema(&mut pool)?;
// Creates the table if it doesn't exist, or adds missing columns
```
