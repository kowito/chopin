# Chopin Benchmark Example

A purpose-built server for throughput benchmarking. Uses **Performance mode** — raw hyper HTTP/1.1 with `SO_REUSEPORT` multi-core accept loops and `mimalloc` global allocator.

## What Gets Benchmarked

| Endpoint       | Path          | Handled By  | Notes                                     |
|----------------|---------------|-------------|-------------------------------------------|
| JSON           | `GET /json`   | Raw hyper   | ChopinBody zero-alloc, direct headers     |
| Plaintext      | `GET /plaintext` | Raw hyper | ChopinBody zero-alloc, direct headers     |
| Welcome (Axum) | `GET /`       | Axum router | Standard middleware pipeline              |

The `/json` and `/plaintext` endpoints bypass Axum entirely — they are intercepted by `ChopinService` in `server.rs` and return `ChopinBody::Fast(Option<Bytes>)` with headers built directly from individual `HeaderValue`s (no `HeaderMap` clone) and a cached Date header.

## Quick Start

```bash
# From workspace root, release mode for accurate numbers
SERVER_MODE=performance \
DATABASE_URL=sqlite::memory: \
JWT_SECRET=bench \
cargo run -p chopin-benchmark --release
```

## Benchmarking with wrk

Install [wrk](https://github.com/wg/wrk):

```bash
brew install wrk   # macOS
```

Run benchmarks:

```bash
# JSON (raw hyper fast-path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/json

# Plaintext (raw hyper fast-path)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/plaintext

# Axum welcome route (through middleware)
wrk -t4 -c256 -d10s http://127.0.0.1:3000/
```

## Benchmarking with oha

[oha](https://github.com/hatoo/oha) provides better latency histogram output:

```bash
brew install oha

oha -c 256 -z 10s http://127.0.0.1:3000/json
```

## OS Tuning (Linux)

For serious benchmarks on Linux, increase file descriptor limits:

```bash
ulimit -n 65535
sysctl -w net.core.somaxconn=65535
sysctl -w net.ipv4.tcp_max_syn_backlog=65535
sysctl -w net.core.netdev_max_backlog=65535
```

## Performance Mode Details

When `SERVER_MODE=performance`:

1. **SO_REUSEPORT** — One TCP listener per CPU core, kernel balances connections
2. **Raw hyper** — `/json` and `/plaintext` skip Axum's Router entirely
3. **Pre-computed responses** — Static `Bytes` and `HeaderValue` constants
4. **Cached Date header** — Updated every 500ms by a background Tokio task
5. **mimalloc** — Microsoft's high-concurrency allocator (via `perf` feature)
6. **LTO + native CPU** — fat LTO, single codegen unit, `target-cpu=native`

## Expected Numbers

Performance varies by hardware. Rough baselines on Apple M-series:

| Endpoint    | Requests/sec |
|-------------|-------------|
| `/json`     | 500K–1.7M+  |
| `/plaintext`| 500K–1.7M+  |
| `/` (Axum)  | 150K–300K   |
