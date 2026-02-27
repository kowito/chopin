# Chopin 🎼 (Codename: Nocturne Op. 9 No. 2)

At peak optimization, Chopin delivers **280,000+ req/s** on a single core, effectively outperforming standard frameworks like Hyper by **~40%** while maintaining significantly lower latency.

## 🚀 Core Architecture

### 1. Shared-Nothing Model
Chopin adheres strictly to a shared-nothing model to ensure linear scaling across multi-core systems.
- **Independent Workers**: Each CPU core runs its own isolated event loop, memory allocator, and metrics counters.
- **SO_REUSEPORT Architecture**: Every worker thread creates its own listening socket. The kernel balances connections at the socket layer, eliminating any "Acceptor" thread bottleneck or cross-thread synchronization.
- **Partitioned Metrics**: Metrics are collected per-worker in 64-byte aligned, cache-local atomic counters, eliminating "cache-line bouncing."

### 2. Zero-Allocation Request Pipeline
- **Zero-Alloc Parser**: Slices raw socket buffers into string references (`&str`) without a single heap allocation.
- **Stack-Allocated Hot-Paths**: HTTP headers and route parameters are stored in fixed-size stack arrays.
- **Radix Tree Routing**: Efficient $O(K)$ path matching (where $K$ is path length) with zero-cost parameter extraction.
- **Raw Byte Serialization**: Responses are built using raw byte copies and inline `itoa` formatting, removing the overhead of `std::fmt`.

### 3. Native Asynchronous Core
- **Platform Native**: Direct interaction with `kqueue` (macOS) and `epoll` (Linux) via low-level `libc` syscalls.
- **Manual Buffer Management**: Uses a custom `ConnectionSlab` (Slab Allocator) for O(1) connection state management.
- **Robust I/O**: Intelligent partial-write tracking (`write_pos`) to handle backpressure and socket saturation without data loss.

## 🛠️ Features

- **Radix Router**: Supports static paths, labeled parameters (`:id`), and wildcards (`*path`).
- **Declarative Extractors**: Ergonomic `FromRequest` trait for automatic `Json<T>` or `Query<X>` extraction.
- **Panic Resilience**: `catch_unwind` protection ensures a handler panic doesn't crash the worker thread.
- **Production-Ready**: Default HTTP/1.1 keep-alive, graceful shutdown, and O(1) connection pruning.

## 🛠️ Usage Example

```rust
use chopin::{Server, Router, Context, Response, KJson};

#[derive(KJson, Default)]
struct User {
    id: u64,
    username: String,
}

fn user_handler(ctx: Context) -> Response {
    let user = User { id: 1, username: "kowito".into() };
    ctx.respond_json(&user) // Turbo-charged Schema-JIT serialization
}

fn main() {
    let mut router = Router::new();
    router.get("/user", user_handler);
    
    Server::bind("0.0.0.0:8080").serve(router).unwrap();
}
```

## 📊 Performance Benchmark (macOS Apple Silicon)

| Framework | Endpoint | Throughput | Latency (Avg) |
| :--- | :--- | :--- | :--- |
| **Chopin** | `/json` | **289,966 req/s** | **686 μs** |
| **Chopin** | `/plain` | **283,983 req/s** | **700 μs** |
| Hyper | `/json` | 212,731 req/s | 1,810 μs |
| Hyper | `/plain` | 211,844 req/s | 1,820 μs |

*Chopin is **40-43% faster** than Hyper with **5.4x lower latency**.*

---
"Simple as a melody, fast as a nocturne." - *nocturne-op9-no2*
