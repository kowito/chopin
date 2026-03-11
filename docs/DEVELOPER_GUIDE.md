# The Chopin Framework: Comprehensive API Development Guide

Welcome to the official developer guide for the **Chopin HTTP Framework**. This guide is designed to take you from a hello-world API all the way to a production-hardened, database-backed microservice, leveraging Chopin's unique Shared-Nothing architecture.

---

## 1. Foundation & Setup

### Environment Configuration
Chopin requires a modern Rust toolchain (1.75+). It's optimized for UNIX-like environments (Linux `epoll` and macOS `kqueue`).

**Install the CLI Toolkit:**
The `chopin-cli` is your central tool for generating projects and managing them:
```bash
cargo install --path crates/chopin-cli
```

**Project Initialization:**
Create your new project with:
```bash
chopin new my_api
cd my_api
chopin dev
```

### The Big Picture (Architecture)
Chopin is not like Actix or Axum; it eschews heavy async runtimes (like Tokio) for the critical request path. It relies on a **Shared-Nothing threaded model**:
- **Atomic Listener Binding**: Using `SO_REUSEPORT`, every single thread binds independently to the same port. The kernel load balances traffic natively.
- **Worker Concurrency**: Each logical CPU core gets a dedicated thread running an Edge-Triggered event loop. You do not write async/await code. You write blocking, linear Rust that relies on O(1) Slab allocation for handling connection states.

### Boilerplate Breakdown
When you run `chopin new`, you get:
- `src/main.rs`: Contains `Chopin::new().mount_all_routes().serve("0.0.0.0:8080")`. The `mount_all_routes()` method uses the `inventory` crate to implicitly discover handlers across all modules.
- `src/apps/mod.rs`: The modular place to define distinct API domains (e.g., users, products).

---

## 2. Core Routing & Handlers

### Request Mapping
Chopin uses declarative attribute macros (`#[get]`, `#[post]`, `#[put]`, `#[delete]`). Handlers take a single `Context` argument and return a `Response`.

```rust
use chopin_core::{Context, Response};
use chopin_macros::get;

#[get("/ping")]
fn ping(ctx: Context) -> Response {
    Response::text("pong")
}
```

### Path Parameters & Headers
URL path variables and HTTP headers can be cleanly extracted from the `Context`.

```rust
#[post("/users/:id")]
fn update_user(ctx: Context) -> Response {
    let user_id = ctx.param("id").unwrap_or("0");
    let auth_header = ctx.header("Authorization");
    
    Response::text(format!("Updating {}", user_id))
}
```

### Type-Safe JSON Request Bodies
Chopin incorporates `kowito-json`, an ultra-fast Schema-JIT (Just-In-Time) serializer. It replaces standard reflection with compile-time layout maps.

```rust
use kowito_json::serialize::Serialize; // Or use chopin_core::json::Serialize

#[derive(Serialize)]
struct UserResponse {
    id: i32,
    username: String,
}

#[get("/user")]
fn get_user(ctx: Context) -> Response {
    let payload = UserResponse { id: 1, username: "virtuoso".into() };
    ctx.json(&payload)
}
```

### Zero-Copy Static File Serving
`Response::file(path)` opens a file and serves it via the platform `sendfile` syscall, so the file contents are transferred entirely in kernel space — Chopin's user-space process never touches the bytes.

```rust
#[get("/assets/:name")]
fn serve_asset(ctx: Context) -> Response {
    let name = ctx.param("name").unwrap_or("");
    Response::file(&format!("public/{}", name))
    // Automatically sets Content-Type from extension (~30 MIME types)
    // Returns 404 if the file doesn't exist
}
```

For custom byte ranges or pre-opened file descriptors, use `Response::sendfile(fd, offset, len, content_type)`.

---

## 3. The Middleware Pipeline

Middlewares in Chopin are pure functions that take a request `Context` and a `BoxedHandler` (the next step in the request chain) and return a `Response`.

### Interceptors
```rust
fn timing_middleware(ctx: Context, next: BoxedHandler) -> Response {
    let start = std::time::Instant::now();
    let mut response = next(ctx);
    let duration = start.elapsed();
    
    response.headers.push(("X-Runtime-Ms", duration.as_millis().to_string()));
    response
}
```
You attach these globally via `Router::new().layer(timing_middleware)`.

### Authentication & Role-Based Authorization
The `chopin-auth` crate brings JWT validation. Middleware can be statically generated with zero allocations using the `require_role_middleware!` macro.

```rust
use chopin_auth::{require_role_middleware, Role};

#[derive(PartialEq, Role)]
pub enum UserRole { Admin, Guest }

// This generates a static function `admin_only`
require_role_middleware!(admin_only, MyClaims, UserRole::Admin, MyClaims::has_role);

fn main() {
    let mut router = Router::new();
    // Wrap handler locally
    router.get("/admin/dashboard", admin_only(dashboard_handler));
}
```

---

## 4. Data & Persistence

Chopin implements a natively synchronous, high-throughput Postgres connection via `chopin-pg`, overlaid with the zero-allocation `chopin-orm`.

### Worker-Local Pooling
Because Chopin is Shared-Nothing, you should NOT use an `Arc<Mutex<Pool>>`. Rely on `thread_local!` to initialize a Postgres pool inside each worker thread seamlessly.

```rust
use chopin_pg::{PgPool, PgConfig};
use std::cell::RefCell;

thread_local! {
    pub static DB: RefCell<PgPool> = RefCell::new(
        PgPool::connect(
            PgConfig::from_url("postgres://user:pass@localhost/main").unwrap(),
            10 // 10 connections PER WORKER
        ).expect("Database panic")
    );
}
```

### ORM & ActiveModel
Declaring models and interacting with the database is completely type-safe with the `Model` derive macro.

**ActiveModel for Partial Updates:**
For fine-grained control over updates, use `ActiveModel`. It tracks modified fields dynamically, ensuring only changed columns are sent to the database.

```rust
#[get("/products/:id")]
fn update_price(ctx: Context) -> Response {
    let id: i32 = ctx.param("id").unwrap().parse().unwrap();
    
    DB.with(|db| {
        let mut pool = db.borrow_mut();
        
        // 1. Fetch existing model
        let product = Product::find().filter(ProductColumn::id.eq(id)).one(&mut *pool).unwrap().unwrap();
        
        // 2. Wrap in ActiveModel
        let mut active = ProductActiveModel::from(product);
        
        // 3. Set ONLY the fields that changed
        active.set("price", 4000);
        
        // 4. Save - intelligently issues UPDATE or INSERT
        active.save(&mut *pool).unwrap();
    });

    Response::new(200)
}
```

### Advanced Relationship Joins
Chopin ORM supports automatic eager loading and joins, including tables with composite primary keys.

**Defining Relations:**
Use `belongs_to` and `has_many` attributes in your model definition.

```rust
#[derive(Model)]
struct Order {
    #[model(primary_key)]
    id: i32,
    #[model(belongs_to = User)]
    user_id: i32,
}
```

**Eager Loading with Joins:**
```rust
let orders_with_users = Order::find()
    .join_parent::<User>()
    .all(&mut *pool).unwrap();
```
Chopin automatically resolves the foreign key mapping and constructs the `JOIN` clause.

---

## 5. Advanced Logic

### Streaming File Uploads (Multipart)
Chopin provides an inherently streaming `Multipart` parser on the `Context` that steps through body bytes without allocating massive buffers, protecting your application from out-of-memory errors on large uploads.

```rust
#[post("/upload")]
fn upload(ctx: Context) -> Response {
    if let Some(multipart) = ctx.multipart() {
        for part in multipart {
            let p = part.unwrap();
            println!("File Name: {:?}", p.filename);
            // Slice of bytes extracted immediately: p.body
        }
    }
    Response::text("Upload Done")
}
```

### Global Error Handling (Catch-Unwind)
By default, Chopin captures worker panics via `catch_unwind`. A faulty handler route that triggers a panic will fail transparently gracefully with a `500 Internal Server Error`, keeping the worker loop and connection handlers actively evaluating the next request. 

---

## 6. Performance & Scaling

Chopin operates efficiently on a single core but shines when scaling across physical CPUs.

### Thread-Per-Core Strategy & Affinity
In its initialization sequence, Chopin discovers the logical core count. Workers are explicitly pinned using `core_affinity`. Memory stays hot in L1/L2 caches.

### Zero-Copy I/O
Two techniques eliminate unnecessary data copies on every response:
- **`writev` flush**: Response headers and body are delivered in a single `writev` syscall. Static (`&'static [u8]`) and allocated byte bodies are never copied into the write buffer.
- **`sendfile` files**: `Response::file()` transfers file contents entirely in kernel space, bypassing user-space buffers entirely.

### Pre-Composed Middleware
At startup, `Router::finalize()` walks the entire route tree and composes all middleware chains into a single `Arc<dyn Fn(Context) -> Response>` per route. On the hot path, Chopin calls one pre-built closure — no `Arc::new`, no chain construction, no allocations.

### Global Allocator: mimalloc
Chopin uses `mimalloc` as the global allocator. Under heavy concurrency, mimalloc delivers dramatically lower allocation latency than the system allocator by using per-thread free lists and avoiding global lock contention.

### Kernel-Level Socket Handoff
Chopin exploits the deep optimizations in Linux (`TCP_DEFER_ACCEPT`, `TCP_FASTOPEN`) and macOS (`SO_NOSIGPIPE`, `TCP_FASTOPEN`). `TCP_NODELAY` is attached immediately to the initial listener so all accepted connections natively inherit the flag, preventing an O(N) penalty dynamically modifying individual sockets.

---

## 7. Testing & Quality

### The Architectural Linter
Validation is backed natively by the CLI tool:
```bash
chopin check
```
This utility analyzes standard project files and blocks builds containing prohibited anti-patterns (such as importing heavy async frameworks out-of-band).

### Benchmarking
Before deploying, validate your API throughput locally using `wrk`:
```bash
chopin bench
# Example: 10 threads, 200 connections
wrk -t10 -c200 -d10s http://localhost:8080/ping
```

---

## 8. Deployment & Observability

### Containerization
Standard setups include a pre-optimized, multi-stage Docker build ready out of the box with `chopin-cli` deployments.
```bash
chopin deploy docker
```
This scaffolding generates `amd64` / `arm64` tuned deployment images leveraging `musl` compilation.

### Health Metrics
Every Chopin Server automatically spins a background thread dedicated exclusively to metrics. Active connection slabs and throughput aggregates are piped regularly to standard out (`stdout`), providing out-of-the-box infrastructure monitoring:

```text
[Metrics] Active Connections: 194 | Total Requests: 1248030
```

---
*End of Guide.*
