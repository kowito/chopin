# chopin-pg Benchmarks

This document describes benchmarking methodology and setup for comparing `chopin-pg` against other PostgreSQL drivers.

## Overview

Four PostgreSQL drivers are compared:

| Driver | I/O Model | Runtime | Thread Model |
|--------|-----------|---------|--------------|
| **chopin-pg** | Sync non-blocking (epoll/kqueue) | None | Thread-per-core, shared-nothing |
| **sqlx** | Async + await | tokio | Tokio threadpool |
| **tokio-postgres** | Async + await | tokio | Tokio threadpool |
| **monoio-pg** | Async + await | monoio | Thread-per-core with async |

## Architecture & Design Tradeoffs

### chopin-pg
- **Zero external runtime dependencies** — only `libc` for syscalls
- **Synchronous API** — blocking calls from worker thread perspective, non-blocking I/O underneath
- **Shared-nothing pool** — each thread owns its connections; no `Arc`, no `Mutex`
- **Best for**: Thread-per-core servers (like Chopin); workloads with low-to-moderate concurrency per connection

### sqlx
- **Full async/await stack** — requires tokio runtime
- **Connection pooling** — `Arc`-based pool with `Mutex` syncing between workers
- **Flexibility** — works with any async code, multiple runtimes (limited support)
- **Best for**: High-concurrency async applications; integration with existing tokio ecosystem

### tokio-postgres
- **Async native** — built for tokio
- **Copy protocol support** — efficient bulk loading
- **Lowest-level async API** — fine-grained control over tasks and connections
- **Best for**: Low-latency requirements; applications requiring fine-tuned async behavior

### monoio-pg
- **Thread-per-core + async** — hybrid model
- **Minimal overhead** — no Mutex, minimal context switching
- **Niche ecosystem** — smaller community than tokio
- **Best for**: Thread-per-core architectures with async I/O preference

---

## Benchmark Scenarios

### 1. Simple Query (`bench_pg.rs` / `bench_sqlx.rs` / `bench_tokio_postgres.rs`)

**Scenario**: Repeated `SELECT 1` and `SELECT $1::int4 + $2::int4`

**Measures**:
- Per-request throughput (req/s)
- Average latency (µs)
- Protocol overhead

**Expected Results**:
```
chopin-pg:       ~100,000+ req/s (single-threaded)
tokio-postgres:  ~80,000 req/s
sqlx:            ~70,000 req/s
```

**Why**:
- chopin-pg has minimal allocation and zero async overhead
- sqlx/tokio-postgres must poll, wake tasks, potentially context-switch
- For simple queries, driver overhead dominates

---

### 2. CRUD Benchmark (`bench_crud.rs`)

**Scenario**:
1. Bulk insert via COPY (1K, 100K, 1M rows)
2. 10,000 point SELECTs
3. 10,000 point UPDATEs
4. 10,000 DELETEs

**Measures**:
- COPY throughput (rows/s)
- Point query throughput (req/s)
- Write throughput (req/s)

**Expected Results**:

| Operation | Scale | chopin-pg | tokio-postgres |
|-----------|-------|-----------|-----------------|
| COPY | 1M rows | 50K-100K rows/s | 40K-80K rows/s |
| SELECT | 10K queries | 40K-60K req/s | 30K-50K req/s |
| UPDATE | 10K queries | 25K-40K req/s | 20K-30K req/s |
| DELETE | 10K queries | 25K-40K req/s | 20K-30K req/s |

**Why**:
- COPY is highly optimized; chopin-pg's zero-copy write helps
- Point queries show the cost of statement caching + protocol
- Writes are slower; transaction overhead matters

---

### 3. Concurrent Load (Implicit in multi-worker setups)

**Scenario**: Multiple threads/tasks each running 10K queries

**Measures**:
- Total aggregate throughput
- Connection pool overhead
- Lock contention (for async drivers)

**Expected Results**:
- **chopin-pg**: Linear scaling with threads (no lock contention)
- **tokio-postgres**: Sublinear beyond a few threads (pool lock overhead)
- **sqlx**: Similar to tokio-postgres; lower from Mutex plus task scheduling

---

## Running the Benchmarks

See [`running-pg-benchmarks.md`](./running-pg-benchmarks.md) for setup and execution instructions.

---

## Fair Comparison Guidelines

### Pool Configuration

Each driver should use a **single connection** for single-threaded benchmarks:
- chopin-pg: `PgConnection::connect()` directly
- sqlx: `max_connections(1)`
- tokio-postgres: single client
- monoio-pg: single client

### Multi-threaded Benchmark

If running concurrent workers:
- **chopin-pg**: N threads, each with its own `PgConnection` (no pool)
- **tokio-postgres**: `N` tasks with shared 1-connection "pool" (simulated with `Arc<Mutex>`)
- **sqlx**: N tasks with 1 connection pooled
- **monoio-pg**: N monoio tasks, single I/O worker

---

## Performance Expectations

### Single Query (`SELECT 1`)

**chopin-pg advantage**: ~15-25% faster
- No async overhead
- Simple syscall → parse → execute → return

### Bulk Insert (COPY)

**chopin-pg advantage**: ~20-30% faster
- Zero-copy internal buffer management
- No async task scheduling overhead per batch
