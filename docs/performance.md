# Performance (v0.1.2)

**Current Version:** 0.1.2 | **Last Updated:** February 2026

## Server Modes

Chopin offers **three server modes** for different performance/flexibility tradeoffs:

```bash
# Standard mode (default) — full middleware, easy development
cargo run

# Performance mode — raw hyper, multi-core, zero-alloc FastRoutes
SERVER_MODE=performance cargo run --release --features perf

# Raw mode — hyper completely bypassed, maximum possible throughput
SERVER_MODE=raw cargo run --release --features perf
```

### Mode Comparison

| Metric | Standard | Performance | Raw |
|--------|----------|-------------|-----|
| **Per-request cost** | ~800ns | ~450ns | **~240ns** |
| **Throughput (JSON)** | ~300K req/s | ~600K req/s | **~900K+ req/s** |
| **Middleware** | ✅ Full | ✅ Full | ❌ None |
| **Axum Router** | ✅ Yes | ✅ Fallback | ❌ No |
| **FastRoute endpoints** | Via Axum | Via hyper | Via raw TCP |
| **Use case** | Development | Production | Benchmarks |

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

For Raw mode, `cached_date_bytes()` returns raw `[u8; 29]` for direct memcpy into the write buffer.

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

## Raw Mode — Ultimate Performance

**Raw mode completely bypasses hyper** and writes pre-serialized HTTP responses directly to TCP sockets. This eliminates all HTTP framework overhead for an estimated **~45% throughput improvement** over Performance mode.

### Usage

```bash
SERVER_MODE=raw cargo run --release --features perf
```

**Important:** Raw mode only serves FastRoute endpoints. There is no Axum router fallback.

```rust
use chopin_core::{App, FastRoute};

let app = App::new().await?
    .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));
app.run().await?;
```

### Architecture

```text
SO_REUSEPORT × N CPU cores
  → per-core accept loop (raw TCP)
    → TCP_NODELAY
      → loop (keep-alive):
        → read request bytes into reusable buffer
          → parse path (scan for spaces — ~10ns)
            → match route → write pre-serialized bytes (one syscall)
            → no match → write cached 404
```

### What Gets Eliminated

| Component | Performance mode (hyper) | Raw mode |
|-----------|-------------------------|----------|
| **Request parsing** | Full HTTP/1.1 (method, version, all headers) | Path only (~10ns) |
| **Response building** | `Response<ChopinBody>` + `HeaderMap` | Pre-serialized bytes |
| **Header serialization** | 4 headers → wire format (~100ns) | Pre-baked at startup |
| **Per-request allocs** | HeaderMap clone (~50ns) | **Zero** |
| **Atomics** | Arc clone × 2 per request | **None** |
| **Write buffering** | hyper manages buffering | Single `write_all()` |

### Pre-Serialized HTTP Responses

At startup, FastRoutes are converted to `RawFastRoute` with pre-serialized HTTP:

```rust
// Registration time (once):
prefix = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 27\r\nserver: chopin\r\ndate: "
suffix = "\r\n\r\n{\"message\":\"Hello, World!\"}"

// Request time (~240ns):
date = cached_date_bytes()  // [u8; 29] from thread-local cache (5ns)
buf.clear()
buf.extend_from_slice(&prefix)
buf.extend_from_slice(&date)    // Only dynamic part
buf.extend_from_slice(&suffix)
stream.write_all(&buf).await    // Single syscall (~200ns)
```

### Per-Request Cost Breakdown

| Operation | Performance mode | Raw mode |
|-----------|-----------------|----------|
| HTTP parsing | ~100ns | **~10ns** (path only) |
| Route matching | ~5ns | ~5ns |
| Response building | ~150ns | **~25ns** (memcpy) |
| Date header | ~8ns | **~5ns** (raw bytes) |
| Write syscall | ~200ns | ~200ns |
| **Total** | **~450ns** | **~240ns** |

### Expected Throughput

Based on per-request cost and typical hardware (8-core, 3GHz):

| Mode | Requests/sec | vs Axum |
|------|--------------|---------|
| Performance | ~600K | -4% |
| **Raw** | **~900K+** | **+40%** |

### Limitations

- ❌ No Axum router (FastRoute endpoints only)
- ❌ No middleware (CORS, tracing, etc.)
- ❌ No HTTP/2 support
- ❌ No request body parsing
- ❌ No dynamic routing (`/users/:id`)

### Best Use Cases

- ✅ TechEmpower benchmarks
- ✅ Health check endpoints at extreme scale
- ✅ Metrics/monitoring endpoints
- ✅ Static JSON APIs (>1M req/s target)
- ✅ High-frequency trading infrastructure

For most production APIs, **Performance mode is recommended** — it provides excellent throughput (600K+ req/s) while maintaining full Axum compatibility for middleware and dynamic routing.

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

This enables `sonic-rs` to use SIMD instructions:
- **NEON** on ARM (aarch64) for 2-4× faster JSON serialization
- **AVX2** on x86_64 for 2-4× faster JSON serialization

## JSON Serialization

Chopin uses **sonic-rs** instead of `serde_json` for all JSON operations:

- `ApiResponse::into_response()` → `sonic_rs::to_vec()`
- `ChopinError::into_response()` → `sonic_rs::to_vec()`
- `Json` extractor → `sonic_rs::from_slice()`
- Welcome endpoint → `sonic_rs::to_vec()`

sonic-rs is 2-4x faster than serde_json on ARM (NEON) and x86 (AVX2/SSE).

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
| Date header | Per-request | Cached (500ms, `std::sync::RwLock`) |
| Middleware on fast routes | Full stack | None |
| Best for | Development, typical API | Benchmarks, extreme throughput |

## Checklist for Maximum Performance

1. [ ] `SERVER_MODE=performance`
2. [ ] `--release` build
3. [ ] `--features perf` (mimalloc)
4. [ ] `.cargo/config.toml` with `target-cpu=native`
5. [ ] `ENVIRONMENT=production` (disables tracing middleware)
6. [ ] Tune OS: `ulimit -n 65536`, sysctl `net.core.somaxconn=65536`
7. [ ] Use PostgreSQL with connection pooling for real API workloads
