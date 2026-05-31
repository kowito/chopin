# Changelog

All notable changes to the Chopin framework are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

### Added

#### chopin-core
- **Mutual TLS** — `Server::with_mtls(cert, key, client_ca_pem)` and `Chopin::serve_mtls(addr, cert, key, client_ca_pem)` build a rustls `ServerConfig` with `WebPkiClientVerifier` so the server requires and validates client certificates against the supplied CA bundle. Server-auth-only `with_tls` / `serve_tls` are unchanged. Behind the existing `tls` feature.
- **Configurable graceful shutdown drain** — workers now read `CHOPIN_SHUTDOWN_TIMEOUT_MS` (default 30000, minimum 1000) once at startup and use it as a hard deadline for draining in-flight connections after a shutdown signal. Previously the `epoll` loop hardcoded 30 s and the `io_uring` loop had no drain deadline at all.

#### chopin-auth
- **Richer `AuthError` variants** — added `Malformed(String)`, `InvalidSignature`, `InvalidAlgorithm`, `MissingKid(String)`, `NotYetValid` and a new `AuthError::http_status() -> u16` helper (401 for authentication failures, 500 for configuration/internal errors). `JwtManager::decode` and `JwksProvider::decode` now classify upstream `jsonwebtoken` errors into the appropriate variant via `AuthError::from_jwt`.
- **JWKS staleness API** — `JwksProvider::age()`, `is_stale(ttl)`, and `refresh_if_stale(ttl, fetch)` enable TTL-based key-set refresh without forcing an HTTP transport dependency on the crate (the caller supplies the fetch closure). `JwksProvider::decode` now returns `AuthError::MissingKid` for unknown `kid` values instead of the generic `InvalidToken`.
- **Cross-module integration test suite** — `crates/chopin-auth/tests/integration.rs` adds 23 tests covering PKCE verifier/challenge, password hashing roundtrip + salting, JWT expired/revoked/bad-signature/malformed classification, `TokenBlacklist` lifecycle, and JWKS parse/refresh/staleness semantics.

#### chopin-pg
- **`PgPoolConfig::from_env()`** — reads nine `CHOPIN_PG_*` environment variables: `MAX_SIZE`, `MIN_SIZE`, `MAX_LIFETIME_MS`, `IDLE_TIMEOUT_MS`, `CHECKOUT_TIMEOUT_MS`, `CONNECT_TIMEOUT_MS`, `TEST_ON_CHECKOUT`, `VALIDATION_QUERY`, `AUTO_RECONNECT`. Lifetime/idle values of `0` disable the corresponding timeout; booleans accept `1/true/yes` and `0/false/no` case-insensitively.

#### chopin-cli
- **`chopin migrate` no longer panics on non-UTF8 paths** — `migrations.rs` introduces `collect_up_migrations(dir)` and `migration_name(path)` helpers that filter unrecognized filenames instead of unwrapping `.to_str()`.

#### chopin-core (extractor & state ergonomics)
- **`ctx.body_json::<T>()`** — one-liner JSON body extraction on `Context`. Deserializes the request body into `T` via `serde_json`; returns `Err(400 Bad Request)` on failure. Eliminates the verbose `ctx.extract::<Json<T>>()` match pattern and enables `?` in handlers returning `Result<Response, Response>`.
- **`ctx.query_params::<T>()`** — one-liner query string extraction on `Context`. Parses `?key=val&…` into any `serde::Deserialize` type; returns `Err(400 Bad Request)` on failure. Same `?`-compatible ergonomics as `body_json`.
- **`ctx.state::<T>()`** — typed per-thread state retrieval on `Context`. Returns `Option<T>` (clone of the stored value). The idiomatic way to access resources initialised in `with_worker_init` (e.g. `Arc<DbPool>`).
- **Thread-local state system** (`chopin_core::state`) — `set_state<T>(val)`, `get_state::<T>()`, and `with_state::<T, F, R>(f)` provide a type-safe, zero-synchronisation per-thread key-value store. Replaces the `lazy_static!` + global pattern for per-worker resources. Re-exported at crate root: `chopin_core::{set_state, get_state, with_state}`.
- **Closure middleware** (`BoxedMiddleware`, `Router::layer_fn`, `Router::layer_path_fn`) — new `BoxedMiddleware = Arc<dyn Fn(Context, BoxedHandler) -> Response + Send + Sync>` type enables middleware closures that capture environment (e.g. `Arc<State>`, config values). `router.layer_fn(|ctx, next| { … })` for global closure middleware; `router.layer_path_fn(path, |ctx, next| { … })` for path-scoped closure middleware. Fully compose with existing fn-pointer middlewares. `BoxedHandler` and `BoxedMiddleware` re-exported at crate root.
- **`Chopin::layer(mw)`** — builder method to attach a global fn-pointer middleware when using the high-level `Chopin` builder.
- **`Chopin::layer_fn(f)`** — builder method to attach a global closure middleware to the `Chopin` builder. Accepts any `Fn(Context, BoxedHandler) -> Response + Send + Sync + 'static`.
- **`IntoResponse` re-exported** at crate root so user crates no longer need to import from `chopin_core::http`.

#### chopin-macros
- **`Result`-returning handlers** — `#[get]`, `#[post]`, and all route macros now generate an internal trampoline that calls `IntoResponse::into_response()` on the handler's return value. Handlers may now return `Response`, `Result<Response, E>`, or any type that implements `IntoResponse`. This makes `?` propagation work natively without any wrapper macro.

### Changed
- **Default `MAX_REQUEST_SIZE` raised from 1 MiB to 4 MiB** — better default for modern JSON REST APIs. Override via `CHOPIN_MAX_REQUEST_SIZE` env var or `.with_max_request_size(bytes)`.

---

## [0.5.31] — 2026-05-21

### Added

#### chopin-pg
- **Thread-local pool API** — `chopin_pg::init_pool(url, size) -> PgResult<()>` initialises a per-thread `PgPool` (stored in a `Cell<*mut PgPool>` thread-local). `chopin_pg::pool() -> &'static mut PgPool` returns the pool with zero synchronisation cost. Designed to be called from `Chopin::with_worker_init`. Re-initialising replaces the old pool cleanly.

#### chopin-core
- **`ctx.param_parse::<T>()`** — typed path-parameter extraction on `Context`. Parses the raw segment with `std::str::FromStr`; returns `Err(400 Bad Request)` automatically when the parameter is absent or cannot be parsed. Eliminates `.unwrap()` / `parse().unwrap()` boilerplate in every handler.
- **`Chopin::with_worker_init(f)`** — builder method that accepts a `Fn() + Send + Sync + 'static` closure and runs it once inside each spawned worker thread before the event loop starts. The idiomatic hook for `chopin_pg::init_pool` or any other per-thread resource initialisation. Mirrored on `Server::with_worker_init` for manual `Server` usage.

#### chopin-macros
- **`#[derive(IntoResponse)]`** — proc-macro derive for error enums. Each variant is annotated with `#[status(N)]` (defaults to `500` if absent); the macro generates `impl From<YourError> for chopin_core::http::Response`. Enables ergonomic `e.into()` conversion in handlers and `?` propagation in service functions.

#### chopin-cli
- **`chopin generate scaffold <Name> field:type …`** — generates a complete, production-ready CRUD resource: `#[derive(Model)]` struct, `Create`/`Update` DTOs, type-safe service layer (using `chopin_pg::pool()`), five REST handlers (index / show / create / update / destroy) wired with `ctx.param_parse`, an `#[derive(IntoResponse)]` error enum, and timestamped up/down migrations. No TODO stubs.
- **`chopin db seed`** — runs seed data by invoking `cargo run -- --seed` with `CHOPIN_SEED=1`, following Chopin's synchronous binary model.

---

## [0.5.27] — 2026-05-08

### Security
- **`rustls-webpki` 0.103.12 → 0.103.13** — patched reachable panic in certificate revocation list parsing (RUSTSEC-2026-0104 / CVE). Affects the `tls` feature of `chopin-core` and `chopin-pg`.

### Changed
- Removed unused `indicatif` dependency from `chopin-cli` (eliminates RUSTSEC-2025-0119 `number_prefix` unmaintained warning).
- Added `.cargo/audit.toml` to formally document accepted advisories that cannot currently be resolved upstream (transitive `fxhash`, `paste`; dev-only `rand 0.8.5`; unmaintained `rustls-pemfile`).
- Applied `cargo fmt` across the workspace (formatting-only, no behaviour change).
- Fixed two `cargo clippy -D warnings` findings in `chopin-core`:
  - `explicit_auto_deref`: `&mut *buf` → `&mut buf` in `Response::json`.
  - `implicit_saturating_sub`: manual bounds check replaced with `total.saturating_sub(suffix)` in `Response::file_range`.

---

## [0.5.30] — 2026-05-21

### Added

#### chopin-auth
- **`StandardClaims<R>`** — generic, batteries-included claims struct (`sub`, `jti`, `exp`, `iat`, `role: Option<R>`, `scope: Option<String>`). Implements `HasJti`, `RoleCheck<R>`, and `ScopeCheck` out of the box. `StandardClaims::new(sub, ttl_secs, role, scope)` auto-generates a unique `jti` (atomic counter + unix timestamp) and sets `iat`/`exp`. Eliminates claims boilerplate for the vast majority of projects.

#### chopin-macros
- **`#[require_role(ClaimsType, role_expr)]`** — inline RBAC guard attribute macro. Decodes the `Authorization: Bearer` token via `GLOBAL_JWT_MANAGER`, checks the role with `RoleCheck::has_role`, and short-circuits with `401`/`403` before the handler body executes. Zero heap allocations — no closures, no middleware chain overhead.
- **`#[require_scope(ClaimsType, "scope")]`** — inline OAuth 2.0 scope guard. Same pattern as `#[require_role]` but delegates to `ScopeCheck::has_scope`.
- Both guards must be placed **above** the route macro (`#[get]`, `#[post]`, …) so the wrapper is applied before the handler is registered in the inventory.

#### chopin-cli
- **`chopin generate auth`** — scaffolds a complete authentication module: `src/apps/auth/{mod,models,handlers,services,errors}.rs` and a `migrations/<ts>_create_users/{up,down}.sql` migration. Pre-wired to `StandardClaims<Role>` and `PasswordHasher`; DB query stubs marked with `TODO` comments.

---

## [0.5.29] — 2026-05-21

### Added

#### chopin-core
- **Response compression** — `Response::gzip()` behind the `compression` feature flag (flate2)
- **Structured logging** — `tracing` façade always included; `tracing-subscriber` (JSON format, `RUST_LOG`-aware) behind `logging` feature; `Chopin::with_logging()` builder method; startup/shutdown/worker events instrumented
- **Prometheus `/metrics` endpoint** — `Chopin::with_metrics(path)` mounts a Prometheus text-format scrape endpoint aggregating per-worker counters (`requests_total`, `active_connections`, `bytes_sent_total`, `uptime_seconds`) across all workers
- **Built-in `/health` endpoint** — `Chopin::with_health(path)` returns `{"status":"ok","uptime_secs":…,"workers":…,"requests":…,"active_connections":…,"bytes_sent":…}` for Kubernetes probes and AWS ALB health checks
- **TLS/HTTPS server** — `tls` feature flag adds `rustls` TLS 1.2/1.3 termination directly in the epoll worker; `Server::with_tls(cert, key)` and `Chopin::serve_tls(addr, cert, key)` builder APIs; TLS-aware read/write/writev/sendfile paths with `Conn::tls_session`; supports AWS ACM private CA bundles; `Chopin::serve_tls()` entry point
- **Public API documentation** — doc comments and examples on `Router`, `Context`, `Response`, `Chopin`, `Server`, `FromRequest`, `Json`, `Query`, `Body`, `Method`, `IntoResponse`
- **Usage guide** — database integration section covering `chopin-pg` and `chopin-orm`

#### chopin-pg
- **TLS/SSL support** — `SslMode` (disable/prefer/require), TLS negotiation, `TlsStream` wrapper
- **MD5 authentication** — RFC 1321 hash for `AuthenticationMD5Password`
- **BIT / VARBIT types** — encode/decode as `Vec<u8>` bit vectors
- **MACADDR8 (EUI-64)** — `[u8; 8]` encode/decode
- **Array OID coverage** — `UUID_ARRAY`, `JSONB_ARRAY`, `JSON_ARRAY` OID constants

#### chopin-orm
- **`SoftDelete` trait** — `soft_delete()`, `restore()`, `find_active()`, `find_with_trashed()`, `find_only_trashed()` for models with a `deleted_at` column
- **`batch_insert()`** — insert a `Vec<M>` in a single multi-row `INSERT … VALUES` round-trip with `RETURNING` for server-generated columns
- **`Condition` re-export** — `pub use builder::Condition` for complex WHERE clauses

#### chopin-auth
- **OAuth PKCE helpers** — `code_verifier()`, `code_challenge_s256()` (zero external deps, custom SHA-256)
- **`AuthorizationUrl` builder** — construct OAuth 2.0 authorization URLs with PKCE, state, and scopes
- **`token_pair()`** — issue access + refresh JWT pair from a `JwtManager`
- **`ScopeCheck` trait** — `has_scope(&self, scope: &str) -> bool`
- **`require_scope_middleware!` macro** — scope-based authorization middleware (mirrors `require_role_middleware!`)

#### chopin-cli
- **Hot-reload** (`chopin dev`) — auto-detects `cargo-watch` for live reloading, falls back to `cargo run`
- **Model generator** (`chopin generate model`) — scaffolds a `#[derive(Model)]` struct + timestamped SQL migrations from `name:type` field definitions
- **Enhanced checks** (`chopin check`) — validates config, database connectivity (with URL masking), and project structure; formatted summary table

### Changed
- `chopin-core` — thread-per-core worker model now pins threads to CPU cores via `core_affinity`
- `chopin-pg` — connection handshake negotiates TLS when `sslmode=prefer` or `sslmode=require`
- `chopin-orm` — `build_query` visibility changed to `pub(crate)` for internal testing

### Fixed
- `chopin-core` — `Connection-close` header handling; partial-write loop for large responses
- `chopin-core` — `Content-Length` correctness for `Body::Static` variant
- `chopin-core` — timer-wheel slot collision under high concurrency
- `chopin-core` — E0499 borrow checker error in request pipeline: transmute `Request<'_>` to `Request<'static>` to allow second `slab.get_mut()` in response serialization; `ConnectionSlab` uses heap-pinned `Box<[Conn]>` so buffers remain valid across the full event-loop iteration
- `chopin-core` — undefined `next_state` and out-of-scope variable references in `Body::Raw` handler
- `chopin-core` — unused import warning on io-uring builds: gate `use crate::syscalls` with `#[cfg(not(io-uring))]`
- `chopin-pg` — statement cache eviction race under connection reuse
- `chopin-pg` — **response buffer overflow**: `message_complete()` now returns `Result<Option<usize>, PgError>` instead of `Option<usize>`; a server message whose length field exceeds `MAX_MESSAGE_SIZE` (16 MB) returns `Err(PgError::BufferOverflow)` and is propagated immediately through all read loops — previously the driver looped forever waiting for data that never arrived. `ensure_read_space()` also guards against OOM by skipping buffer growth when the advertised length exceeds the limit.

---

## [0.5.x] — Prior Releases

See [docs/releases/RELEASE_NOTES_0.5.x.md](docs/releases/RELEASE_NOTES_0.5.x.md) for earlier changes.
