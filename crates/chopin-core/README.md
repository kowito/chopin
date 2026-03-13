# chopin-core

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-core` is the zero-overhead HTTP engine powering the Chopin framework. It outperforms async runtimes like Tokio/Hyper by **5–6× on pipelined workloads** using a synchronous, shared-nothing architecture.

---

## Benchmark (Mac, 10 cores, 512 connections, 16-deep pipeline)

| Framework | Pipelined req/s |
|-----------|----------------|
| **chopin-core** | **~21M** |
| hyper (Tokio) | ~440K |

> No `async`, no `Arc`, no `Mutex`. Each worker thread is a fully self-contained event loop.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        chopin-core                           │
│                                                              │
│  Server::bind("0.0.0.0:8080")                               │
│      │                                                       │
│      ├── Worker 0  (SO_REUSEPORT fd)  ──► epoll/kqueue loop │
│      ├── Worker 1  (SO_REUSEPORT fd)  ──► epoll/kqueue loop │
│      ├── Worker 2  (SO_REUSEPORT fd)  ──► epoll/kqueue loop │
│      └── Worker N  (SO_REUSEPORT fd)  ──► epoll/kqueue loop │
│                                                              │
│  Per-Worker Hot Path:                                        │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ epoll_wait()                                        │    │
│  │   └─► Accept / Read / Parse (zero-copy parser)     │    │
│  │         └─► Router O(1) fast-table → Handler fn()  │    │
│  │               └─► Serialize into write_buf          │    │
│  │                     └─► Batch write (1 syscall      │    │
│  │                           for N pipelined requests) │    │
│  └─────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

### Design Principles

| Principle | Implementation |
|-----------|---------------|
| **No async runtime** | Every worker is a plain OS thread with a `loop { epoll_wait() }` |
| **Shared-nothing** | Each worker owns: event loop, accept socket, connection slab, router clone |
| **Zero allocation hot path** | Stack-allocated parse buffers, pre-baked header strings, `mimalloc` allocator |
| **Pipeline batching** | Small responses copy into `write_buf`; a single `write()` drains N pipelined requests |
| **O(1) routing** | Static routes resolved from a `HashMap` pre-built at startup; trie only for dynamic paths |
| **SO_REUSEPORT** | Each worker binds its own listen socket — kernel distributes connections without a shared queue |

---

## Quick Start

### Macro Style (recommended)

```rust
use chopin_core::{get, post, Context, Response, Chopin};
use kowito_json::KJson;

#[derive(KJson, Default)]
struct Message {
    message: &'static str,
}

#[get("/")]
fn index(_ctx: Context) -> Response {
    Response::text_static(b"Hello, World!")
}

#[get("/json")]
fn json_handler(_ctx: Context) -> Response {
    Response::json(&Message { message: "Hello, World!" })
}

fn main() {
    Chopin::new()
        .mount_all_routes()    // discovers all #[get], #[post], etc.
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

### Manual Router Style

```rust
use chopin_core::{Context, Response, Router, Server};

fn ping(_ctx: Context) -> Response {
    Response::text_static(b"pong")
}

fn main() {
    let mut router = Router::new();
    router.get("/ping", ping);

    Server::bind("0.0.0.0:8080")
        .workers(4)              // defaults to num_cpus
        .serve(router)
        .unwrap();
}
```

---

## Routing

### HTTP Methods

```rust
#[get("/users")]       fn list_users(_: Context) -> Response { ... }
#[post("/users")]      fn create_user(_: Context) -> Response { ... }
#[put("/users/:id")]   fn update_user(_: Context) -> Response { ... }
#[delete("/users/:id")]fn delete_user(_: Context) -> Response { ... }
#[patch("/users/:id")] fn patch_user(_: Context) -> Response { ... }
```

### Path Parameters

```rust
#[get("/users/:id/posts/:post_id")]
fn get_post(ctx: Context) -> Response {
    let id = ctx.param("id").unwrap_or("unknown");
    let post_id = ctx.param("post_id").unwrap_or("unknown");
    Response::text(format!("User {id}, Post {post_id}"))
}
```

### Wildcard Segments

```rust
#[get("/assets/*path")]
fn static_file(ctx: Context) -> Response {
    let path = ctx.param("path").unwrap_or("");
    // serve file at path
    Response::text(format!("File: {path}"))
}
```

### Sub-Routers (nest / merge)

```rust
let mut api = Router::new();
api.get("/status", status_handler);
api.post("/login", login_handler);

let mut root = Router::new();
root = root.nest("/api/v1", api);   // mounts at /api/v1/status, /api/v1/login
```

---

## Request Handling

### Accessing Request Data

```rust
#[post("/echo")]
fn echo(ctx: Context) -> Response {
    // Method, path, HTTP version
    let method = ctx.req.method;
    let path   = ctx.req.path;

    // Raw body bytes
    let body   = ctx.req.body;

    // Headers
    if let Some(ct) = ctx.req.header("content-type") {
        // ct is &str
    }

    // Query string ?foo=bar
    let q = ctx.query("foo").unwrap_or("default");

    Response::text("ok")
}
```

### JSON Extraction

```rust
use chopin_core::Json;
use kowito_json::KJson;

#[derive(KJson, Default)]
struct CreateUser {
    username: String,
    email: String,
}

#[post("/users")]
fn create_user(ctx: Context) -> Response {
    match Json::<CreateUser>::from_request(&ctx) {
        Ok(Json(user)) => Response::text(format!("Created: {}", user.username)),
        Err(_) => Response::bad_request("Invalid JSON"),
    }
}
```

### Query String Extraction

```rust
use chopin_core::Query;
use std::collections::HashMap;

#[get("/search")]
fn search(ctx: Context) -> Response {
    let params: Query<HashMap<String, String>> = Query::from_request(&ctx).unwrap_or_default();
    let q = params.get("q").map(|s| s.as_str()).unwrap_or("");
    Response::text(format!("Searching for: {q}"))
}
```

---

## Responses

```rust
// Plain text (static — zero allocation)
Response::text_static(b"Hello, World!")

// Plain text (owned)
Response::text("dynamic string")

// JSON (serialized via KJson)
Response::json(&my_struct)

// JSON from pre-serialized bytes
Response::json_bytes(b"{\"ok\":true}")

// HTML
Response::html("<h1>hello</h1>")

// Redirect
Response::redirect("/new-path")

// 404 / 400 / 500
Response::not_found()
Response::bad_request("reason")
Response::server_error("internal error")

// Custom status + headers
Response::text("Created")
    .with_status(201)
    .with_header("X-Request-Id", "abc123")

// File download (sendfile — zero-copy)
Response::file(fd, offset, length)

// Chunked streaming
Response::stream(my_iterator)
```

---

## Middleware

Middleware wraps handlers and can read/modify the request/response.

```rust
use chopin_core::{Context, Response, BoxedHandler};

fn auth_middleware(ctx: Context, next: BoxedHandler) -> Response {
    if ctx.req.header("x-api-key").is_none() {
        return Response::text("Unauthorized").with_status(401);
    }
    next(ctx)
}

fn logging_middleware(ctx: Context, next: BoxedHandler) -> Response {
    let path = ctx.req.path;
    let resp = next(ctx);
    eprintln!("{} -> {}", path, resp.status);
    resp
}

// Global middleware (applies to all routes)
let mut router = Router::new();
router.layer(logging_middleware);
router.layer(auth_middleware);

// Path-scoped middleware
router.layer_path("/admin", auth_middleware);
```

---

## WebSocket

```rust
use chopin_core::{get, Context, Response};
use chopin_core::websocket::{ws_upgrade, decode_frame, encode_text};

#[get("/ws")]
fn websocket_handler(ctx: Context) -> Response {
    // Returns the 101 Switching Protocols upgrade response.
    // The actual WS frame loop runs in the connection slab after this returns.
    ws_upgrade(&ctx).unwrap_or_else(|| Response::bad_request("Not a WS upgrade"))
}
```

---

## Multipart / Form Upload

```rust
use chopin_core::{post, Context, Response};
use chopin_core::multipart::parse_multipart;

#[post("/upload")]
fn upload(ctx: Context) -> Response {
    let boundary = ctx.req.header("content-type")
        .and_then(|ct| ct.split("boundary=").nth(1))
        .unwrap_or("");

    match parse_multipart(ctx.req.body, boundary.as_bytes()) {
        Ok(parts) => {
            for part in &parts {
                // part.name, part.filename, part.content_type, part.data
            }
            Response::text("uploaded")
        }
        Err(_) => Response::bad_request("bad multipart"),
    }
}
```

---

## OpenAPI / Scalar docs

```rust
fn main() {
    Chopin::new()
        .mount_all_routes()
        .with_openapi()           // adds /openapi.json and /docs (Scalar UI)
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

Doc comments on handlers become OpenAPI descriptions automatically:

```rust
/// Returns a greeting message.
/// Useful for healthchecks.
#[get("/hello")]
fn hello(_ctx: Context) -> Response {
    Response::text_static(b"Hello!")
}
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `WORKERS` | `num_cpus` | Number of worker threads |
| `PORT` | `8080` | Listening port |
| `CHOPIN_SLAB_CAPACITY` | `10000` | Max simultaneous connections per worker |
| `CHOPIN_EPOLL_TIMEOUT_MS` | `1000` | epoll wait timeout (ms). Set `0` for spin-poll (lowest latency, higher CPU) |

---

## Modules

| Module | Description |
|--------|-------------|
| `server` | `Server` (low-level) and `Chopin` (macro-driven) builders |
| `router` | Trie-based router with O(1) static fast-table |
| `worker` | Per-thread epoll/kqueue + io_uring event loop |
| `http` | `Request`, `Response`, `Body`, `Method`, `Context` |
| `parser` | Zero-copy HTTP/1.1 request parser |
| `conn` | Connection state machine and slab slot |
| `slab` | Fixed-capacity connection pool (no `malloc` in accept path) |
| `timer` | Hashed timing wheel for keep-alive timeouts |
| `websocket` | RFC 6455 WebSocket frame encode/decode |
| `multipart` | RFC 7578 multipart/form-data parser |
| `http2` | HTTP/2 frame primitives |
| `openapi` | OpenAPI 3.1 spec generation + Scalar UI handler |
| `extract` | `FromRequest` trait, `Json<T>`, `Query<T>` extractors |
| `headers` | Compact inline header store |
| `syscalls` | Raw epoll, kqueue, `SO_REUSEPORT`, `sendfile`, `writev` wrappers |

---

## License

MIT © kowito
