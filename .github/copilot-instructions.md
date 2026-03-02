# Chopin – Copilot Instructions

## What this project is
Chopin is a **shared-nothing, zero-allocation HTTP/1.1 framework** written in Rust. Performance is the primary design constraint — every design decision traces back to minimizing syscalls, heap allocations, and cross-thread synchronisation. Internal codename: `nocturne-op9-no2`.

## Workspace layout
```
crates/
  chopin-core        # HTTP engine: event loop, parser, router, serializer
  chopin-macros      # Proc-macros: #[get], #[post], … route registration
  chopin-pg          # Synchronous PostgreSQL wire-protocol driver
  chopin-orm         # Thin ORM over chopin-pg; #[derive(Model)]
  chopin-orm-macro   # Proc-macro: #[derive(Model)]
  chopin-auth        # JWT, password hashing, RBAC middleware
  chopin-cli         # `chopin` binary: new / dev / check / deploy / openapi
```

## Build & test commands
```bash
cargo check          # fast type-check (use first)
cargo build          # debug build
cargo build --release  # release: LTO fat, codegen-units=1, panic=abort, strip
cargo test           # unit + integration tests
cargo clippy         # lints
cargo fmt            # format
```
TFB benchmarking target lives in `crates/chopin-core/examples/tfb.rs`.

## Route registration pattern
Routes are registered at **link time** via `inventory::submit!`, not at runtime. The `#[get("/path")]` macro expands to an `inventory::submit!` call. `Chopin::new().mount_all_routes()` collects them all.

```rust
#[get("/users/:id")]
fn get_user(ctx: Context) -> Response {
    let id = ctx.param("id").unwrap_or("0");
    Response::json(&User { id: id.parse().unwrap_or(0) })
}
```
Never call `router.add()` directly unless writing a non-macro integration test.

## Response construction
Prefer the typed helpers — they write pre-baked content-type bytes:
```rust
Response::json(&value)          // application/json  (uses kowito_json KJson)
Response::text_static(b"hi")    // text/plain, zero-copy &'static [u8]
Response::text(String::from("dynamic"))
Response::html(body_bytes)
Response::file("assets/img.png") // sendfile — kernel-space zero-copy
Response::not_found()
Response::bad_request()
```
Custom headers: `response.headers.set("X-Foo", "bar")`.

## JSON serialisation
This project uses **`kowito_json` (`KJson`)**, not `serde_json` directly, for response serialisation. Derive `KJson` (re-exported as `chopin_core::KJson`) on response structs. `serde` / `serde_json` are used for request deserialisation (`Json<T>`, `Query<T>` extractors).

```rust
#[derive(KJson)]
struct Msg { message: &'static str }
```

## Performance-critical rules (do not violate)
- **No `Arc::new` / `Vec::new` on the hot request path.** Headers use `Headers` (fixed stack array). Route params use `[(&str, &str); MAX_PARAMS]`.
- **No `String` allocation in parsers or serializers.** Use `&str` slices into buffers.
- **`Conn` and `WorkerMetrics` are `#[repr(C, align(64))]`** — keep them that way to prevent false sharing.
- **`SystemTime::now()` for the `Date` header is called once per-response.** This is intentional — do not replace it with a cached/stale value. `format_http_date(unix_secs, &mut buf)` is cheap (~200 cycles); the syscall cost is acceptable.
- **Buffers**: `READ_BUF_SIZE = 8192`, `WRITE_BUF_SIZE = 16384` (conn.rs). Static/byte bodies bypass `write_buf` entirely via `writev` zero-copy.

## ORM pattern
```rust
#[derive(Model, KJson)]
struct World {
    #[model(primary_key)]
    id: i32,
    randomnumber: i32,
}
// Default table name = struct name lowercased + "s" → "worlds"
// Override: #[model(table_name = "custom_table")]
```
Database access uses `PgPool` per-worker (shared-nothing). Use `lazy_static!` or thread-locals for pool access — do not share state across workers via `Arc<Mutex<_>>`.

## Middleware
Middleware is composed **once at `finalize()`** into a single `Arc<dyn Fn(Context) -> Response>`. On the hot path this is one indirect call — no chain construction, no `Arc::new`.

```rust
router.middleware("/admin", auth_middleware);
// fn auth_middleware(ctx: Context, next: BoxedHandler) -> Response { … }
```

## TFB compliance checklist
All responses must include `Date`, `Server: chopin`, `Content-Type`, `Content-Length`, and `Connection`.

**NEVER pre-cache or store the formatted Date string.** Each response calls `SystemTime::now()` and `format_http_date(unix_secs, &mut date_buf)` directly — no `date_cache` fields, no struct state, no stale timestamps.
