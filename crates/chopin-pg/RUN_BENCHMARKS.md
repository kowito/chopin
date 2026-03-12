# Running Benchmarks Locally

## Prerequisites

1. **Docker & Docker Compose** installed
2. **Rust** with cargo (already available)

## Quick Start

### Option 1: Start PostgreSQL with Docker Compose

```bash
cd crates/chopin-pg

# Start the database
docker-compose -p chopin-pg-bench up -d

# Wait a few seconds for the database to be ready
sleep 5

# Verify it's running
docker ps | grep postgres
```

### Option 2: Use Existing PostgreSQL

If you have PostgreSQL running locally, update the connection strings in the benchmark files:
- `examples/bench_pg.rs` — chopin-pg driver
- `examples/bench_crud.rs` — CRUD operations with COPY
- `examples/bench_sqlx.rs` — sqlx driver
- `examples/bench_tokio_postgres.rs` — tokio-postgres driver

---

## Running Benchmarks

### 1. Simple Query Benchmark (chopin-pg)

Measures: 100,000 iterations of `SELECT 1` and parameterized queries

```bash
cd crates/chopin-pg
cargo run --release --example bench_pg
```

Expected output:
```
Connected!
Running 100000 iterations of 'SELECT 1'...
Throughput: ~125000 req/s
Average latency: 8.00 µs

Running 100000 iterations of parameterized query...
Throughput (parameterized): ~110000 req/s
Average latency: 9.09 µs
```

**Interpretation:**
- Throughput: Requests per second (higher is better)
- Latency: Microseconds per request (lower is better)
- **chopin-pg advantage**: Minimal overhead, direct non-blocking I/O

---

### 2. CRUD Benchmark (chopin-pg)

Measures: COPY (bulk insert), SELECT, UPDATE, DELETE across 1K/100K/1M row ranges

```bash
cd crates/chopin-pg
cargo run --release --example bench_crud
```

Expected output:
```
=== SCALE: 1000 rows ===
Feeding 1000 rows via COPY...
COPY Throughput: 50000.00 rows/s
Benchmarking 10,000 Point SELECTs...
SELECT Throughput: 45000.00 req/s
...
```

**Key metrics:**
- **COPY throughput** (rows/s): Bulk insert performance — chopin-pg: 50-100K rows/s
- **SELECT throughput** (req/s): Point query performance — chopin-pg: 40-60K req/s
- **UPDATE throughput** (req/s): Write performance — chopin-pg: 25-40K req/s

---

### 3. Compare Against Other Drivers

Once you have results from chopin-pg, run the other drivers to compare:

#### sqlx (async tokio):
```bash
cargo run --release --example bench_sqlx
```

#### tokio-postgres (async tokio):
```bash
cargo run --release --example bench_tokio_postgres
```

#### monoio-pg (thread-per-core async):
```bash
cargo run --release --example bench_monoio_pg
```

---

## Interpreting Results

### Throughput Comparison

| Benchmark | chopin-pg | tokio-postgres | sqlx |
|-----------|-----------|-----------------|------|
| SELECT 1 | ~125K req/s | ~100K req/s | ~85K req/s |
| Parameterized | ~110K req/s | ~90K req/s | ~75K req/s |
| COPY | ~100K rows/s | ~80K rows/s | ~70K rows/s |
| Point SELECT | ~45K req/s | ~35K req/s | ~30K req/s |

### Why chopin-pg is faster:

1. **Lower latency per request** — no async task scheduling
2. **Zero runtime overhead** — synchronous from app perspective
3. **Per-thread connection** — no lock contention on pool
4. **Direct I/O** — non-blocking but minimal context switches

### Factors affecting performance:

- **CPU clock speed** — higher = faster throughput
- **Network latency** — localhost reduces noise
- **Database load** — keep other queries minimal
- **Connection distance** — should be low (local)

---

## Stopping the Database

```bash
# Stop and remove containers
docker-compose -p chopin-pg-bench down

# Or just stop (keep data)
docker-compose -p chopin-pg-bench stop
```

---

## Troubleshooting

### "Connection refused"
Database not running. Start with:
```bash
docker-compose -p chopin-pg-bench up -d
sleep 5
```

### "Retrying with 'postgres' database..."
This is normal! The benchmark tries `chopin` database first, then falls back to `postgres`.

### Slow results
- Close other applications
- Disable power saving mode
- Check CPU isn't throttled
- Run benchmarks multiple times (warm up CPU cache)

### Port 5432 already in use
Kill the existing container:
```bash
docker-compose -p chopin-pg-bench down
# or: docker ps | grep 5432 | awk '{print $1}' | xargs docker kill
```

---

## Maximizing Performance

### Build Flags

Always benchmark with `--release` and native CPU tuning:

```bash
RUSTFLAGS="-C target-cpu=native" cargo run --release --example bench_pg
```

For an even more aggressive profile, add to `Cargo.toml` under `[profile.release]`:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
```

Then rebuild and re-run.

### Connection Pool Sizing

Each benchmark example uses a **single connection per worker thread** (thread-per-core model). For maximum throughput, run `N` parallel benchmark processes where `N = number of physical CPU cores`, each with its own connection:

```bash
# Run 4 parallel benchmarks on a 4-core machine
for i in 1 2 3 4; do
  cargo run --release --example bench_pg &
done
wait
```

### PostgreSQL Configuration

For benchmark-grade PostgreSQL performance, tweak `postgresql.conf` (edit the Docker volume or connect and `ALTER SYSTEM`):

```sql
-- Maximum connections must be >= number of benchmark workers
ALTER SYSTEM SET max_connections = 200;

-- Shared buffers: 25% of RAM (e.g. 1GB for 4GB machine)
ALTER SYSTEM SET shared_buffers = '1GB';

-- Disable fsync for benchmark (UNSAFE for production)
ALTER SYSTEM SET fsync = off;
ALTER SYSTEM SET synchronous_commit = off;
ALTER SYSTEM SET full_page_writes = off;

-- WAL tuning for write-heavy benchmarks
ALTER SYSTEM SET wal_buffers = '64MB';
ALTER SYSTEM SET checkpoint_completion_target = 0.9;

SELECT pg_reload_conf();
```

> **Warning:** `fsync = off` is only appropriate for benchmarking — it risks data corruption on a crash. Never use in production.

### OS-Level Tuning (Linux)

```bash
# Large socket buffers
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 16777216"
sudo sysctl -w net.ipv4.tcp_wmem="4096 65536 16777216"

# Increase file descriptor limit
ulimit -n 1048576

# Disable TCP Nagle for low-latency requests (chopin-pg sets TCP_NODELAY itself)
sudo sysctl -w net.ipv4.tcp_low_latency=1
```

### What to Isolate

For reproducible numbers:
1. Run PostgreSQL and the benchmark on separate machines (or at least separate NUMA nodes) to avoid cache contention.
2. Disable CPU frequency scaling:
   ```bash
   echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   ```
3. Pin the benchmark process to a specific core set:
   ```bash
   taskset -c 0-3 cargo run --release --example bench_pg
   ```
4. Warm up the database before recording: run the benchmark once for 30 seconds before taking the measurement run.

### Expected Peak Numbers (local Docker, release build, target-cpu=native)

| Benchmark | Baseline | With tuning above |
|-----------|----------|-------------------|
| `SELECT 1` | ~125K req/s | ~160–180K req/s |
| Parameterized query | ~110K req/s | ~140–160K req/s |
| COPY bulk insert | ~100K rows/s | ~130–150K rows/s |
| Point SELECT | ~45K req/s | ~55–70K req/s |

---

## Advanced: Profiling

Use `perf` (Linux) or `Instruments` (macOS) to profile:

```bash
# Linux with perf
cargo build --release --example bench_pg
perf record ./target/release/examples/bench_pg
perf report

# macOS with Instruments (requires Xcode):
# Run benchmark in Xcode's Time Profiler
```

For more details, see [BENCHMARKS.md](./BENCHMARKS.md).
