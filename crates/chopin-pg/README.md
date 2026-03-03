# chopin-pg

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-pg)](https://crates.io/crates/chopin-pg)
[![Downloads](https://img.shields.io/crates/d/chopin-pg.svg)](https://crates.io/crates/chopin-pg)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-pg` is a high‑performance, zero‑dependency PostgreSQL driver for the Chopin suite. Built for thread‑per‑core architectures with synchronous non‑blocking I/O, per‑worker connection pools, and zero external runtime dependencies (only `libc`).

## Features

- **Zero external dependencies** — all crypto (SCRAM-SHA-256), codec, and protocol are hand-written
- **Thread-per-core** — each worker owns its connections and pool; no `Arc`, no `Mutex`
- **Synchronous non-blocking I/O** — sockets in NB mode with poll-based application-level timeouts
- **Extended Query Protocol** — prepared statements with binary parameter encoding
- **Statement cache** — FNV-1a hash-based with LRU eviction and configurable capacity
- **Connection pool** — `PgPool` with checkout timeout, idle/max lifetime, test-on-checkout, auto-reconnect
- **COPY protocol** — bulk `COPY IN`/`COPY OUT` with streaming `CopyWriter`/`CopyReader`
- **LISTEN/NOTIFY** — async notification support with buffered delivery
- **Transactions** — `begin`/`commit`/`rollback`, savepoints, nested transactions, closure-based API
- **22 PostgreSQL types** — Bool, Int2/4/8, Float4/8, Text, Bytes, Json, Jsonb, Uuid, Date, Time, Timestamp, Timestamptz, Interval, Inet, Numeric, MacAddr, Point, Range, Array
- **Binary wire format** — per-parameter format codes with binary result decoding
- **SCRAM-SHA-256 auth** — zero-dep implementation; cleartext password also supported
- **Unix domain sockets** — `PgConfig.socket_dir` or `?host=` URL parameter
- **Error classification** — `ErrorClass::Transient`/`Permanent`/`Client`/`Pool` with SQLSTATE mapping
- **Retry helper** — `retry(max_retries, || { ... })` with transient error detection
- **Production hardening** — broken connection flag, TCP_NODELAY, zero-copy writes, `Rc<ColumnDesc>` sharing

## 🛠️ Quick Start

```rust
use chopin_pg::{PgConfig, PgConnection, PgResult};

fn main() -> PgResult<()> {
    let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;
    let mut conn = PgConnection::connect(&config)?;

    // Simple query (no parameters)
    let rows = conn.query_simple("SELECT current_database()")?;
    println!("Database: {:?}", rows[0].get(0)?);

    // Prepared statement with binary parameters
    let rows = conn.query(
        "SELECT id, name FROM users WHERE id = $1",
        &[&42i32],
    )?;
    for row in &rows {
        let id: i32 = row.get_typed(0)?;
        let name: String = row.get_typed(1)?;
        println!("User {}: {}", id, name);
    }

    // Execute (returns affected row count)
    let affected = conn.execute(
        "UPDATE users SET active = $1 WHERE id = $2",
        &[&true, &42i32],
    )?;
    println!("Updated {} rows", affected);

    Ok(())
}
```

## 🔗 Connection Pool

```rust
use chopin_pg::{PgConfig, PgPool, PgPoolConfig};
use std::time::Duration;

let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;

// Simple pool
let mut pool = PgPool::connect(config.clone(), 10)?;

// Advanced pool with configuration
let pool_config = PgPoolConfig::new()
    .max_size(25)
    .min_size(5)
    .checkout_timeout(Duration::from_secs(5))
    .idle_timeout(Duration::from_secs(300))
    .max_lifetime(Duration::from_secs(3600))
    .test_on_checkout(true);

let mut pool = PgPool::connect_with_config(config, pool_config)?;

// Get a connection (auto-returned on drop)
let mut conn = pool.get()?;
conn.query_simple("SELECT 1")?;

// Monitor pool health
println!("Active: {}, Idle: {}, Total: {}",
    pool.active_connections(), pool.idle_connections(), pool.total_connections());
let stats = pool.stats();
println!("Checkouts: {}, Created: {}", stats.total_checkouts, stats.total_connections_created);
```

## 📋 COPY Protocol (Bulk Operations)

```rust
// Bulk COPY IN
let mut writer = conn.copy_in("COPY users (name, email) FROM STDIN WITH (FORMAT csv)")?;
writer.write_row(&["Alice", "alice@example.com"])?;
writer.write_row(&["Bob", "bob@example.com"])?;
let rows_copied = writer.finish()?;
println!("Copied {} rows", rows_copied);

// COPY OUT
let mut reader = conn.copy_out("COPY users TO STDOUT WITH (FORMAT csv)")?;
let all_data = reader.read_all()?;
println!("Export: {}", String::from_utf8_lossy(&all_data));
```

## 🔔 LISTEN/NOTIFY

```rust
conn.listen("events")?;
conn.notify("events", "hello world")?;

// Poll for notifications
if let Some(notif) = conn.poll_notification()? {
    println!("Channel: {}, Payload: {}", notif.channel, notif.payload);
}

// Drain all buffered notifications
for notif in conn.drain_notifications() {
    println!("{}: {}", notif.channel, notif.payload);
}

conn.unlisten("events")?;
```

## 🔄 Transactions

```rust
// Closure-based (auto-commit on Ok, auto-rollback on Err)
conn.transaction(|tx| {
    tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Alice"])?;
    tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Bob"])?;
    Ok(())
})?;

// Manual control
conn.begin()?;
conn.execute("INSERT INTO users (name) VALUES ($1)", &[&"Charlie"])?;
conn.commit()?;

// Savepoints
conn.begin()?;
conn.savepoint("sp1")?;
conn.execute("INSERT INTO users (name) VALUES ($1)", &[&"Dave"])?;
conn.rollback_to("sp1")?;  // undo Dave
conn.commit()?;

// Nested transactions
conn.transaction(|tx| {
    tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Eve"])?;
    tx.transaction(|nested_tx| {
        nested_tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"Frank"])?;
        Ok(())
    })?;
    Ok(())
})?;
```

## 📊 Supported PostgreSQL Types

| PgValue Variant | PostgreSQL Type | Rust ToSql/FromSql |
|---|---|---|
| `Bool` | BOOLEAN | `bool` |
| `Int2` | SMALLINT | `i16` |
| `Int4` | INTEGER | `i32` |
| `Int8` | BIGINT | `i64` |
| `Float4` | REAL | `f32` |
| `Float8` | DOUBLE PRECISION | `f64` |
| `Text` | TEXT, VARCHAR | `String`, `&str` |
| `Bytes` | BYTEA | `Vec<u8>`, `&[u8]` |
| `Json` | JSON | `String` |
| `Jsonb` | JSONB | `Vec<u8>` |
| `Uuid` | UUID | `[u8; 16]` |
| `Date` | DATE | i32 (PG epoch days) |
| `Time` | TIME | i64 (microseconds) |
| `Timestamp` | TIMESTAMP | i64 (microseconds) |
| `Timestamptz` | TIMESTAMPTZ | i64 (microseconds) |
| `Interval` | INTERVAL | `{months, days, microseconds}` |
| `Inet` | INET, CIDR | `IpAddr`, `Ipv4Addr`, `Ipv6Addr` |
| `Numeric` | NUMERIC | `String` (lossless precision) |
| `MacAddr` | MACADDR | `[u8; 6]` |
| `Point` | POINT | `(f64, f64)` |
| `Range` | INT4RANGE, INT8RANGE, etc. | `String` |
| `Array` | ARRAY types | `Vec<T>` for scalar `T` |

## 🔐 Authentication

- **SCRAM-SHA-256** — fully implemented with zero external dependencies
- **Cleartext password** — supported
- **MD5** — recognized but returns an error (not implemented)

## 🔌 Connection Pool Sizing for High Concurrency

When handling high concurrency (e.g., 512+ concurrent connections), proper connection pool sizing is critical. Understanding the relationship between HTTP concurrency and database pool size is essential to avoid connection starvation and timeouts.

### Why Pool Size Matters

A common mistake is assuming a **1:1 ratio** between concurrent HTTP connections and database pool size. This fails because:

- **Not all incoming requests hit the database simultaneously.** At any moment, only 30-40% of HTTP connections are actively waiting on DB queries. The rest are parsing requests, serializing responses, or executing in middleware.
- **Database connections are expensive.** Each connection consumes memory and resources on both the client and server. Creating a connection for every possible concurrent request wastes resources.
- **Connection starvation causes cascading failures.** If all pool connections are busy and a new request arrives, it must wait. If many requests queue, timeouts increase exponentially.

### The Right Formula

```
Pool Size per Worker = (Total Concurrent Connections / Number of Workers) × Connection Ratio

Connection Ratio (typical): 0.3 to 0.5 (or 2:1 to 5:1 HTTP:DB ratio)
```

### Example: 512 Concurrent Connections

Assuming an 8-core system with 8 workers:

```
512 connections ÷ 8 workers = 64 connections per worker

❌ Pool size 64 per worker:  64:64 = 1:1 ratio (FAILS - connection starvation)
✅ Pool size 25 per worker:  64:25 = 2.5:1 ratio (RECOMMENDED)
✅ Pool size 20 per worker:  64:20 = 3.2:1 ratio (CONSERVATIVE)
✅ Pool size 32 per worker:  64:32 = 2:1 ratio (IF READ-HEAVY)
```

**Why 64 failed:** A 1:1 ratio means every HTTP connection needs its own DB connection. Since DB operations are fast, the pool becomes the bottleneck instead of the database. Requests queue up waiting for available connections, leading to timeouts.

### Configuration

Set pool size when initializing the connection pool:

```rust
use chopin_pg::{PgConfig, PgPool};

let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;

// For 512 concurrent with 8 workers, use 25 per worker
let pool = PgPool::new(config, 25);  // ← Recommended starting point
```

### Load Testing Recommendations

After configuring pool size, validate under realistic load:

```bash
# Load test with 512 concurrent clients, 8 threads, 30 seconds
wrk -t 8 -c 512 -d 30s http://localhost:8080/api/endpoint

# Monitor for:
# - Connection pool timeouts
# - Response latency increases
# - "All connections busy" errors in logs

# Database connection stats (in psql):
SELECT count(*) FROM pg_stat_activity;  -- Current active connections
SHOW max_connections;                    -- PostgreSQL server limit (default: 100)
```

### Tuning Guidelines

| Load Pattern | Suggested Pool Size | Ratio | Notes |
|--------------|---------------------|-------|-------|
| Read-heavy (80%+ reads) | 30-35 per worker | 2:1 | Queries are fast; can support higher concurrency |
| Balanced (50/50) | 20-25 per worker | 2.5-3.2:1 | **Starting point for most workloads** |
| Write-heavy (80%+ writes) | 15-20 per worker | 4-5:1 | Queries are slower; queue requests instead |
| Microservices + API calls | 25-40 per worker | 2-3:1 | External latency means more waiting connections |

### Monitoring & Alerts

Set up monitoring for pool exhaustion:

```rust
// Desired: Log when pool utilization > 80%
// If pool_size=25 and active_connections > 20, investigate

// Symptoms of undersized pool:
// - Increasing avg response time under sustained load
// - Queries queued in pg_stat_activity
// - Application logs: "Pool connection timeout"
// - Database slow query log fills up
```

### Summary

- **Never use 1:1 ratio** of HTTP connections to DB pool size
- **Start with 2.5:1 ratio** (20-25 pool size for 512 concurrent / 8 workers)
- **Load test under realistic conditions** before production deployment
- **Monitor pool utilization** and adjust based on actual behavior

For 512 concurrent connections, a well-tuned pool of 25 connections per worker will handle typical API workloads efficiently while preventing resource exhaustion.
