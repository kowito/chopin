# Chopin 🎼 (Codename: Nocturne Op. 9 No. 2)
<p align="center">
  <img src="docs/site/assets/logo.png" alt="Chopin Logo" width="200">
</p>

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-nightly-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

At peak optimization, Chopin delivers industry-leading throughput, effectively outperforming standard frameworks like Hyper by **~40%** while maintaining significantly lower latency.

## 🚀 Core Architecture

### 1. Shared-Nothing Model
Chopin adheres strictly to a shared-nothing model to ensure linear scaling across multi-core systems.
- **Independent Workers**: Each CPU core runs its own isolated event loop, memory allocator, and metrics counters.
- **SO_REUSEPORT Architecture**: Every worker thread creates its own listening socket. The kernel balances connections at the socket layer, eliminating any "Acceptor" thread bottleneck or cross-thread synchronization.
- **Partitioned Metrics**: Metrics are collected per-worker in 64-byte aligned, cache-local atomic counters, eliminating "cache-line bouncing."

### 2. Zero-Allocation Request Pipeline
- **Zero-Alloc Parser**: Slices raw socket buffers into string references (`&str`) without a single heap allocation.
- **Stack-Allocated Hot-Paths**: HTTP headers and route parameters are stored in fixed-size stack arrays.
- **Radix Tree Routing**: Efficient $O(K)$ path matching (where $K$ is path length) with zero-cost parameter extraction.
- **Raw Byte Serialization**: Responses are built using raw byte copies and inline `itoa` formatting, removing the overhead of `std::fmt`.
- **Pre-Composed Middleware**: Middleware chains are resolved once at router `finalize()`. The hot path calls a single pre-built `Arc<dyn Fn>` with no per-request `Arc::new` or chain construction.
- **writev Zero-Copy Flush**: Response headers and body are written in one `writev` syscall. Static/byte bodies bypass the write buffer entirely — no memcpy.
- **sendfile File Serving**: `Response::file()` transfers file data directly in kernel space via `sendfile` (Linux) / `sendfile` (macOS), eliminating user-space copies.

### 3. Native Asynchronous Core
- **Platform Native**: Direct interaction with `kqueue` (macOS) and `epoll` (Linux) via low-level `libc` syscalls.
- **Manual Buffer Management**: Uses a custom `ConnectionSlab` (Slab Allocator) for O(1) connection state management.
- **Robust I/O**: Intelligent partial-write tracking (`write_pos`) to handle backpressure and socket saturation without data loss.

## 🛠️ Features

### chopin-core — HTTP Engine
- **Radix Router**: Static paths, labeled parameters (`:id`), and wildcards (`*path`) with O(K) matching.
- **Declarative Extractors**: `FromRequest` trait for automatic `Json<T>`, `Query<X>`, and custom extractor support.
- **Route Macros**: `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`, `#[head]`, `#[options]`, `#[trace]`, `#[connect]` via `chopin-macros`; inventory-based auto-discovery with zero per-request cost.
- **`ctx.param_parse::<T>()`**: Typed path-parameter extraction on `Context` — parses the raw segment with `FromStr`, returns `Err(400 Bad Request)` automatically on absent or unparseable values; eliminates `.unwrap()` boilerplate.
- **`Chopin::with_worker_init(f)`**: Builder method that runs a closure once inside each spawned worker thread before the event loop starts — the idiomatic hook for `chopin_pg::init_pool()` or any other per-thread initialisation.
- **Zero-Copy File Serving**: `Response::file(path)` via platform `sendfile` (Linux/macOS) with automatic MIME detection (~30 types).
- **writev Body Flush**: Headers and body written in one `writev` syscall — no memcpy into the write buffer.
- **Pre-Composed Middleware**: Chains resolved once at `finalize()`; zero `Arc::new` allocations on the hot path.
- **WebSocket** (RFC 6455): Upgrade handshake validation, `Sec-WebSocket-Accept` derivation, frame-level codec (text, binary, ping/pong, close, continuation opcodes), 16 MiB max frame size.
- **HTTP/2 Frame Codec** (RFC 9113): Protocol detection, frame encode/decode, SETTINGS exchange, h2c upgrade and prior-knowledge detection.
- **TLS 1.2/1.3** (`feature = "tls"`): rustls-backed termination in the event-loop worker; PEM cert/key loading with chain support.
- **Rate Limiting**: Per-IP token-bucket middleware; thread-local state (zero mutexes), configurable capacity/window, `X-Real-IP` / `X-Forwarded-For` / socket peer-address source priority, trusted-proxy depth, bounded bucket map with idle+LRU eviction.
- **OpenAPI 3.0 Generation**: Auto-derives spec from all registered routes including path parameters.
- **Per-Worker Metrics**: 64-byte aligned atomic counters (`req_count`, `active_conns`, `bytes_sent`) with global registry; built-in `/metrics` and `/health` handlers.
- **Multipart Parsing**: Zero-copy boundary splitting with SIMD-accelerated search via `memchr`.
- **I/O Filter Stack**: Composable `Filter` trait (`process_read`/`process_write`) with inline `ArrayVec` stack (no heap for ≤3 filters); built-in `PassthroughFilter` and `LoggingFilter`.
- **Buffer Pool**: Reusable `BufGuard` allocations to amortize per-request heap cost.
- **mimalloc**: Global allocator for all binaries linking `chopin-core` — per-thread free-lists, low fragmentation, cache-aware design.
- **io-uring** (`feature = "io-uring"`): 35–50% latency reduction on Linux (see benchmarks).
- **Panic Resilience**: `catch_unwind` per handler — a panic never crashes the worker thread.
- **Production-Ready**: HTTP/1.1 keep-alive, graceful shutdown, O(1) connection pruning.

### chopin-pg — PostgreSQL Driver
- **SCRAM-SHA-256** authentication; TLS (`feature = "tls"`) via rustls.
- **Extended Query Protocol**: Parse/Bind/Execute with per-connection statement cache (`CacheStats`).
- **Transactions**: Closure-based API with automatic rollback on drop or panic (`CancelToken`).
- **COPY Protocol**: `CopyWriter` (COPY IN) and `CopyReader` (COPY OUT).
- **LISTEN/NOTIFY**: Notification buffering during active query processing.
- **Rich Type System**: 40+ OIDs — bool, int2/4/8, float4/8, text, varchar, bytea, char, oid, uuid, json, jsonb, date, time, timestamp, timestamptz, interval, numeric, inet, cidr, macaddr, macaddr8, point, line, lseg, box, path, polygon, circle, bit, varbit, int4/8/num/ts/tstz range types, and typed arrays.
- **Connection Pool**: Worker-local `PgPool` with RAII `ConnectionGuard`, FIFO idle queue, `try_get()`/`get()` with configurable checkout timeout, and `PoolStats`.
- **Thread-Local Pool API**: `chopin_pg::init_pool(url, size)` initialises a per-thread `PgPool`; `chopin_pg::pool()` returns a `&'static mut PgPool` with zero synchronisation cost — designed to be called from `Chopin::with_worker_init`. Re-initialising replaces the old pool cleanly.
- **Error Classification**: Transient vs permanent errors for retry logic.
- **Non-blocking I/O**: Sockets set non-blocking post-connect; poll-based reads/writes with configurable timeouts.

### chopin-orm — ORM
- **`#[derive(Model)]`** macro for type-safe, zero-boilerplate model definitions via `chopin-orm-macro`.
- **`QueryBuilder`** with `Condition`/`Expr` DSL: `AND`/`OR` tree nesting, raw SQL escaping, indexed parameter binding.
- **`ActiveModel`**: Field-level change tracking (`Set`/`Unchanged`/`NotSet`) for minimal `INSERT`/`UPDATE` queries.
- **`MigrationManager`**: `up`, `down`, `status` with `__chopin_migrations` ledger table; `Index` helpers.
- **`MockExecutor`**: In-memory FIFO test stub — no PostgreSQL connection needed.
- **`Executor` trait**: Uniform interface over `PgPool`, `PgConnection`, and `Transaction`.

### chopin-auth — Authentication
- **JWT**: Encode/decode with HS256, RS256, ES256; configurable `JwtConfig` (issuer, audience, algorithm); global `JwtManager` singleton; `Auth<T>` request extractor.
- **`StandardClaims<R>`**: Generic, batteries-included claims struct — `sub`, `jti`, `exp`, `iat`, `role: Option<R>`, `scope: Option<String>`. Implements `HasJti`, `RoleCheck<R>`, and `ScopeCheck` out of the box. Eliminates manual claims boilerplate for ~95 % of projects.
- **Token Revocation**: Thread-safe JTI blacklist (`TokenBlacklist`) with optional per-token expiry and `cleanup()`.
- **Password Hashing**: Argon2 via `PasswordHasher` (interactive/sensitive presets); `hash_password` / `verify_password` helpers.
- **RBAC Middleware**: `require_role_middleware!` macro generates zero-allocation middleware; `Role` and `RoleCheck<R>` traits.
- **OAuth 2.0 Scopes**: `ScopeCheck` trait for space-delimited scope validation.
- **PKCE** (RFC 7636): `code_verifier()` (CSPRNG, 32-byte entropy), `code_challenge_s256()`, `AuthorizationUrl` builder, `token_pair()` issuance helper.
- **JWKS** (RFC 7517): `JwksProvider` parses RSA (RS256/384/512) and EC (ES256/384) key sets; `kid`-based key lookup; `JwkSet` deserialization.

### chopin-macros — Route & Auth Macros
- **Route macros**: `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`, `#[head]`, `#[options]`, `#[trace]`, `#[connect]` — register handlers via `inventory` at link time, zero runtime overhead.
- **`#[require_role(ClaimsType, Role::Admin)]`**: Inline RBAC guard. Decodes the `Authorization: Bearer` token, verifies the role, and short-circuits with `401`/`403` — no boilerplate in the handler body.
- **`#[require_scope(ClaimsType, "scope:name")]`**: Inline OAuth 2.0 scope guard. Same short-circuit pattern for `401`/`403`.
- **`#[derive(IntoResponse)]`**: Derive macro for error enums. Annotate each variant with `#[status(N)]` to auto-generate `impl From<YourError> for Response` — enables `?` propagation from handlers and services.

  ```rust
  // Place auth guards ABOVE the route macro so they wrap the body first.
  #[require_role(Claims, Role::Admin)]
  #[get("/admin/dashboard")]
  pub fn admin_dashboard(ctx: Context) -> Response {
      ctx.json(&"welcome, admin")
  }
  ```

### chopin-cli — Developer Toolchain
- `chopin new <name>` — project scaffolding with Cargo workspace layout.
- `chopin dev` — hot-reload development server.
- `chopin build` — production build.
- `chopin migrate up/down/status` — database migration management.
- `chopin bench` — run benchmarks.
- `chopin db` — database utilities.
- `chopin generate app/handler/model` — code scaffolding (app modules, handler functions, model structs + migrations from field definitions).
- `chopin generate scaffold <Name> field:type …` — generate a **complete CRUD resource** in one command: `#[derive(Model)]` struct, `Create`/`Update` DTOs, type-safe services using `chopin_pg::pool()`, REST handlers (index/show/create/update/destroy) wired with `ctx.param_parse`, an `#[derive(IntoResponse)]` error enum, and up/down migrations. No TODOs, no stubs — production-ready from the start.
- `chopin generate auth` — scaffold a complete authentication module: `User` model, `Role` enum, `register`/`login`/`logout`/`refresh` handlers and services, `AuthError` domain type, and a `CREATE TABLE users` migration — all wired to `StandardClaims<Role>` and `PasswordHasher` out of the box.
- `chopin db seed` — run seed data (via `CHOPIN_SEED=1 cargo run -- --seed`).
- `chopin check` — architectural linter.
- `chopin deploy docker` — optimized Dockerfile generation.
- `chopin openapi` — scrape routes and emit OpenAPI 3.0 spec.

## 🛠️ Usage Example

Chopin uses attribute-based route discovery for a clean, declarative experience.

```rust
use chopin_core::{Chopin, Context, Response};
use chopin_macros::get;

#[get("/user")]
fn user_handler(ctx: Context) -> Response {
    let user = User { id: 1, username: "kowito".into() };
    ctx.json(&user)
}

fn main() {
    Chopin::new()
        .mount_all_routes()
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

### With a database pool (thread-local, zero-Arc)

```rust
use chopin_core::{Chopin, Context, Response};
use chopin_macros::get;

#[get("/posts/:id")]
fn show_post(ctx: Context) -> Response {
    let id: i32 = match ctx.param_parse("id") {
        Ok(v) => v,
        Err(r) => return r,   // 400 Bad Request
    };
    match Post::find_by_id(chopin_pg::pool(), id) {
        Ok(Some(post)) => Response::json(&post),
        Ok(None)       => Response::new(404),
        Err(_)         => Response::new(500),
    }
}

fn main() {
    Chopin::new()
        .mount_all_routes()
        .with_worker_init(|| {
            chopin_pg::init_pool("postgres://localhost/myapp", 10)
                .expect("DB pool init failed");
        })
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

## 🎹 CLI at a Glance

The `chopin` CLI handles everything from project scaffolding to production deployment.

```bash
cargo install chopin-cli
chopin new my_app
chopin dev                                # hot-reload development
chopin check                              # architectural linter
chopin generate scaffold Post title:String body:text   # full CRUD resource
chopin migrate up                         # run pending migrations
chopin db seed                            # load seed data
chopin openapi                            # generate OpenAPI spec
```

## 📊 Performance Benchmark

### TechEmpower Framework Benchmark Report

Run 20260526114540 compared Chopin v0.5.31 against axum, elysia, and hono on Linux 6.12.72-linuxkit (Docker / Apple Silicon). The shared test surface was intentionally limited to `/json` and `/plaintext`.

```text
JSON sweep winners:     axum (16-128), chopin (256-512)
Plaintext sweep winners: chopin (256-16384)

Peak throughput snapshot

JSON @ 512 concurrency
Chopin | ################################################## 652,465
Axum   | #############################################     580,429
Elysia | #######################################            515,637
Hono   | ###################                               238,367

Plaintext @ 16,384 pipeline depth
Chopin | ################################################## 2,694,197
Axum   | ########################################            2,162,232
Elysia | #########################                           1,337,024
Hono   | ####                                                322,058
```

Chopin only takes the JSON lead at the two highest concurrency levels. Axum is fastest through 128 concurrency, while Chopin leads plaintext at every pipeline depth.

Benchmark numbers change with hardware, kernel, and compiler settings, so this section captures one TechEmpower run instead of a fixed cross-framework table. For the latest methodology, commands, and results, see [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

**🔧 Optimization Tip**: On Linux, enable the `io-uring` feature for lower latency on supported kernels:
```toml
chopin-core = { version = "0.5.30", features = ["io-uring"] }
```

For detailed benchmark methodology, optimization layers, and how to maximize performance, see [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

> **🚀 Benchmark Integrity**: Check out our [TechEmpower Compliance Guide](docs/TFB_COMPLIANCE.md) to learn how Chopin achieves these numbers while remaining fully "Realistic" and rule-compliant.

---

## ⚙️ Runtime Configuration

Chopin workers read the following environment variables at startup:

| Variable | Default | Description |
| :--- | :--- | :--- |
| `CHOPIN_SLAB_CAPACITY` | `16000` | Max concurrent connections per worker. Increase for high-concurrency workloads (e.g. `25000`). |
| `CHOPIN_EPOLL_TIMEOUT_MS` | `100` | Event loop poll timeout in milliseconds. Lower values improve timer resolution at the cost of CPU. |
| `CHOPIN_READ_BUF_SIZE` | `8192` | Per-connection read buffer size in bytes (min 512, max 65535). Set at startup; all workers use the same value. |
| `CHOPIN_WRITE_BUF_SIZE` | `32768` | Per-connection write buffer size in bytes (min 512, max 65535). Larger values reduce write syscalls for big responses. |

```bash
CHOPIN_SLAB_CAPACITY=25000 CHOPIN_READ_BUF_SIZE=16384 CHOPIN_WRITE_BUF_SIZE=65535 ./my_chopin_server
```

> **Memory tip**: each connection slot uses `CHOPIN_READ_BUF_SIZE + CHOPIN_WRITE_BUF_SIZE` bytes of heap. With the defaults (8 KiB + 32 KiB = 40 KiB) and 16 000 slots, that is ~625 MiB per worker. Tune `CHOPIN_SLAB_CAPACITY` down if memory is tight.

---

## 🗺️ Roadmap

Living roadmap of planned improvements, distilled from an audit of every workspace crate. Items are grouped by theme and priority rather than calendar dates. The deeper ntex-parity engineering roadmap lives in [docs/roadmap.md](docs/roadmap.md).

### Snapshot (2026-06-01)

- **Completed:** P0 (production hardening) is fully shipped.
- **Remaining:** 28 roadmap items across P1-P5, Documentation, and Testing.
- **By section:** P1 (1), P2 (3), P3 (8), P4 (5), P5 (4), Documentation (4), Testing (3).

### ✅ Recently shipped (on branch `roadmap/p0-hardening`)

- **Mutual TLS** — `Server::with_mtls(cert, key, client_ca)` and `Chopin::serve_mtls(...)` in [crates/chopin-core/src/tls.rs](crates/chopin-core/src/tls.rs) and [crates/chopin-core/src/server.rs](crates/chopin-core/src/server.rs), backed by `rustls::server::WebPkiClientVerifier`.
- **Configurable graceful shutdown** — `CHOPIN_SHUTDOWN_TIMEOUT_MS` honored by both the epoll and io_uring worker loops in [crates/chopin-core/src/worker.rs](crates/chopin-core/src/worker.rs).
- **CLI migrations no longer panic on non-UTF8 paths** — [crates/chopin-cli/src/migrations.rs](crates/chopin-cli/src/migrations.rs) skips unrecognized entries via typed helpers instead of `.unwrap().to_str().unwrap()`.
- **`chopin migrate redo`** subcommand for fast iteration during development in [crates/chopin-cli/src/main.rs](crates/chopin-cli/src/main.rs) (rollback + reapply in one command).
- **`chopin migrate rollback`** friendly alias for `migrate down` in [crates/chopin-cli/src/main.rs](crates/chopin-cli/src/main.rs).
- **Colored CLI error chains** — bold-red `error:` header, numbered cause-chain walk, and `RUST_BACKTRACE=1` hint in [crates/chopin-cli/src/main.rs](crates/chopin-cli/src/main.rs).
- **Richer `AuthError`** — `Malformed`, `InvalidSignature`, `InvalidAlgorithm`, `MissingKid`, `NotYetValid` plus `AuthError::http_status()` (401 vs 500) in [crates/chopin-auth/src/jwt.rs](crates/chopin-auth/src/jwt.rs).
- **JWKS TTL & lazy refresh** — `age()`, `is_stale(ttl)`, `refresh_if_stale(ttl, fetch)` in [crates/chopin-auth/src/jwks.rs](crates/chopin-auth/src/jwks.rs); unknown `kid` now returns `MissingKid`.
- **PgPoolConfig::from_env()** — 9 `CHOPIN_PG_*` env vars (min/max size, lifetime, idle/checkout/connect timeouts, validation query, auto-reconnect, test-on-checkout) in [crates/chopin-pg/src/pool.rs](crates/chopin-pg/src/pool.rs).
- **`chopin-auth` integration test suite** — 23 cross-module tests covering PKCE, password hashing, revocation lifecycle, JWT error classification, and JWKS refresh semantics in [crates/chopin-auth/tests/integration.rs](crates/chopin-auth/tests/integration.rs).
- **CORS middleware** — `Cors` builder + preflight handling in [crates/chopin-core/src/cors.rs](crates/chopin-core/src/cors.rs).
- **Gzip compression middleware** — content-type-aware, double-encode-safe, `CHOPIN_GZIP_MIN_SIZE` threshold in [crates/chopin-core/src/compression.rs](crates/chopin-core/src/compression.rs) (behind `compression` feature).
- **Request logger middleware** — `tracing::info!` line per request with method/path/status/latency in [crates/chopin-core/src/logger.rs](crates/chopin-core/src/logger.rs).
- **Date header cache** — per-worker `[u8; 29]` Date string refreshed at most once per second in [crates/chopin-core/src/worker.rs](crates/chopin-core/src/worker.rs).
- **Read backpressure** — reads are paused when the per-connection write buffer is > 75% full, preventing unbounded memory growth (already implemented in [crates/chopin-core/src/worker.rs](crates/chopin-core/src/worker.rs)).
- **`Headers::get(name)`** — case-insensitive lookup helper added to [crates/chopin-core/src/headers.rs](crates/chopin-core/src/headers.rs).

### P0 — Production hardening (security & robustness)

_All P0 items shipped — see the section above._

### P1 — Quick wins (high value, low effort)

- **LUT-based integer encoding** for Content-Length and status lines (see [docs/roadmap.md](docs/roadmap.md) Phase 2).

### P2 — Observability

- **Latency histograms & error counters** in [crates/chopin-core/src/metrics.rs](crates/chopin-core/src/metrics.rs) (currently only `req_count`, `active_conns`, `bytes_sent`).
- **`tracing` integration** with feature flag; per-request span carrying a request ID propagated to handlers and downstream `chopin-pg` queries.
- **Prometheus exposition format** for `/metrics` (text format with HELP/TYPE), plus an OpenMetrics feature flag.

### P3 — Feature completion

- **HTTP/2 request path** — the frame codec in [crates/chopin-core/src/http2.rs](crates/chopin-core/src/http2.rs) exists but is not wired to the router; complete `h2c` upgrade → stream multiplexing → HPACK so gRPC and modern clients work end-to-end.
- **Brotli / zstd compression** as additional response encodings (gzip ships today via `compression::gzip`).
- **Prepared-statement cache API** and **query pipelining** in [crates/chopin-pg/src/connection.rs](crates/chopin-pg/src/connection.rs) — today every `query()` re-parses; batch independent queries into one round trip.
- **COPY OUT streaming reader** in [crates/chopin-pg/src/connection.rs](crates/chopin-pg/src/connection.rs) — only `CopyWriter` (COPY IN) is implemented.
- **LISTEN/NOTIFY callback API** — internal buffering exists but no user-facing async handler registration.
- **ORM relationships** in [crates/chopin-orm/src/builder.rs](crates/chopin-orm/src/builder.rs) — promote raw `join()` into declarative `#[has_many] / #[belongs_to] / #[has_one]` on `#[derive(Model)]` with eager-load helpers.
- **Soft deletes & timestamps as derive options** in [crates/chopin-orm-macro](crates/chopin-orm-macro) instead of opt-in trait wiring.
- **OAuth `/token` and `/refresh` handlers** built on top of [crates/chopin-auth/src/oauth.rs](crates/chopin-auth/src/oauth.rs); ship reference providers (GitHub, Google) behind feature flags.

### P4 — Developer experience

- **Scaffold cleanup** — remove every `TODO` / `todo!()` from `chopin generate` output ([crates/chopin-cli/src/generate.rs](crates/chopin-cli/src/generate.rs)); generated services must compile and run unedited.
- **`chopin introspect`** — generate `#[derive(Model)]` structs from an existing database schema.
- **Embedded file watcher** for `chopin dev` instead of shelling out to `cargo-watch`; reload on `.env` changes too.
- **Rich errors via `miette`** for CLI parse/config/migration failures.
- **OpenAPI generator polish** — auto-discover path parameters, response schemas from `#[derive(IntoResponse)]`, and request bodies from `Json<T>` extractors.

### P5 — Performance (see [docs/roadmap.md](docs/roadmap.md) for full plan)

- Thread-local `BytesMut` pool for dynamic response buffers.
- Adaptive read/write buffer watermarks per connection.
- Concurrent in-flight request dispatch on a single keep-alive connection.
- Finish the io_uring backend: registered fixed buffers, `IORING_OP_SPLICE` sendfile, optional `SQPOLL`.

### Documentation

- HTTP/2 usage guide (h2c upgrade example) once the request path lands.
- `chopin-pg` COPY example under [crates/chopin-pg/examples](crates/chopin-pg/examples).
- Unified error-handling guide across `core`, `pg`, `orm`, `auth`.
- Document every `CHOPIN_*` runtime variable (including the planned `CHOPIN_SHUTDOWN_TIMEOUT_MS` and pool tunables) in one table.

### Testing

- Raise per-crate source-to-test ratios. Current baseline: `chopin-auth` 9 src / 1 integration suite (23 tests) + per-module tests, `chopin-pg` 12 / 1, `chopin-orm` 8 / 1, `chopin-core` 16 / 2.
- Add fuzz targets for the HTTP/1.1 parser and the PG wire-protocol decoder.
- Add a TLS integration test exercising both server-auth and the planned mTLS path.

---

Contributions welcome — pick any roadmap item, open a tracking issue first, and reference the file paths above so reviewers can scope quickly.

---
"Simple as a melody, fast as a nocturne." - *nocturne-op9-no2*
