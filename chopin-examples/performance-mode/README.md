# Performance Example

Deep dive into Chopin's **unified ChopinService architecture** with hands-on benchmarking.

## Quick Start

**Default (single listener):**
```bash
cargo run -p chopin-performance-mode
```

**With SO_REUSEPORT (multi-core):**
```bash
REUSEPORT=true cargo run -p chopin-performance-mode --release --features perf
```

## Configuration Comparison

| Config | Architecture | Use Case |
|--------|--------------|----------|
| **Default** | Single listener, multi-thread tokio | Development, typical production |
| **REUSEPORT=true** | Per-core listeners, per-core runtimes | Maximum throughput benchmarks |

## What is FastRoute?

Chopin uses a **unified ChopinService** dispatcher for all requests.
Each FastRoute can be individually configured with **decorators** — all
pre-computed at registration time with zero per-request overhead.

```rust
use chopin::{App, FastRoute};

let app = App::new().await?
    // Bare: maximum performance, no extras
    .fast_route(FastRoute::json("/json", body))

    // With CORS + method filter (still ~35ns/req)
    .fast_route(
        FastRoute::json("/api/status", body)
            .cors()               // permissive CORS + auto OPTIONS preflight
            .get_only()           // POST falls through to Axum
    )

    // With Cache-Control + custom header
    .fast_route(
        FastRoute::text("/health", b"OK")
            .cache_control("public, max-age=60")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
    );
```

### Per-Route Trade-off Matrix

| Feature | FastRoute (bare) | FastRoute (+decorators) | Axum Router |
|---------|------------------|-------------------------|-------------|
| **Performance** | ~35ns | ~35ns | ~1,000-5,000ns |
| **Throughput** | ~28M req/s | ~28M req/s | ~200K-1M req/s |
| Static body | Yes | Yes | Yes |
| Dynamic body | — | — | Yes |
| CORS | — | `.cors()` | CorsLayer |
| Cache-Control | — | `.cache_control()` | manual |
| Method filter | — | `.methods()` / `.get_only()` | built-in |
| Custom headers | — | `.header()` | manual |
| Auth | — | — | middleware |
| Logging/Tracing | — | — | TraceLayer |
| Request ID | — | — | middleware |

**FastRoute is 28-142× faster than Axum Router** — decorators add zero per-request overhead.

### Request Dispatch

```
Client → ChopinService
  ├─ GET /json            → FastRoute (bare, ~35ns)
  ├─ GET /api/status      → FastRoute (+cors, ~35ns)
  ├─ OPTIONS /api/status  → FastRoute (204 preflight, automatic)
  ├─ POST /api/status     → falls through to Axum (method not allowed)
  └─ /* other             → Axum Router → Middleware → Handler
```

## Benchmark

### Setup

Install [wrk](https://github.com/wg/wrk):
```bash
brew install wrk      # macOS
apt-get install wrk   # Linux
```

### Terminal 1: Start Server

```bash
# With SO_REUSEPORT and release optimizations
REUSEPORT=true cargo run -p chopin-performance-mode --release
```

You should see:
```
🎹 Chopin Performance Mode Example
   → REUSEPORT: true
   → Server: http://127.0.0.1:3000
   → API docs: http://127.0.0.1:3000/api-docs
   → SO_REUSEPORT: enabled (multi-core)
   → Fast routes: 3
     • /json [GET, HEAD] (27 bytes)
     • /plaintext [GET, HEAD] (13 bytes)
     • /api/status [GET, HEAD] +cors (15 bytes)
```

### Terminal 2: Benchmark

```bash
# JSON endpoint (FastRoute zero-alloc path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/json

# Plaintext endpoint (FastRoute zero-alloc path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext

# Axum route (full middleware)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/
```

### Expected Results

Typical results on Apple M-series or modern x86_64 (adjust for your hardware):

#### Default
| Endpoint | Req/sec | Latency (avg) |
|----------|---------|---------------|
| `/json` (FastRoute) | 300K–500K | <1ms |
| `/` (Axum) | 150K–300K | 1-5ms |

#### REUSEPORT=true
| Endpoint | Req/sec | Latency (avg) |
|----------|---------|---------------|
| `/json` (FastRoute) | 500K–1.7M | <1ms |
| `/plaintext` (FastRoute) | 500K–1.7M | <1ms |
| `/` (Axum) | 150K–300K | 1-5ms |

## Code Patterns

### Per-route configuration

```rust
use chopin::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new().await?
        // Bare: benchmark endpoint (zero-alloc, no extras)
        .fast_route(
            FastRoute::json("/json", br#"{"message":"Hello, World!"}"#)
                .get_only()
        )
        // With CORS: frontend-accessible status endpoint
        .fast_route(
            FastRoute::json("/api/status", br#"{"status":"ok"}"#)
                .cors()
                .get_only()
                .cache_control("public, max-age=5")
        )
        // Full middleware: everything else goes through Axum
        ;

    app.run().await?;
    Ok(())
}
```

### SO_REUSEPORT (multi-core)

```rust
use chopin::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Via environment variable (recommended)
    std::env::set_var("REUSEPORT", "true");
    
    let app = App::new().await?;
    app.run().await?;  // Uses per-core SO_REUSEPORT listeners
    
    Ok(())
}
```

### Via .env File

```bash
# .env
REUSEPORT=true
DATABASE_URL=sqlite:./app.db
JWT_SECRET=your-secret-key
```

### Release Profile

Optimizations configured in `Cargo.toml`:

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
Request Routing:
┌─────────────────────────────────────────────────┐
│ Client Request                                  │
└───────────────────┬─────────────────────────────┘
                    │
         ┌──────────┴──────────┐
         │  ChopinService      │
         │  dispatch            │
         └──────────┬──────────┘
                    │
    ┌───────────────┼───────────────┐
    │               │               │
  FastRoute      FastRoute        Axum Router
  (bare)         (+cors,cache)    (full middleware)
  ~35ns/req      ~35ns/req        ~1-5μs/req
    │               │               │
    └───────────────┼───────────────┘
                    ▼
               Response
```

All requests flow through **ChopinService**:

1. **Request received** → ChopinService::call(req)
2. **CORS preflight?** (OPTIONS + `.cors()` enabled)
   - Yes → Pre-computed 204 No Content with CORS headers
3. **FastRoute path match + method allowed?**
   - Yes → Pre-computed response (zero heap alloc)
   - No → Falls through
4. **Axum Router** → Full middleware stack (CORS, auth, tracing, etc.)

**With `REUSEPORT=true`:**
- Creates N socket listeners with `SO_REUSEPORT`
- Spawns N accept loops (one per CPU core)
- Each core has its own `current_thread` tokio runtime
- Kernel load-balances TCP connections across cores
- Zero cross-thread synchronization

### SO_REUSEPORT Implementation

```rust
// In app.rs - REUSEPORT=true path
let socket_addr: std::net::SocketAddr = addr.parse()?;
crate::server::run_reuseport(socket_addr, fast_routes, router, shutdown_signal()).await?;
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

## Further Reading

- [Tutorial](https://kowito.github.io/chopin/tutorial.html) — Complete guide
- [chopin-core/src/server.rs](../../chopin-core/src/server.rs) — ChopinService implementation
- [chopin-core/src/perf.rs](../../chopin-core/src/perf.rs) — Lock-free date cache
