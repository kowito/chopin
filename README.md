# Chopin 🎼 (Codename: Nocturne Op. 9 No. 2)

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

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

Chopin uses attribute-based route discovery for a clean, declarative experience.

```rust
use chopin_core::Chopin;
use chopin_macros::get;

#[get("/user")]
fn user_handler(ctx: Context) -> Response {
    let user = User { id: 1, username: "kowito".into() };
    ctx.respond_json(&user)
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

| Framework | Endpoint | Throughput | Latency (Avg) |
| :--- | :--- | :--- | :--- |
| **Chopin** | `/json` | **289,966 req/s** | **686 μs** |
| **Chopin** | `/plain` | **283,983 req/s** | **700 μs** |
| Hyper | `/json` | 212,731 req/s | 1,810 μs |
| Hyper | `/plain` | 211,844 req/s | 1,820 μs |

*Chopin is **40-43% faster** than Hyper with **5.4x lower latency**.*

---
"Simple as a melody, fast as a nocturne." - *nocturne-op9-no2*
