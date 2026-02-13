# Performance

**Last Updated:** February 2026

## Quick Start: Make It Fast

To maximize Chopin performance, use this checklist:

```bash
# 1. Start with Performance mode and release build
SERVER_MODE=performance cargo run --release --features perf

# 2. Benchmark
wrk -t4 -c256 -d10s http://127.0.0.1:3000/json
```

Expected throughput: **~600K req/s** on 8-core hardware (vs ~300K in standard mode).

---

## Server Modes

Chopin offers **two server modes** for different performance/flexibility tradeoffs:

```bash
# Standard mode (default) — full middleware, easy development
cargo run

# Performance mode — raw hyper, multi-core, zero-alloc FastRoutes
SERVER_MODE=performance cargo run --release --features perf
```

### Mode Comparison

| Metric | Standard | Performance |
|--------|----------|-------------|
| **Per-request cost** | ~800ns | ~450ns |
| **Throughput (JSON)** | ~300K req/s | ~600K+ req/s |
| **Middleware** | ✅ Full | ✅ Full |
| **Axum Router** | ✅ Yes | ✅ Fallback |
| **FastRoute endpoints** | Via Axum | Via hyper (zero alloc) |
| **Use case** | Development | Production |

---

## What Makes It Fast? The Layer-By-Layer Breakdown

Each mode removes overhead from the request path:

```
Standard (800ns):
  TCP receive → Axum route matching (200ns) → Middleware (300ns) 
  → Response building (150ns) → HeaderMap (50ns) → Date cache (8ns) 
  → Write syscall (200ns)

Performance (450ns):  
  TCP receive → Hyper HTTP parser (100ns) → Route match (5ns)
  → Response building (150ns) → HeaderMap clone (25ns) → Date cache (8ns)
  → Write syscall (200ns)
```

**Key optimizations in Performance mode:**
- **SO_REUSEPORT**: Kernel distributes connections across all CPU cores
- **FastRoute zero-alloc**: Pre-built headers, `ChopinBody::Fast` avoids `Box::pin`
- **Lock-free Date cache**: `AtomicU64` + `thread_local!` (~8ns vs ~25ns RwLock)
- **mimalloc**: 10-20% better throughput under high concurrency
- **serde_json `to_writer`**: Writes directly into pre-allocated buffer (avoids intermediate Vec)

## Performance Mode Deep Dive

### Multi-Core Accept (SO_REUSEPORT)

In performance mode, Chopin creates **N TCP listeners** (one per CPU core) on the same port using `SO_REUSEPORT`. The kernel distributes incoming connections across all cores, eliminating the single accept-loop bottleneck.

```
Core 0: TcpListener → accept → spawn connection handler
Core 1: TcpListener → accept → spawn connection handler
Core 2: TcpListener → accept → spawn connection handler
...
Core N: TcpListener → accept → spawn connection handler
```

### Zero-Allocation Fast Routes

Register static response endpoints via the `FastRoute` API — they bypass Axum entirely:

```rust
use chopin_core::{App, FastRoute};

let app = App::new().await?
    .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!"))
    .fast_route(FastRoute::html("/health", b"OK"));
app.run().await?;
```

Under the hood, `FastRoute` pre-computes everything at registration time:
- **Body:** `Bytes::from_static` embedded in binary's `.rodata` section, stored as `ChopinBody::Fast(Option<Bytes>)` inline — **zero heap allocation** (avoids the `Box::new(Full::new(bytes))` that `Body::from(Bytes)` does)
- **Headers:** Individual `HeaderValue`s stored. At request time, built directly on the response — **no `HeaderMap` clone**. `Content-Type` from `from_static` is a pointer-copy (~8 bytes).
- **Date header:** Only header inserted per-request, cached and updated every 500ms

The `ChopinFuture::Ready` variant returns the response inline without `Box::pin` heap allocation.

### Lock-Free Date Header Caching

The `Date` HTTP header uses a **lock-free thread-local cache** updated every 500ms:

- Background task atomically increments an epoch counter (u64)
- Each thread maintains its own (epoch, HeaderValue) cache
- Hot path: 1 relaxed atomic load + thread-local lookup = **~8ns**
- Cold path: format date once per thread per 500ms
- **Zero cross-thread synchronization** — no RwLock, no Arc increment

### TCP Optimizations

| Setting | Value | Why |
|---------|-------|-----|
| `TCP_NODELAY` | `true` | Disable Nagle's algorithm for small responses |
| Backlog | `16384` | Handle burst connections (increased from 8192) |
| `SO_REUSEADDR` | `true` | Quick port reuse after restart |
| `SO_REUSEPORT` | `true` | Kernel-level load balancing across cores |

### HTTP/1.1 Tuning

| Setting | Value | Why |
|---------|-------|-----|
| `keep_alive` | `true` | Reuse connections |
| `pipeline_flush` | `true` | Flush responses immediately for pipelined requests |
| `half_close` | `false` | Skip unnecessary half-close handling (saves 1 syscall) |
| `max_buf_size` | `16384` | Increased from 8KB for larger headers (fewer read syscalls) |

---

## mimalloc Allocator

Enable the `perf` feature to use Microsoft's [mimalloc](https://github.com/microsoft/mimalloc) as the global allocator:

```bash
cargo run --release --features perf
```

mimalloc outperforms the system allocator under high concurrency:
- ~10-20% throughput improvement
- Lower allocation latency
- Better memory locality

## Compilation Optimizations

### Release Profile

```toml
[profile.release]
opt-level = 3        # Maximum optimization
lto = "fat"          # Full link-time optimization across all crates
codegen-units = 1    # Single codegen unit for maximum optimization
strip = true         # Remove debug symbols (smaller binary)
panic = "abort"      # No unwinding overhead
```

### CPU-Specific Targeting

Create `.cargo/config.toml` in your project:

```toml
# For Apple Silicon (M1/M2/M3/M4)
[target.'cfg(target_arch = "aarch64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]

# For x86_64 Linux/macOS servers (Intel/AMD)
[target.'cfg(target_arch = "x86_64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+avx2,+aes,+sse4.2"]
```

This enables better code generation:
- **NEON** on ARM (aarch64)
- **AVX2/SSE** on x86_64

## JSON Serialization

Chopin uses **serde_json** with `to_writer` optimization for all JSON operations:

- `ApiResponse::into_response()` → `serde_json::to_writer()` into pre-allocated buffer
- `ChopinError::into_response()` → `serde_json::to_writer()` into pre-allocated buffer
- `Json` extractor → `serde_json::from_slice()` (zero-copy deserialization)
- `Json` response → `serde_json::to_writer()` into pre-allocated buffer

### Why serde_json over sonic-rs?

- **Stability**: serde_json is battle-tested across the entire Rust ecosystem
- **Compatibility**: Works with every serde derive, no edge cases
- **Optimized path**: Using `to_writer` with pre-allocated `Vec` avoids the extra allocation that `to_vec` performs
- **Real-world**: For production APIs, the JSON serialization cost (~100ns) is dwarfed by database queries (~1ms+)

## Benchmarking

### With wrk

```bash
# Start the server
SERVER_MODE=performance cargo run --release --features perf

# JSON benchmark
wrk -t4 -c256 -d10s http://127.0.0.1:3000/json

# Plaintext benchmark
wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext
```

### With bombardier

```bash
bombardier -c 256 -d 10s http://127.0.0.1:3000/json
bombardier -c 256 -d 10s http://127.0.0.1:3000/plaintext
```

## Performance Comparison

| Feature | Standard Mode | Performance Mode |
|---------|--------------|------------------|
| Server layer | `axum::serve` | Raw `hyper::http1` |
| Accept loops | 1 | N (per CPU core) |
| SO_REUSEPORT | No | Yes |
| Fast routes | Through Axum + middleware | `FastRoute` API (zero alloc) |
| Future type | Axum internal | `ChopinFuture` enum (no `Box::pin`) |
| Allocator | System | mimalloc (with `perf`) |
| Date header | Per-request | Cached (500ms, lock-free `AtomicU64` + `thread_local!`) |
| Middleware on fast routes | Full stack | None |
| Best for | Development, typical API | Benchmarks, extreme throughput |

---

## Production Recommendations

For **real-world production APIs**, here's the recommended approach:

### 1. Use Performance Mode (Default)
Performance mode gives you **600K+ req/s** while maintaining full Axum compatibility:

```bash
SERVER_MODE=performance cargo build --release --features perf
```

### 2. Enable Compiler Optimizations

Create `.cargo/config.toml` in your project root:

```toml
[build]
jobs = 4  # Adjust to your CPU core count

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"

# Target your deployment CPU
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+avx2,+aes"]

[target.aarch64-unknown-linux-gnu]  
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+neon,+aes"]
```

### 3. OS Tuning

On Linux servers, increase file descriptor limits and backlog:

```bash
# Increase open files per process
ulimit -n 65536

# Increase TCP backlog (requires root)
sudo sysctl -w net.core.somaxconn=65536
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65536
```

### 4. Database Optimization

For real APIs, the database often becomes the bottleneck:

```rust
use sqlx::postgres::PgPool;

let pool = PgPool::connect(&database_url).await?;

// Set connection pool size to 2-3x your CPU cores
let pool = PgPoolOptions::new()
    .max_connections(24)  // 8 cores × 3
    .connect(&database_url)
    .await?;
```

**Pro tip:** Add Chopin's built-in caching for frequently-accessed data:

```rust
use chopin_core::cache::Cache;

let cache = Cache::new();
cache.set("user:123", user_data, Duration::from_secs(300)).await;
```

### 5. Monitoring Throughput

Deploy with these metrics:

```bash
# Log requests per second
ENVIRONMENT=production SERVER_MODE=performance cargo run --release --features perf
```

Monitor with `wrk` in production-like conditions:

```bash
wrk -t8 -c512 -d60s http://api.example.com/api/endpoint
```

Expected results for different scenarios:

| Endpoint Type | Performance Mode | Requirements |
|---------------|-----------------|--------------|
| Simple JSON response | ~600K+ req/s | FastRoute endpoint |
| Database query | ~50-200K req/s | DB connection pool |
| Cached response | ~400K+ req/s | Cache hit |

---

## Checklist for Maximum Performance

1. [ ] `SERVER_MODE=performance`
2. [ ] `--release` build
3. [ ] `--features perf` (mimalloc)
4. [ ] `.cargo/config.toml` with `target-cpu=native`
5. [ ] `ENVIRONMENT=production` (disables tracing middleware)
6. [ ] Tune OS: `ulimit -n 65536`, sysctl `net.core.somaxconn=65536`
7. [ ] Database connection pool tuned (2-3x CPU cores)
8. [ ] Benchmark with `wrk` or `bombardier`
