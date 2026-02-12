# Performance

## Server Modes

Chopin offers two server modes to balance ease-of-use and raw throughput:

```bash
# Standard mode (default) — full middleware, easy development
cargo run

# Performance mode — raw hyper, multi-core, zero-alloc hot path
SERVER_MODE=performance cargo run --release

# Maximum performance — add mimalloc allocator
SERVER_MODE=performance cargo run --release --features perf
```

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

### Zero-Allocation Benchmark Endpoints

The `/json` and `/plaintext` endpoints bypass Axum entirely:

```rust
// Pre-computed at compile time — embedded in .rodata section
static JSON_BODY: Bytes = Bytes::from_static(b"{\"message\":\"Hello, World!\"}");
static PLAIN_BODY: Bytes = Bytes::from_static(b"Hello, World!");
```

Response construction uses pre-computed `HeaderValue` statics — no heap allocation, no string formatting:

```rust
static CT_JSON: HeaderValue = HeaderValue::from_static("application/json");
static CL_27: HeaderValue = HeaderValue::from_static("27");
```

### Date Header Caching

The `Date` HTTP header is cached and refreshed every 500ms by a background tokio task, instead of calling `SystemTime::now()` + formatting on every request.

### TCP Optimizations

| Setting | Value | Why |
|---------|-------|-----|
| `TCP_NODELAY` | `true` | Disable Nagle's algorithm for small responses |
| Backlog | `8192` | Handle burst connections (vs default 128) |
| `SO_REUSEADDR` | `true` | Quick port reuse after restart |
| `SO_REUSEPORT` | `true` | Kernel-level load balancing across cores |

### HTTP/1.1 Tuning

| Setting | Value | Why |
|---------|-------|-----|
| `keep_alive` | `true` | Reuse connections |
| `pipeline_flush` | `true` | Flush responses immediately for pipelined requests |
| `max_buf_size` | `8192` | Minimize memory per connection for small requests |

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
[target.aarch64-apple-darwin]
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]

# For x86_64 Linux servers
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+avx2,+aes"]
```

This enables `sonic-rs` to use SIMD instructions (NEON on ARM, AVX2 on x86) for JSON serialization.

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
| `/json` path | Through Axum + middleware | Direct hyper (zero alloc) |
| Allocator | System | mimalloc (with `perf`) |
| Date header | Per-request | Cached (500ms) |
| Middleware on `/json` | Full stack | None |
| Best for | Development, typical API | Benchmarks, extreme throughput |

## Checklist for Maximum Performance

1. [ ] `SERVER_MODE=performance`
2. [ ] `--release` build
3. [ ] `--features perf` (mimalloc)
4. [ ] `.cargo/config.toml` with `target-cpu=native`
5. [ ] `ENVIRONMENT=production` (disables tracing middleware)
6. [ ] Tune OS: `ulimit -n 65536`, sysctl `net.core.somaxconn=65536`
7. [ ] Use PostgreSQL with connection pooling for real API workloads
