# Changelog

All notable changes to the Chopin framework are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

### Added

#### chopin-core
- **Response compression** — `Response::gzip()` behind the `compression` feature flag (flate2)
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

---

## [0.5.x] — Prior Releases

See [docs/releases/RELEASE_NOTES_0.5.x.md](docs/releases/RELEASE_NOTES_0.5.x.md) for earlier changes.
