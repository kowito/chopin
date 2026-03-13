# Benchmarks

Performance benchmarks for the Chopin HTTP framework.

## Directory layout

```
benchmarks/
├── run.sh          — Single-variant wrk benchmark
├── compare.sh      — Side-by-side epoll vs io_uring comparison
├── profile.sh      — CPU profiling (macOS sample / Linux perf)
├── http_bench.py   — Dependency-free Python load generator
├── docs/
│   ├── chopin-pg-benchmarks.md    — PostgreSQL driver benchmark methodology
│   └── running-pg-benchmarks.md   — Step-by-step guide to run pg benchmarks
└── results/        — Timestamped output files (git-ignored)
```

## Criterion micro-benchmarks (Rust)

Hot-path micro-benchmarks live alongside each crate and use [criterion](https://bheisler.github.io/criterion.rs/):

```bash
# HTTP request processing pipeline (chopin-core)
cargo bench --bench request_pipeline -p chopin-core

# ORM query builder (chopin-orm — requires a running PostgreSQL)
cargo bench --bench orm_bench -p chopin-orm
```

Criterion HTML reports are written to `target/criterion/`.

## Prerequisites

| Tool  | Install |
|-------|---------|
| [wrk](https://github.com/wg/wrk) | `brew install wrk` / `apt install wrk` |
| Python 3.8+ | pre-installed on macOS/Linux |

## Quick start

**Step 1 — Build and start the server** (terminal 1):

```bash
# epoll (default, all platforms)
cargo run --release --example bench_chopin -p chopin-core

# io_uring (Linux only)
CHOPIN_IO_URING=1 cargo run --release --example bench_chopin -p chopin-core
```

**Step 2 — Run the benchmark** (terminal 2):

```bash
# With wrk (recommended, most accurate)
./benchmarks/run.sh epoll
./benchmarks/run.sh iouring

# Without wrk (Python, no deps)
python3 benchmarks/http_bench.py
python3 benchmarks/http_bench.py http://127.0.0.1:8080/plaintext --threads 16 --duration 30
```

## Environment variables

All scripts respect these overrides:

| Variable           | Default        | Description                   |
|--------------------|----------------|-------------------------------|
| `BENCH_HOST`       | `127.0.0.1`    | Server host                   |
| `BENCH_PORT`       | `8080`         | Server port                   |
| `BENCH_THREADS`    | CPU count      | wrk thread count              |
| `BENCH_CONNECTIONS`| `512`          | Concurrent connections        |
| `BENCH_DURATION`   | `30s`          | Benchmark duration            |
| `BENCH_WARMUP`     | `5s`           | Warmup duration               |

## Comparing epoll vs io_uring (Linux only)

```bash
./benchmarks/compare.sh
```

This script builds both variants, starts each in turn, benchmarks them, and prints a side-by-side summary.

## CPU profiling

```bash
# macOS (uses Instruments `sample`)
./benchmarks/profile.sh

# Linux (uses perf)
./benchmarks/profile.sh
```

Profile output is saved to `benchmarks/results/`.

## Interpreting results

| Metric         | Target          |
|----------------|-----------------|
| Req/s          | > 200k/core     |
| P99 latency    | < 1ms           |
| Errors         | 0               |

For TechEmpower Framework Benchmark (TFB) methodology, see [`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md).
