# Performance Mode Example

Deep dive into Chopin's dual-mode architecture with hands-on benchmarking.

## Quick Start

**Standard Mode** (development, default):
```bash
cargo run -p chopin-performance-mode
```

**Performance Mode** (raw hyper, SO_REUSEPORT):
```bash
SERVER_MODE=performance cargo run -p chopin-performance-mode --release
```

## What is Performance Mode?

Chopin's **Performance Mode** bypasses Axum's router for `/json` and `/plaintext` endpoints, routing them through a raw hyper `ChopinService` with:

- **SO_REUSEPORT** — Multiple TCP listeners (one per CPU core), kernel load balances
- **Pre-computed responses** — Static `Bytes` + `HeaderValue` constants, zero allocation
- **Cached Date headers** — Updated every 500ms by async task (avoids allocation)
- **mimalloc** — Microsoft's high-concurrency memory allocator
- **Native CPU** — Compiled with `target-cpu=native` and fat LTO

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

Typical results on Apple M-series (adjust for your hardware):

| Endpoint | Mode | Req/sec | Latency (avg) |
|----------|------|---------|---------------|
| `/json` | Performance | 500K–1.7M+ | <1ms |
| `/plaintext` | Performance | 500K–1.7M+ | <1ms |
| `/` | Standard | 150K–300K | 1-5ms |

**Note:** Performance mode's `/json` bypasses the entire Axum stack—it's just a pre-computed byte response. This is for benchmarking purposes; real APIs use Axum routes.

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
