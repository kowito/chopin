# Chopin 🎼

Chopin is an ultra-high-performance, **Shared-Nothing**, asynchronous HTTP framework written in Rust. It is designed to maximize per-core efficiency by eliminating cross-core contention and minimizing heap allocations.

## 🚀 Core Concepts

### 1. Shared-Nothing Architecture
Chopin adheres strictly to a shared-nothing model. Each CPU core runs its own independent event loop, memory allocator, and metrics counters.
- **Independent Workers**: Each worker thread manages its own dedicated `ConnectionSlab` and `Epoll` (kqueue on macOS) instance.
- **Partitioned Metrics**: Metrics are collected per-worker in 64-byte aligned, cache-local atomic counters. This eliminates "cache-line bouncing" and write contention between CPU cores.
- **No Global Locks**: There are zero mutexes or global locks in the critical request-response path.

### 2. Zero-Allocation Pipeline
The entire HTTP processing pipeline is optimized for memory efficiency:
- **Zero-Alloc Parser**: The HTTP parser slices raw socket buffers into string references (`&str`) without copying data to the heap.
- **Cache-Line Aligned State**: The `Conn` structure is aligned to 64 bytes to fit perfectly into CPU L1 cache lines, preventing false sharing.
- **Efficient Routing**: Uses a Radix Tree (Prefix Tree) for $O(K)$ routing performance, where $K$ is the length of the path.

### 3. Native Asynchronous Core
Chopin bypasses high-level runtimes like Tokio to interact directly with OS-level syscalls:
- **Platform Native**: Uses `kqueue` on macOS and is architected for `epoll` efficiency on Linux.
- **Non-Blocking I/O**: Custom implementation of `accept`, `read`, and `write` loops using raw FD management.
- **Pipelining Support**: Built-in support for HTTP/1.1 keep-alive and pipelining with intelligent buffer offset tracking.

## 🛠️ Implementation Details

- **Radix Router**: Supports static paths, labeled parameters (`:id`), and wildcards (`*path`).
- **Declarative Extractors**: Ergonomic `FromRequest` trait for automatic `Json<T>` or `Query<X>` extraction.
- **Zero-Alloc Middleware**: Inject global logic (logging, auth) via raw function pointers, maintaining a zero-allocation pipeline.
- **Panic Resilience**: Caught-unwind protection per request ensures a single handler panic doesn't crash the entire worker core.
- **Graceful Shutdown**: Signal handling for safe connection draining before worker exit.

## 📊 Performance Summary (macOS)

At peak optimization, Chopin achieves significantly higher per-core efficiency compared to standard frameworks:

| Configuration | Throughput | Per-Core Efficiency |
| :--- | :--- | :--- |
| **Chopin (1 Worker)** | **~37,000 req/s** | **High** |
| **Chopin (10 Workers)** | **~32,000 req/s** | **Medium (OS Contention)** |

*Note: Multi-worker scaling on macOS is currently limited by kernel-level `SO_REUSEPORT` contention. The architecture is pre-optimized for linear scaling on Linux.*

---
"Simple as a melody, fast as a nocturne."
