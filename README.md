# Chopin 🎼 (Codename: Nocturne Op. 9 No. 2)
<p align="center">
  <img src="docs/assets/logo.png" alt="Chopin Logo" width="200">
</p>

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-nightly-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

At peak optimization, Chopin delivers industry-leading throughput, effectively outperforming standard frameworks like Hyper by **~40%** while maintaining significantly lower latency.

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
- **Pre-Composed Middleware**: Middleware chains are resolved once at router `finalize()`. The hot path calls a single pre-built `Arc<dyn Fn>` with no per-request `Arc::new` or chain construction.
- **writev Zero-Copy Flush**: Response headers and body are written in one `writev` syscall. Static/byte bodies bypass the write buffer entirely — no memcpy.
- **sendfile File Serving**: `Response::file()` transfers file data directly in kernel space via `sendfile` (Linux) / `sendfile` (macOS), eliminating user-space copies.

### 3. Native Asynchronous Core
- **Platform Native**: Direct interaction with `kqueue` (macOS) and `epoll` (Linux) via low-level `libc` syscalls.
- **Manual Buffer Management**: Uses a custom `ConnectionSlab` (Slab Allocator) for O(1) connection state management.
- **Robust I/O**: Intelligent partial-write tracking (`write_pos`) to handle backpressure and socket saturation without data loss.

## 🛠️ Features

- **Radix Router**: Supports static paths, labeled parameters (`:id`), and wildcards (`*path`).
- **Declarative Extractors**: Ergonomic `FromRequest` trait for automatic `Json<T>` or `Query<X>` extraction.
- **Zero-Copy File Serving**: `Response::file(path)` uses platform `sendfile` (Linux/macOS) with automatic MIME detection (~30 types).
- **writev Body Flush**: Headers and response body are flushed in a single `writev` syscall, eliminating the memcpy into the write buffer.
- **Pre-Composed Middleware**: Middleware chains are composed once at startup; zero `Arc::new` allocations on the hot request path.
- **Database (PostgreSQL)**: `chopin-pg` (low-level driver) and `chopin-orm` (zero-allocation ORM) with per-worker connection pooling.
- **Authentication**: `chopin-auth` provides JWT, password hashing, and role-based access control.
- **Panic Resilience**: `catch_unwind` protection ensures a handler panic doesn't crash the worker thread.
- **Production-Ready**: Default HTTP/1.1 keep-alive, graceful shutdown, and O(1) connection pruning.

## 🛠️ Usage Example

Chopin uses attribute-based route discovery for a clean, declarative experience.

```rust
use chopin_core::{Chopin, Context, Response};
use chopin_macros::get;

#[get("/user")]
fn user_handler(ctx: Context) -> Response {
    let user = User { id: 1, username: "kowito".into() };
    ctx.json(&user)
}

fn main() {
    Chopin::new()
        .mount_all_routes()
        .serve("0.0.0.0:8080")
        .unwrap();
}
```

## 🎹 CLI at a Glance

The `chopin` CLI handles everything from project scaffolding to production deployment.

```bash
cargo install chopin-cli
chopin new my_app
chopin dev          # Hot-reload development
chopin check        # Architectural linter
chopin openapi      # Generate spec
```

## 📊 Performance Benchmark (macOS Apple Silicon)

| Framework | Endpoint | Relative Throughput | Latency (Avg) |
| :--- | :--- | :--- | :--- |
| **Chopin** | `/json` | **100%** | **686 μs** |
| **Chopin** | `/plain` | **100%** | **700 μs** |
| Actix Web | `/json` | 91% | 812 μs |
| Axum | `/json` | 84% | 945 μs |
| Hyper | `/json` | 73% | 1,810 μs |
| Hyper | `/plain` | 73% | 1,820 μs |

### 📊 Performance Visualization

```text
Throughput Comparison (Single-Core)
-------------------------------------------------------
Chopin     [██████████████████████████████] 100% (Baseline)
Actix Web  [███████████████████████████   ]  91%
Axum       [█████████████████████████     ]  84%
Hyper      [██████████████████████        ]  73%
-------------------------------------------------------
```

*Chopin is **10-15% faster** than Actix/Axum and **~40% faster** than Hyper with significantly lower latency.*

---
"Simple as a melody, fast as a nocturne." - *nocturne-op9-no2*
