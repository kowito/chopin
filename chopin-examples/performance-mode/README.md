# Performance Mode Example

Deep dive into Chopin's **three server modes** with hands-on benchmarking.

## Quick Start

**Standard Mode** (development, default):
```bash
cargo run -p chopin-performance-mode
```

**Performance Mode** (raw hyper, SO_REUSEPORT):
```bash
SERVER_MODE=performance cargo run -p chopin-performance-mode --release
```

**Raw Mode** (hyper bypassed, maximum throughput):
```bash
SERVER_MODE=raw cargo run -p chopin-performance-mode --release --features perf
```

## Server Mode Comparison

| Mode | Per-request | Use Case |
|------|-------------|----------|
| **Standard** | ~800ns | Development, typical APIs |
| **Performance** | ~450ns | Production high-load |
| **Raw** | ~240ns | Benchmarks, >1M req/s |

## What is Performance Mode?

Chopin's **Performance Mode** bypasses Axum's router for FastRoute endpoints (`/json`, `/plaintext`), routing them through a raw hyper `ChopinService` with:

- **SO_REUSEPORT** — Multiple TCP listeners (one per CPU core), kernel load balances
- **ChopinBody zero-alloc** — `ChopinBody::Fast(Option<Bytes>)` inline on stack, no `Box` heap allocation
- **Pre-built headers** — HeaderMap cloned once (3 headers), then Date inserted
- **Lock-free date cache** — thread_local + atomic epoch (~8ns per request)
- **mimalloc** — Microsoft's high-concurrency memory allocator
- **Native CPU** — Compiled with `target-cpu=native` and fat LTO

## What is Raw Mode?

**Raw Mode** completely bypasses hyper and writes pre-serialized HTTP responses directly to TCP sockets:

- **No HTTP parser** — Only path extracted (~10ns)
- **Pre-serialized responses** — HTTP bytes assembled at startup
- **Zero per-request allocs** — Reusable connection buffers
- **Single syscall writes** — Entire response in one `write_all()`
- **~45% faster than Performance mode** — 240ns vs 450ns per request

**Limitations:** Only FastRoute endpoints (no Axum router, no middleware).

## Benchmark

### Setup

Install [wrk](https://github.com/wg/wrk):
```bash
brew install wrk      # macOS
apt-get install wrk   # Linux
```

### Terminal 1: Start Server

```bash
# Performance mode with release optimizations
SERVER_MODE=performance cargo run -p chopin-performance-mode --release
```

You should see:
```
🎹 Chopin Performance Mode Example
   → Mode: performance
   → Server: http://127.0.0.1:3000
   → API docs: http://127.0.0.1:3000/api-docs
```

### Terminal 2: Benchmark

```bash
# JSON endpoint (raw hyper fast-path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/json

# Plaintext endpoint (raw hyper fast-path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext

# Axum route (standard middleware)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/
```

### Expected Results

Typical results on Apple M-series or modern x86_64 (adjust for your hardware):

#### Standard Mode
| Endpoint | Req/sec | Latency (avg) |
|----------|---------|---------------|
| `/` (Axum) | 150K–300K | 1-5ms |

#### Performance Mode
| Endpoint | Req/sec | Latency (avg) |
|----------|---------|---------------|
| `/json` | 500K–700K | <1ms |
| `/plaintext` | 500K–700K | <1ms |
| `/` (Axum) | 150K–300K | 1-5ms |

#### Raw Mode
| Endpoint | Req/sec | Latency (avg) |
|----------|---------|---------------|
| `/json` | **900K–1.2M+** | <500µs |
| `/plaintext` | **2M–2.5M+** | <500µs |

**Note:** Raw mode only serves FastRoute endpoints. No Axum fallback.

## Code Patterns

### Activating Performance Mode

```rust
use chopin_core::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Via environment variable (recommended)
    std::env::set_var("SERVER_MODE", "performance");
    
    let app = App::new().await?;
    app.run().await?;  // Automatically uses raw hyper + SO_REUSEPORT
    
    Ok(())
}
```

### Via .env File

```bash
# .env
SERVER_MODE=performance
DATABASE_URL=sqlite:./app.db
JWT_SECRET=your-secret-key
```

### Release Profile

Performance mode is enabled in `Cargo.toml`:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
```

Compile with:
```bash
cargo build -p chopin-performance-mode --release
```

## How It Works

### Architecture

```
Performance Mode Request Routing:
┌─────────────────────────────────────────────┐
│ Client Request                              │
└──────────────────┬──────────────────────────┘
                   │
        ┌──────────┴──────────┐
        │                     │
   /json, /plaintext     Other routes
        │                     │
        ▼                     ▼
  ChopinService         Axum Router
  (raw hyper)         (full middleware)
        │                     │
        ├─────────────────────┤
        ▼
   Response
```

### Pre-computed Responses

In `chopin-core/src/server.rs`:

```rust
// Static response - allocated once at startup
static JSON_RESPONSE: &[u8] = b"{\"message\":\"Hello, World!\"}";
static PLAINTEXT_RESPONSE: &[u8] = b"Hello, World!";

// Pre-computed headers - allocated once
lazy_static! {
    static ref CONTENT_TYPE_JSON: HeaderValue = 
        HeaderValue::from_static("application/json");
    static ref CONTENT_LENGTH: HeaderValue = 
        HeaderValue::from_static("27");  // len(JSON_RESPONSE)
}
```

### SO_REUSEPORT Multi-Core

```rust
// In app.rs - ServerMode::Performance path
let socket_addr: std::net::SocketAddr = addr.parse()?;
crate::server::run_reuseport(socket_addr, router, shutdown_signal()).await?;
```

In `server.rs`, one listener per CPU core:

```rust
let num_cores = num_cpus::get();
for _ in 0..num_cores {
    let socket = socket2::Socket::new(/*...*/)?;
    socket.set_reuse_port(true)?;  // SO_REUSEPORT
    let listener = TcpListener::from_std(socket.into())?;
    
    tokio::spawn(async move {
        accept_loop(listener, service.clone()).await
    });
}
```

## Comparisons

### Chopin Performance vs Axum

- **Chopin Performance:** 500K-1.7M+ req/s (fast-path endpoints)
- **Axum standard:** 150K-300K req/s
- **Raw hyper:** 1M-2M+ req/s (theoretical max, no routing)

### When to Use Each Mode

**Standard Mode** (default):
- Development
- Typical production (all endpoints through Axum)
- When you need predictable middleware behavior
- Most real-world APIs

**Performance Mode**:
- Benchmarking framework capabilities
- Extreme throughput requirements
- Testing SO_REUSEPORT multi-core behavior
- Learning high-performance Rust patterns

## Deep Dive: What Chopin Does

1. **Server Mode Detection** → `config.rs`
2. **Router Building** → `app.rs`
3. **Standard Path** → `axum::serve(listener, router)`
4. **Performance Path** → `server.rs::run_reuseport(addr, router, shutdown)`
   - Creates N socket listeners with `SO_REUSEPORT`
   - Spawns N accept loops (one per CPU core)
   - Kernel load-balances connections
   - Pre-computed responses for fast paths
   - Cached Date headers (500ms refresh)

## Further Reading

- [docs/performance.md](../../docs/performance.md) — Performance mode details
- [docs/architecture.md](../../docs/architecture.md) — Dual-mode design
- [chopin-core/src/server.rs](../../chopin-core/src/server.rs) — Raw hyper implementation
- [chopin-core/src/perf.rs](../../chopin-core/src/perf.rs) — Cached date header
