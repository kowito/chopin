# Chopin – Copilot Instructions

## HONESTY RULES — NON-NEGOTIABLE

**NEVER LIE. NEVER FABRICATE. NEVER PRESENT OLD DATA AS NEW.**

1. **If asked to do something, DO IT.** Do not describe it, summarise it, or claim it was done without actually doing it.
2. **If you cannot do something, say so immediately.** "I can't do X because Y" is always the correct answer — never fake the result.
3. **Never present old/estimated/fabricated numbers as real results.** If a benchmark hasn't been run to completion and you don't have the actual output, say: "I don't have the real results yet."
4. **Never commit or publish data you did not directly observe.** If benchmark output was not captured, do not write it into README or docs.
5. **If a terminal command produces no output, report that honestly.** Do not re-use stale numbers and claim they are fresh.
6. **Benchmark results are only valid when you have the raw terminal output in hand.** No output = no result = do not update README.

Violating these rules is worse than doing nothing. Silence and honesty are always preferred over a lie.

---

## What this project is
Chopin is a **shared-nothing, zero-allocation HTTP/1.1 framework** written in Rust. Performance is the primary design constraint — every design decision traces back to minimizing syscalls, heap allocations, and cross-thread synchronisation. Internal codename: `nocturne-op9-no2`.

## Toolchain & edition
- **Edition 2024**, resolver 3 — requires Rust **nightly**
- Release profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`, `panic = "abort"`, `strip = true`
- Version `0.5.14` across all workspace crates

## Workspace layout
```
crates/
  chopin-core        # HTTP engine: event loop, parser, router, serializer (mimalloc global allocator)
  chopin-macros      # Proc-macros: #[get], #[post], … route registration via inventory::submit!
  chopin-pg          # Synchronous PostgreSQL wire-protocol driver (only dep: libc)
  chopin-orm         # Thin ORM over chopin-pg; #[derive(Model)], Executor trait, MockExecutor
  chopin-orm-macro   # Proc-macro: #[derive(Model)]
  chopin-auth        # JWT (jsonwebtoken), Argon2id password hashing, RBAC middleware
  chopin-cli         # `chopin` binary: new / dev / check / deploy / openapi
```

## Build & test commands
```bash
cargo check                                  # fast type-check (use first)
cargo fmt --all && cargo clippy --all --all-targets -- -D warnings  # lint pipeline
cargo test                                   # unit + integration (362 tests in chopin-pg alone)
cargo test -p chopin-pg --lib                # fast: chopin-pg unit tests only
cargo build --release                        # LTO fat build for benchmarks
```
TFB benchmarking target: `crates/chopin-core/examples/tfb.rs`. PG benchmarks: `crates/chopin-pg/examples/bench_*.rs`.

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

## JSON serialisation
**`kowito_json` (`KJson`)** for response serialisation, not `serde_json`. Derive `KJson` on response structs. `serde` / `serde_json` are used only for request deserialisation (`Json<T>`, `Query<T>` extractors).

```rust
#[derive(KJson)]
struct Msg { message: &'static str }
```

## chopin-pg — PostgreSQL driver conventions
- **Zero external dependencies** in production — only `libc`. All crypto (SCRAM-SHA-256), codec, wire protocol are hand-written. Do not add crate dependencies.
- **Synchronous non-blocking I/O** — sockets in NB mode with `libc::poll()` (not `thread::sleep`). See `wait_readable()`/`wait_writable()` in `connection.rs`.
- **No pipeline queries** — pipeline mode was explicitly removed (violates TFB rules requiring each DB query to be a separate round-trip). Do not re-introduce it.
- **Query API** on `PgConnection` and `Transaction`:
  - `query(sql, &[params])` → `Vec<Row>` — general multi-row
  - `query_one(sql, &[params])` → `Row` — dedicated single-row read path (no Vec allocation)
  - `query_opt(sql, &[params])` → `Option<Row>` — returns None if no rows
  - `execute(sql, &[params])` → `u64` (rows affected)
  - `query_simple(sql)` → `Vec<Row>` — simple protocol, no params
- **CompactBytes** in `row.rs` — values ≤24 bytes stored inline (covers all scalar PG types), larger values on heap. Intentional for cache performance.
- **Pool is worker-local** — `PgPool` per-worker, no cross-thread sharing. `ConnectionGuard` is RAII with `Deref`/`DerefMut` to `PgConnection`. Pool `get()` uses exponential backoff `[100, 250, 500, 1000]µs`.
- **Statement cache** — FNV-1a hash, LRU tick-based eviction, 256-entry default in `statement.rs`.
- **Planning doc** — `crates/chopin-pg/implement.md` tracks feature completion (~91% done).

## chopin-auth conventions
- `Auth<T>` extractor implements `FromRequest` — reads `Authorization: Bearer <token>` from headers
- Global JWT manager via `OnceLock`: call `init_jwt_manager(JwtManager::new(config))` at startup
- RBAC: `require_role_middleware!(name, Role::Admin)` macro generates middleware functions
- Password hashing: `PasswordHasher::interactive()` / `::sensitive()` — Argon2id presets
- Token blacklist: `TokenBlacklist` with `revoke(jti)` / `is_revoked(jti)` / `cleanup()`

## ORM pattern
```rust
#[derive(Model, KJson)]
#[model(table_name = "worlds")]  // default: struct name lowercased + "s"
struct World {
    #[model(primary_key)]
    id: i32,
    randomnumber: i32,
}
```
- Database access: `PgPool` per-worker (shared-nothing). Use `lazy_static!` or thread-locals — never `Arc<Mutex<_>>`.
- **MockExecutor** + `mock_row!` macro for unit testing without a database. Results drain FIFO.
- **Executor trait** — both `PgConnection` and `MockExecutor` implement it; write DB logic against `impl Executor`.

## Performance-critical rules (do not violate)
- **No `Arc::new` / `Vec::new` on the hot request path.** Headers use `Headers` (fixed stack array). Route params: `[(&str, &str); MAX_PARAMS]`.
- **No `String` allocation in parsers or serialisers.** Use `&str` slices into buffers.
- **`Conn` and `WorkerMetrics` are `#[repr(C, align(64))]`** — prevents false sharing.
- **`SystemTime::now()` per-response for the `Date` header.** Do not cache/store the formatted date string. `format_http_date()` is ~200 cycles.
- **Buffers**: `READ_BUF_SIZE = 8192`, `WRITE_BUF_SIZE = 16384`. Static/byte bodies bypass `write_buf` via `writev` zero-copy.

## Middleware
Composed **once at `finalize()`** into a single `Arc<dyn Fn(Context) -> Response>`. One indirect call on the hot path — no chain construction, no `Arc::new`.

```rust
router.middleware("/admin", auth_middleware);
```

## Testing conventions
- Every chopin-pg source file has `#[cfg(test)] mod tests` at the bottom — standard `#[test]` + `assert_eq!`
- Integration tests needing PostgreSQL gracefully skip if DB unavailable
- `Row::mock(&names, &values)` constructs test rows without wire protocol
- Integration tests and benchmarks use **localhost PostgreSQL** (not Docker) — ensure `postgres` is running locally on port 5432
- **Benchmarks must run to completion** — use `while pgrep -f bench_compare > /dev/null 2>&1; do sleep 10; done` to wait for the process to finish before examining results. Do NOT interrupt or kill the process mid-run.
- Run `cargo fmt --all && cargo clippy --all --all-targets -- -D warnings` before committing

## TFB compliance
All HTTP responses: `Date`, `Server: chopin`, `Content-Type`, `Content-Length`, `Connection` headers required.
**Each database query must be a separate round-trip** — no pipelining, no batching multiple SELECTs.
**NEVER cache the Date header** — each response calls `SystemTime::now()` directly.
