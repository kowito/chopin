# Chopin Release Notes: v0.5.0 – v0.5.12 (Codename: Nocturne)

The 0.5.x series, codenamed **Nocturne**, marks the most significant evolution of Chopin to date. We've transitioned from a proof-of-concept networking engine to a production-hardened web framework with industry-leading performance and modern ergonomics.

## 🚀 Performance: The Optimization Milestone
The core engine has been completely rebuilt for linear scaling on multi-core systems.
- **SO_REUSEPORT Core**: Implementation of kernel-level load balancing for zero-contention socket accept.
- **Slab Allocation**: $O(1)$ connection management using a custom slab allocator.
- **Zero-Copy Serialization**: Integrated `kowito-json` Schema-JIT engine achieving >30 GiB/s serialization.
- **Validated Benchmarks**: Fresh comparisons show Chopin outperforming Actix Web and Axum by 10-15% and Hyper by ~40% in raw throughput.

## 🎼 Ergonomics: Attribute-Based Routing
We have moved away from manual router registration to a declarative, macro-based system.
- **Implicit Discovery**: Using `#[get]`, `#[post]`, etc., handlers are now automatically discovered and mounted at startup via `Chopin::mount_all_routes()`.
- **Zero Runtime Overhead**: Route collection happens at compile-time/startup using the `inventory` pattern, maintaining our commitment to maximum efficiency.

- **Chopin CLI**: A new unified CLI tool for project scaffolding, architectural linting (`chopin check`), and production deployment (`chopin deploy docker`).
- **Authorization & Security**: Initial release of `chopin-auth` featuring zero-allocation JWT validation and role-based middleware.
- **Premium Documentation**: A new landing page and documentation site featuring a high-fidelity design.
- **Production Hardening**: Exhaustive cleanup of clippy lints, formatting, and unit tests across the workspace.
- **CI/CD Automation**: Fully automated version bumping and publishing to crates.io.

## 💎 New Component: Chopin ORM
The release of `chopin-orm` brings type-safe database interactions to the ecosystem.
- **Zero-Overhead Driver**: Built on `chopin-pg`, our raw syscall-based PostgreSQL driver.
- **Type-Safe Query Builder**: Compile-time checked queries without the performance penalty of traditional ORMs.
- **Linear Scaling**: The ORM matches the raw driver performance across 1k, 100k, and 1M row benchmarks.

## v0.5.12 — chopin-pg & chopin-orm Maturity

### 🔧 chopin-pg: Production-Grade PostgreSQL Driver
The `chopin-pg` crate has reached **91% completion** (145/160 items) with major additions:

- **22 PostgreSQL types** — Bool, Int2/4/8, Float4/8, Text, Bytes, Json, Jsonb, Uuid, Date, Time, Timestamp, Timestamptz, Interval, Inet, Numeric, MacAddr, Point, Range, Array
- **Binary wire format** — per-parameter format codes with binary encoding/decoding for all supported types
- **SCRAM-SHA-256 auth** — zero-external-dependency implementation
- **Connection pool** — `PgPool` with `PgPoolConfig` builder (max_size, min_size, checkout_timeout, idle_timeout, max_lifetime, test_on_checkout, auto-reconnect)
- **COPY protocol** — streaming `CopyWriter`/`CopyReader` for bulk data operations
- **LISTEN/NOTIFY** — async notification with buffered delivery and `poll_notification()`
- **Transactions** — savepoints, nested transactions, closure-based API with auto-rollback
- **Statement cache** — LRU eviction with configurable capacity
- **Unix domain sockets** — `PgConfig.socket_dir` and `?host=` URL parameter
- **Production hardening** — broken connection flag, TCP_NODELAY, zero-copy writes, `Rc<ColumnDesc>` sharing, MAX_MESSAGE_SIZE enforcement
- **362 unit tests + 44 integration tests** against real PostgreSQL

### 🔧 chopin-orm: Type-Safe ORM with Column DSL
The `chopin-orm` crate now provides a comprehensive type-safe ORM:

- **`#[derive(Model)]` macro** — generates `FromRow`, column enum (`UserColumn`), CRUD methods, and relationship accessors
- **`Validate` trait** — supertrait of `Model` with default pass-through; custom validation rules supported
- **Type-safe column DSL** — `UserColumn::name.eq("Alice")`, `.gt()`, `.lt()`, `.like()`, `.is_in()`, `.is_null()`, etc.
- **Condition combinators** — `.and()` / `.or()` for complex filter trees
- **Relationships** — `has_many(Target, fk = "col")` and `#[model(belongs_to(Parent))]` with lazy loading and JOIN queries
- **Auto-migration** — `sync_schema()` diffs and creates/alters tables automatically
- **Pagination** — `QueryBuilder::paginate(size).page(n).fetch()` returns `Page<M>` with totals
- **ActiveModel** — `ActiveModel::from_model()` / `.new_insert()` with `.set()` for partial updates
- **Upsert** — `INSERT ... ON CONFLICT` support
- **Aggregations** — `select_only`, `group_by`, `having` for aggregate queries
- **Mock executor** — `MockExecutor` + `mock_row!` for unit testing
- **Migration system** — `MigrationManager` with `up`/`down` for production migrations

### 📖 Documentation Overhaul
- Updated all examples and benchmarks to use current API (`Validate` trait, type-safe column DSL, correct `ActiveModel` constructors)
- Rewrote `chopin-pg/README.md` with comprehensive feature list and code examples for every feature
- Rewrote `chopin-orm/README.md` with relationships, pagination, ActiveModel, and MockExecutor examples
- Rewrote `chopin-orm/docs/advanced_guide.md` to use correct `#[model(...)]` syntax (not `#[derive(Relation)]`)
- Updated `docs/USAGE_GUIDE.md` ORM section with type-safe DSL, relationships, and correct row accessor methods
- Updated `docs/DEVELOPER_GUIDE.md` ORM integration section

## v0.5.9 — Documentation & Website Update

### 📖 Comprehensive User Manual (Website)
`docs/index.html` and `docs/developer_guide.html` have been rewritten to serve as a full user manual rather than a marketing overview.

- **Extractors**: Corrected `Json<T>` and `Query<T>` examples with accurate `ctx.extract::<T>()` API; removed stale `KJson` / `#[derive(Default)]` pseudocode.
- **Zero-Copy File Serving section**: New dedicated section covering `Response::file(path)`, `Response::sendfile(fd, offset, len, content_type)`, and a complete MIME type reference table.
- **Response API reference**: Full constructor table (11 methods with status codes), `with_header()` builder chain examples, and a Context helpers quick-reference block.
- **Developer Guide expanded**: Added Extractors as a standalone section (#3), expanded Routing section with all HTTP methods, wildcard paths, and manual registration; expanded Performance section from 2 items to 6 cards (writev, sendfile, pre-composed middleware, mimalloc, core affinity, kernel socket handoff).
- **API accuracy pass**: Fixed `ctx.respond_json` → `ctx.json`, `response.headers.push` → `response.with_header()`, toolchain `"1.75+"` → `"nightly"`, and all `0.5.8` version references throughout.

## v0.5.8 — Zero-Copy I/O & Hot-Path Optimization

### ⚡ Zero-Copy Response Body (`writev`)
Response headers and body are now delivered in a single `writev` syscall. Static and allocated byte bodies are no longer copied into the write buffer; instead, Chopin retains a raw pointer and passes both `iovec` slices directly to the kernel. This eliminates the largest memcpy on the hot path.

### ⚡ Zero-Copy File Serving (`sendfile`)
`Response::file(path)` opens a file and streams its contents via the platform `sendfile` syscall (Linux and macOS). The file bytes never enter user space. Content-Type is inferred automatically from the file extension across ~30 MIME types.

### 🎼 Pre-Composed Middleware Chains
Middleware chains are now resolved once when the router is finalized (`Router::finalize()`). Each route stores a single pre-built `Arc<dyn Fn(Context) -> Response>`. On the hot path, Chopin invokes one pre-built closure — no `Arc::new`, no chain construction, zero per-request allocations for middleware.

### 📅 Real-Time Date Header
Every HTTP response now carries a compliant RFC 7231 `Date` header computed fresh per request using a fast hand-rolled formatter. No stale cached timestamps.

### 🚀 mimalloc Global Allocator
`mimalloc` is now the global allocator for all workers. Under high-concurrency workloads this significantly reduces allocation latency compared to the system allocator by using per-thread free lists.

### 🧹 Dependency Cleanup
- `kowito-json` bumped to `0.2.11`.
- Removed duplicate dev-dependencies from `chopin-core`.
- Aligned `chopin-cli` to workspace-level `serde` / `serde_json`.

---

## 📝 Version Summary
- **v0.5.0**: Groundwork for `SO_REUSEPORT` and performance hardening.
- **v0.5.1 - v0.5.2**: Launch of `chopin-orm` and `chopin-pg`.
- **v0.5.3**: Refactoring to Attribute Macros and Implicit Routing.
- **v0.5.4**: CI/CD optimization and benchmark synchronization with Actix/Axum.
- **v0.5.5 - v0.5.6**: Documentation update and ORM performance tuning.
- **v0.5.7**: Major Naming Convention Overhaul (`Response::text`, `ctx.param`, `router.layer`).
- **v0.5.8**: Zero-copy I/O (writev + sendfile), pre-composed middleware chains, mimalloc integration.
- **v0.5.9**: Comprehensive website update — full user manual with accurate extractors, file serving reference, Response API table, and expanded developer guide.
- **v0.5.10 - v0.5.11**: chopin-pg type system expansion (MacAddr, Point, Range, binary codecs), production hardening (broken conn flag, TCP_NODELAY, zero-copy writes), 362 unit tests.
- **v0.5.12**: chopin-orm DSL maturity (Validate trait, type-safe column DSL, relationships, pagination, ActiveModel), documentation overhaul.

---
"Simple as a melody, fast as a nocturne."
