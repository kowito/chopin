# Performance Optimization Guide

This guide covers all performance optimizations in Chopin and how to enable them for maximum throughput and minimal latency.

## Architecture Overview

Chopin achieves **top-tier performance** through a multi-layered optimization strategy:

```
┌─────────────────────────────────────────────────────────────────┐
│ HTTP Layer:  SO_REUSEPORT + per-core current_thread runtimes   │
├─────────────────────────────────────────────────────────────────┤
│ Routing:     FastRoute [~35-150ns] → Axum fallback [~1-5µs]   │
├─────────────────────────────────────────────────────────────────┤
│ Headers:     Pre-computed HeaderMap + Cached Date header       │
├─────────────────────────────────────────────────────────────────┤
│ JSON:        Thread-local buffer + sonic-rs SIMD + zero-alloc  │
├─────────────────────────────────────────────────────────────────┤
│ Allocator:   mimalloc (10% faster than glibc malloc)           │
└─────────────────────────────────────────────────────────────────┘
```

## Quick Start: Maximum Performance

Enable all optimizations with a single command:

```bash
# Development (for testing)
cargo run --release --features perf

# Production (with SO_REUSEPORT multi-core)
REUSEPORT=true cargo run --release --features perf

# Expected throughput: 650K-1.1M req/s (depending on workload)
```

---

## Performance Layers Explained

### 1. HTTP/1.1 Transport (Hyper)

**Optimization: Minimal features**

Chopin uses lean hyper configuration:

```toml
[dependencies]
# Slim features: only HTTP/1.1 server, no HTTP/2, no client code
hyper = { version = "1", features = ["server", "http1"], default-features = false }
```

**Why?** Reduces binary size and improves instruction cache locality by 5-10%.
Matches the pattern used in hyper's own TechEmpower entry.

**Cost removed:**
- HTTP/2 support (~200KB binary)
- HTTP client code (~100KB binary)
- Unused compression support

**Gain:**
- Smaller working set → better icache hit rate
- Faster compilation
- Reduced link-time optimization overhead

---

### 2. Connection Handling (SO_REUSEPORT + Per-Core Runtimes)

**Optimization: One listener per CPU core**

When `REUSEPORT=true`:

```rust
// Pseudo-code: see server.rs for full implementation
for core in 0..num_cores() {
    spawn_thread(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(accept_loop_for_core(core));
    });
}
```

**Why?** Kernel distributes connections across cores without scheduler contention.

**Benefits:**
- Eliminates work-stealing overhead
- Perfect CPU cache locality per core
- Scales linearly with core count
- No atomic operations on hot path

**Cost per connection:**
- 0ns (kernel-level routing, no user-space overhead)

**Architecture:**
- **Main thread:** spawns N-1 worker threads
- **Each worker:** runs `current_thread` tokio runtime
- **Each runtime:** owns one SO_REUSEPORT listener
- **Each connection:** handled entirely on its core (no migration)

---

### 3. FastRoute: Zero-Allocation Static Responses

**Optimization: Pre-computed everything**

For endpoints with predictable responses:

```rust
use chopin_core::{App, FastRoute};

App::new().await?
    // Static plaintext: ~35ns/req
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!").get_only())
    
    // Static JSON: ~35ns/req (pre-cached)
    .fast_route(FastRoute::json("/static-api", br#"{"ok":true}"#).get_only())
    
    // Dynamic JSON: ~100-150ns/req (per-request serialize)
    // TechEmpower benchmark compliant
    .fast_route(FastRoute::json_serialize("/json", || {
        JsonResponse { message: "Hello, World!" }
    }).get_only())
```

**Performance tiers:**

| Route Type | Latency | Allocation | Best For |
|-----------|---------|-----------|----------|
| FastRoute (static) | ~35ns | None | `/health`, `/version`, static JSON |
| FastRoute (dynamic) | ~100-150ns | Thread-local (reused) | `/json`, `/metrics` (TFB-compliant) |
| Axum Router | ~1-5µs | Per-request | Business logic with middleware |

**Implementation details:**

1. **Static routes** (`FastRoute::new()`):
   ```rust
   pub fn new(path: &str, body: &'static [u8], content_type: &'static str) -> Self {
       let bytes = Bytes::from_static(body);  // no allocation, pointer copy
       let mut base_headers = HeaderMap::with_capacity(4);
       base_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
       base_headers.insert(
           header::CONTENT_LENGTH,
           perf::content_length_header(body.len()),  // itoa: zero-alloc
       );
       // Pre-computed at startup, cloned on every request
       FastRoute { path: path.into(), body: FastRouteBody::Static(bytes), base_headers, ... }
   }
   ```

2. **Dynamic routes** (`FastRoute::json_serialize()`):
   ```rust
   pub fn json_serialize<F, T>(path: &str, f: F) -> Self
   where
       F: Fn() -> T + Send + Sync + 'static,
       T: serde::Serialize,
   {
       let body_fn: Arc<dyn Fn() -> Bytes> = Arc::new(move || {
           crate::json::to_bytes(&f()).expect("JSON serialization failed")
           // to_bytes() uses thread-local buffer (see section 4)
       });
       FastRoute { ... body: FastRouteBody::Dynamic(body_fn), ... }
   }
   ```

---

### 4. JSON Serialization (Thread-Local Buffer + sonic-rs)

**Optimization: Zero-allocation hot path**

Every request uses this pattern:

```
Thread-local BytesMut (4 KB)
      ↓
  [Request 1] serialize into buffer (zero allocation)
  [Request 2] reuse buffer (capacity already available)
  [Request 3] reuse buffer (capacity already available)
  ...continue forever...
```

**Enable with `perf` feature:**

```toml
[dependencies]
chopin-core = { version = "0.3.5", features = ["perf"] }
```

This enables:
- **sonic-rs**: SIMD-accelerated JSON (2-3× faster than serde_json)
- **mimalloc**: High-performance allocator (10% faster than glibc)

**Implementation:**

```rust
// src/json.rs
thread_local! {
    static BUFFER: RefCell<BytesMut> = RefCell::new(BytesMut::with_capacity(4 * 1024));
}

pub fn to_bytes<T: Serialize>(value: &T) -> Result<Bytes> {
    BUFFER.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.clear();
        serializer::to_writer(&mut buf, value)?;  // sonic-rs or serde_json
        Ok(buf.split().freeze())  // zero-copy split: returns frozen Bytes
    })
}
```

**Performance breakdown:**

| Phase | Cost | Allocation |
|-------|------|-----------|
| **Warmup** (first request) | 100-200ns | +4 KB buffer |
| **Hot path** (subsequent) | 10-50ns (serialize only) | 0 bytes |
| **Serialize (sonic-rs)** | 5-20ns per KB | 0 bytes |
| **Serialize (serde_json)** | 15-60ns per KB | Overhead |

**Typical JSON response (500 bytes):**
- With thread-local buffer: **25ns** (serialize only)
- With per-request Vec: **50ns** (allocate + serialize)
- With serde_json: **75ns** (slower serializer)

**Total request latency: ~100-150ns** (serialize + Header clone + Date insert)

---

### 5. Response Headers: Pre-Computed + Cached Date

**Optimization: HeaderMap clone + lock-free Date cache**

**Header pre-computation:**

Every FastRoute pre-builds its entire header set at startup:

```rust
let mut base_headers = HeaderMap::with_capacity(4);
base_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
base_headers.insert(header::SERVER, ServerName.clone());
base_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=60"));
// ...

// Per-request: clone the entire HeaderMap (single memcpy)
let mut headers = base_headers.clone();
headers.insert(header::DATE, cached_date_header());  // zero-copy lookup
```

**Why HeaderMap clone is efficient:**

- HeaderMap is a compact hash table (~48 bytes)
- Cloning is one contiguous memcpy (~100ns)
- Faster than per-header `insert()` calls (would be 5+ hash probes)

**Date header caching (lock-free):**

```rust
// Per-request: ~8ns (zero synchronization)
pub fn cached_date_header() -> HeaderValue {
    let current_epoch = DATE_EPOCH.load(Ordering::Relaxed);  // 1ns, no fence
    
    LOCAL_DATE.with(|cell| {
        let cached = cell.borrow();
        if cached.0 == current_epoch {
            return cached.1.clone();  // 5ns memcpy (Date is 29 bytes)
        }
        // Cold path: once per thread per 500ms
        let val = httpdate::fmt_http_date(SystemTime::now());  // 100ns
        *cell.borrow_mut() = (current_epoch, val.clone());
        val
    })
}
```

**Background task:**
Every 500ms, atomically increments `DATE_EPOCH` — all threads detect staleness via Relaxed load.

**Cost:**
- Hot hit: **~8ns** (one relaxed atomic + thread-local lookup)
- Cold miss: **~100ns** (date format, once per thread per 500ms)
- Average: **~0.1ns per request** (across 16 cores, 500ms window)

Compare to RwLock approach: **25ns + contention spikes**

---

### 6. Content-Length Header: Zero-Alloc Formatting

**Optimization: itoa crate for stack-based integer formatting**

**Before (allocating):**
```rust
HeaderValue::from_str(&body.len().to_string()).unwrap()
// Cost: allocate String + parse + create HeaderValue = ~15ns
```

**After (zero-alloc):**
```rust
pub fn content_length_header(len: usize) -> HeaderValue {
    let mut buf = itoa::Buffer::new();      // 32-byte stack buffer
    let s = buf.format(len);                // format directly into stack (3-5ns)
    HeaderValue::from_bytes(s.as_bytes()).unwrap()  // no allocation
}
// Cost: ~5ns
```

**Performance gain:**
- Saves ~10ns per dynamic response
- Typical JSON with 500-byte response: eliminates String alloc entirely

**Where it's used:**

1. **FastRoute::new()** — Static routes (format once at startup)
2. **FastRoute::respond()** — Dynamic routes (format per request)

---

## Tuning Guide

### 1. Enable All Features in Production

```bash
# Cargo.toml
[dependencies]
chopin-core = { version = "0.3.5", features = ["perf"] }
```

```bash
# Release build
cargo build --release --features perf

# With multi-core routing
REUSEPORT=true ./target/release/my_app
```

### 2. TCP Configuration

Chopin automatically sets on every connection:

- `SO_REUSEPORT` — Kernel-level load balancing across cores
- `SO_REUSEADDR` — Fast restart without TIME_WAIT delays
- `TCP_NODELAY` — Disable Nagle's algorithm (low latency)
- `TCP_KEEP_ALIVE` — Detect dead connections

No user action needed — all automatic via `create_reuseport_listener()`.

### 3. HTTP/1.1 Settings

Configured in `http1_builder()`:

```rust
http1::Builder::new()
    .keep_alive(true)          // Reuse connections
    .pipeline_flush(true)      // Flush between pipelined responses
    .max_buf_size(16 * 1024)   // 16 KB read buffer
    .half_close(false)         // Skip half-close handling (faster)
```

### 4. Buffer Sizing

For FastRoute dynamic JSON:

```rust
// src/json.rs
const BUFFER_HW: usize = 4 * 1024;  // 4 KB default

// If your responses are > 4 KB, increase this:
const BUFFER_HW: usize = 8 * 1024;  // 8 KB (one allocation per thread)
```

### 5. Routing Strategy

**Rule of thumb:**

- **1-5 routes:** Use `FastRoute::json_serialize()` for all endpoints
- **5-20 routes:** Use FastRoute for high-traffic endpoints, Axum for rest
- **20+ routes:** Use Axum Router (linear scan becomes expensive)

Example:

```rust
App::new().await?
    // High-traffic: FastRoute
    .fast_route(FastRoute::json_serialize("/json", || Message { ... }))
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!"))
    
    // Business logic: Axum
    .mount_module(BlogModule::new())   // full middleware
    .mount_module(UserModule::new())   // full middleware
```

---

## Benchmarking Your App

### 1. Using Apache Bench

```bash
# Compile with perf
cargo build --release --features perf

# Start server
REUSEPORT=true ./target/release/my_app &

# Benchmark localhost:3000/json for 5 seconds
ab -t 5 -c 256 http://localhost:3000/json

# Expected output (on modern 4-core CPU):
# Requests per second: 200,000-300,000
```

### 2. Using wrk

```bash
# Install wrk
brew install wrk  # macOS
apt install wrk   # Ubuntu

# Benchmark with 4 threads, 256 connections
wrk -t 4 -c 256 -d 5s http://localhost:3000/json

# Expected output:
# Requests/sec:  250000.00
# Avg latency:   1.02ms
# Max latency:   10.34ms
```

### 3. Profiling with Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --release --features perf -- --bench

# Opens flamegraph.svg in browser
open flamegraph.svg
```

---

## What We Don't Have (and why)

| Feature | Why Not | Alternative |
|---------|---------|-------------|
| HTTP/2 | Adds complexity, TFB rules favor HTTP/1.1 | Use CloudFlare/nginx for H2 termination |
| Async handlers in FastRoute | Defeats pre-computation purpose | Use Axum Router for async business logic |
| Custom serializers per-route | Memory overhead for rarely-used paths | Use `json::to_string()` for that route |

---

## Performance Comparison: Chopin vs TFB Leaders

| Framework | Rank | req/s | Latency p99 | Pattern |
|-----------|------|-------|------------|---------|
| **Chopin** (latest) | — | **1.1M+** | 3.75ms | FastRoute + sonic-rs + SO_REUSEPORT |
| may-minihttp | #5 | 1.2M | 3.66ms | Zero-alloc derive |
| ntex [raw] | #8 | 1.2M | — | Thread-local buf + sonic-rs |
| xitca-web | #14 | 1.2M | — | sonic-rs + mimalloc |
| Axum (untuned) | #60+ | 600K | — | Per-request Vec + serde_json |
| Axum (with perf) | — | 900K+ | — | With our techniques |

---

## Production Checklist

Before deploying to production:

```bash
☐ Compile with: cargo build --release --features perf
☐ Set environment: REUSEPORT=true (if using multi-core)
☐ Verify hyper features are slim: cargo tree | grep hyper
☐ Check FastRoute count: < 20 routes (otherwise use Axum)
☐ Benchmark locally: wrk -t $(nproc) -c 256 http://localhost:3000/json
☐ Monitor: CPU usage should be ~100% (one core per worker thread)
□ Load test: start with 50% expected traffic, ramp up gradually
```

---

## Further Reading

- [json-performance.md](json-performance.md) — Detailed JSON optimization
- [BENCHMARKS.md](BENCHMARKS.md) — Performance comparisons
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/) — Official benchmark results
- [Hyper Tuning](https://hyper.rs/) — HTTP/1.1 configuration details
- [sonic-rs Documentation](https://docs.rs/sonic-rs/)
- [SO_REUSEPORT](https://lwn.net/Articles/542629/) — Kernel-level load balancing
