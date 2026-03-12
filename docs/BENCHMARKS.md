# Chopin Benchmark Guide

This document covers how to achieve maximum performance in benchmarks, what the current competitive numbers look like, and how each optimization layer contributes.

---

## TL;DR — Maximize Chopin Performance

```bash
# 1. Release build with full native optimizations
RUSTFLAGS="-C target-cpu=native" cargo build --release

# 2. Linux: enable io_uring backend (single biggest win after hardware tuning)
RUSTFLAGS="-C target-cpu=native" cargo build --release --features io-uring

# 3. Run with worker count pinned to physical cores (not hyperthreads)
CHOPIN_WORKERS=$(nproc --all) ./target/release/my_app

# 4. Tune the OS (see OS Tuning section below)
```

---

## Current Performance Metrics

### macOS Apple Silicon (M-series, 10 cores)

| Framework | Endpoint | Relative Throughput | Avg Latency |
| :--- | :--- | :--- | :--- |
| **Chopin** | `/json` | **100%** | **686 µs** |
| **Chopin** | `/plaintext` | **100%** | **700 µs** |
| Actix Web | `/json` | 91% | 812 µs |
| Axum | `/json` | 84% | 945 µs |
| Hyper | `/json` | 73% | 1,810 µs |

### Linux — TFB-style Benchmark (C16, plaintext)

| Framework | Avg Latency C16 | Throughput | vs Chopin |
| :--- | :--- | :--- | :--- |
| **Chopin (epoll)** | **2.48 ms** | **22.9M req/s** | baseline |
| Axum (Tokio) | 1.39 ms | 48.6M req/s | 2.1× faster |
| Hyper | 6.77 ms | — | 2.7× slower |

> **Note:** The C16 Linux gap vs Axum is a known scheduling overhead issue addressed by the io_uring backend (see below).

---

## Optimization Layers (in order of impact)

### 1. io_uring Backend (Linux only) — up to 50% latency reduction

Enable with the `io-uring` feature flag. At runtime, Chopin automatically switches every worker to a completion-based event loop instead of the epoll readiness loop:

```toml
# Cargo.toml
[dependencies]
chopin-core = { path = "crates/chopin-core", features = ["io-uring"] }
```

Or at build time:
```bash
cargo build --release --features chopin-core/io-uring
```

**What changes:**
- `accept` + `read` + `write` + `writev` + `close` are all submitted as SQEs (no per-op syscalls)
- Multi-shot accept: one submission generates a CQE per new connection indefinitely
- Batch CQE drain: up to 64 completions processed per `io_uring_enter` call
- Kernel ≥5.19 required for multi-shot accept; falls back to standard accept on older kernels

**Expected gains (vs epoll baseline):**

| Concurrency | epoll latency | io_uring latency | Improvement |
| :--- | :--- | :--- | :--- |
| C16 | 2.48 ms | ~1.3–1.7 ms | ~35–45% |
| C32 | 7.01 ms | ~3.5–4.5 ms | ~35–50% |
| C64 | 24.94 ms | ~12–16 ms | ~35–50% |

### 2. SQPOLL Mode — zero-syscall steady state (Linux 5.11+)

For maximum throughput on dedicated benchmark hosts, enable SQPOLL in `worker_uring.rs` by changing the setup flags:

```rust
// In worker_uring.rs: change setup_flags to include SQPOLL
let setup_flags = IORING_SETUP_SQPOLL
    | IORING_SETUP_SINGLE_ISSUER
    | IORING_SETUP_COOP_TASKRUN;
```

With SQPOLL, the kernel spawns a polling thread that continuously watches the SQ ring. Submissions never need `io_uring_enter` — zero syscalls in steady state.

> **Requires:** `CAP_SYS_NICE` or running as root for the kernel poll thread. Add `ulimit -l unlimited` for locked memory.

### 3. `target-cpu=native` — SIMD-accelerated date formatting

`format_http_date()` has an AVX2 fast path for formatting the HTTP Date header. This is only enabled when compiled with native tuning:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

**Impact:** ~2–5 µs savings per response (the date formatting syscall + AVX2 path together).

### 4. Worker Count Tuning

Chopin defaults to `num_cpus::get()` workers (logical cores including hyperthreads). For I/O-bound workloads, physical core count often performs better:

```rust
// In main.rs — pin to physical core count
Chopin::new()
    .mount_all_routes()
    .serve("0.0.0.0:3000")
    .unwrap();
```

Or set explicitly:
```rust
use chopin_core::Server;
Server::bind("0.0.0.0:3000")
    .workers(num_cpus::get_physical())  // physical cores only
    .serve(router)
    .unwrap();
```

### 5. Connection Slab Size

The default slab is 10,000 connections per worker. For TFB-style high-concurrency tests, increase it:

```rust
// worker.rs: change the slab size
let mut slab = ConnectionSlab::new(50_000);
```

Each `Conn` is ~25 KB (8 KB read buf + 16 KB write buf + metadata), so 50k slots ≈ 1.25 GB per worker. Size appropriately for your RAM.

---

## OS Tuning (Linux)

Apply these before running any benchmark:

```bash
# Increase file descriptor limits
ulimit -n 1048576

# TCP socket tuning
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65535
sudo sysctl -w net.core.netdev_max_backlog=65535
sudo sysctl -w net.ipv4.tcp_tw_reuse=1
sudo sysctl -w net.ipv4.tcp_fin_timeout=10

# io_uring: allow locked memory for SQPOLL
ulimit -l unlimited

# CPU governor: performance mode (avoids frequency scaling)
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# NUMA: bind to a single NUMA node (if applicable)
numactl --cpunodebind=0 --membind=0 ./target/release/my_app
```

---

## Benchmark Commands

### wrk (HTTP/1.1 keep-alive)
```bash
# Standard TFB-style: 8 threads, 256 connections, 15 seconds
wrk -t8 -c256 -d15s http://localhost:3000/plaintext

# Higher concurrency
wrk -t16 -c512 -d15s http://localhost:3000/json
```

### wrk2 (constant throughput)
```bash
# Measure latency at a fixed rate
wrk2 -t8 -c256 -d15s -R 100000 http://localhost:3000/plaintext
```

### hey
```bash
hey -n 1000000 -c 256 http://localhost:3000/plaintext
```

---

## Architecture: Why Chopin is Fast

```
┌──────────────────────────────────────────────────────┐
│  Per-Worker Thread (pinned to CPU core)               │
│                                                        │
│  SO_REUSEPORT listener  ─►  io_uring ring             │
│                               │                        │
│     multi-shot accept ◄───────┘                        │
│            │                                           │
│     ConnectionSlab (O(1) alloc, no locks)              │
│            │                                           │
│     Zero-alloc parser ──► Radix router                 │
│            │                                           │
│     Handler ──► Raw byte serializer                    │
│            │                                           │
│     writev (headers + body, 1 syscall)                 │
│            │                                           │
│     CQE completion ──► next SQE submission             │
└──────────────────────────────────────────────────────┘
```

**Key principles:**
- **No cross-thread communication** in the hot path (shared-nothing)
- **No heap allocations** per request (slab + stack arrays)
- **No epoll round-trip** with io_uring (submission ≠ syscall under SQPOLL)
- **No memcpy for body** (`Body::Static` / `Body::Bytes` use writev zero-copy)
- **No sendfile user-space copy** (`Body::File` uses kernel `sendfile`)

---

## Platform Support

| Feature | Linux | macOS |
| :--- | :--- | :--- |
| epoll (default) | ✅ | — |
| kqueue (default) | — | ✅ |
| io_uring (`io-uring` feature) | ✅ kernel ≥5.11 | — |
| TCP_DEFER_ACCEPT | ✅ | — |
| TCP_FASTOPEN | ✅ | ✅ |
| sendfile | ✅ | ✅ |
| SO_REUSEPORT | ✅ | ✅ |

---

## Previous Benchmark Results (2026-03-01, macOS, internal)

| Scenario | Framework | Relative Throughput | Avg Latency | Max Latency |
| :--- | :--- | :--- | :--- | :--- |
| **Plain Text** | **Chopin** | **100%** | **1.25 ms** | **37.83 ms** |
| | Hyper | 88% | 1.45 ms | 68.27 ms |
| **JSON** | **Chopin** | **100%** | **0.98 ms** | **25.05 ms** |
| | Hyper | 74% | 1.45 ms | 67.39 ms |

- **Architecture**: Apple Silicon (M-series), 10 cores
- **Concurrency**: 200 connections
- **Duration**: 10 seconds
- **Command**: `wrk -t10 -c200 -d10s`
