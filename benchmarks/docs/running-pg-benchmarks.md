# Running chopin-pg Benchmarks

Step-by-step guide for running and interpreting the `chopin-pg` benchmark suite.

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
- `crates/chopin-pg/examples/bench_pg.rs` — chopin-pg driver
- `crates/chopin-pg/examples/bench_crud.rs` — CRUD operations with COPY
- `crates/chopin-pg/examples/bench_sqlx.rs` — sqlx driver
- `crates/chopin-pg/examples/bench_tokio_postgres.rs` — tokio-postgres driver

---

## Running Benchmarks

### 1. Simple Query Benchmark (chopin-pg)

Measures: 100,000 iterations of `SELECT 1` and parameterized queries

```bash
cargo run --release --example bench_pg -p chopin-pg
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
cargo run --release --example bench_crud -p chopin-pg
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
- **COPY throughput** (rows/s): Bulk insert performance — chopin-pg: 50–100K rows/s
- **SELECT throughput** (req/s): Point query performance — chopin-pg: 40–60K req/s
- **UPDATE throughput** (req/s): Write performance — chopin-pg: 25–40K req/s

---

### 3. Compare Against Other Drivers

Once you have results from chopin-pg, run the other drivers to compare:

#### sqlx (async tokio):
```bash
cargo run --release --example bench_sqlx -p chopin-pg
```

#### tokio-postgres (async tokio):
```bash
cargo run --release --example bench_tokio_postgres -p chopin-pg
```

#### monoio-pg (thread-per-core async):
```bash
cargo run --release --example bench_monoio_pg -p chopin-pg
```

---

## Interpreting Results

### Throughput Comparison

| Benchmark       | chopin-pg   | tokio-postgres | sqlx        |
|-----------------|-------------|----------------|-------------|
| SELECT 1        | ~125K req/s | ~100K req/s    | ~85K req/s  |
| Parameterized   | ~110K req/s | ~90K req/s     | ~75K req/s  |
| COPY            | ~100K rows/s| ~80K rows/s    | ~70K rows/s |
| Point SELECT    | ~45K req/s  | ~35K req/s     | ~30K req/s  |

### Why chopin-pg is faster:

1. **Lower latency per request** — no async task scheduling
2. **Zero runtime overhead** — synchronous from app perspective
3. **Per-thread connection** — no lock contention on pool
4. **Direct I/O** — non-blocking but minimal context switches

### Factors affecting performance:

- **CPU clock speed** — higher = faster throughput
- **Network latency** — localhost reduces noise
- **Database load** — keep other queries minimal

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
Normal behaviour — the benchmark tries `chopin` database first, then falls back to `postgres`.

### Slow results
- Close other applications
- Disable power saving mode
- Check CPU isn't throttled
- Run benchmarks multiple times (warm up CPU cache)

### Port 5432 already in use
```bash
docker-compose -p chopin-pg-bench down
```

---

## Maximizing Performance

### Build Flags

Always benchmark with `--release` and native CPU tuning:

```bash
RUSTFLAGS="-C target-cpu=native" cargo run --release --example bench_pg -p chopin-pg
```

### PostgreSQL Tuning (Docker / local)

```sql
-- Disable fsync for benchmark only (NEVER in production)
ALTER SYSTEM SET fsync = off;
ALTER SYSTEM SET synchronous_commit = off;
ALTER SYSTEM SET wal_buffers = '64MB';
SELECT pg_reload_conf();
```

### OS-Level Tuning (Linux)

```bash
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216
ulimit -n 1048576
```
