# chopin-pg

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-pg)](https://crates.io/crates/chopin-pg)
[![Downloads](https://img.shields.io/crates/d/chopin-pg.svg)](https://crates.io/crates/chopin-pg)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

`chopin-pg` provides the high‑performance PostgreSQL driver used by the Chopin suite. It offers zero‑allocation query handling and per‑core connection pools.

## 🛠️ Usage Example

```rust
use chopin_pg::{PgConfig, PgConnection, PgValue};

fn main() -> PgResult<()> {
    let config = PgConfig::from_url("postgres://user:pass@localhost:5432/db")?;
    let mut conn = PgConnection::connect(&config)?;

    // Execute simple query
    let rows = conn.query_simple("SELECT current_database()")?;
    println!("Database: {}", rows[0].get(0)?);

    // Prepared statement (Extended Query Protocol)
    let rows = conn.query(
        "SELECT username FROM users WHERE id = $1",
        &[&42i32]
    )?;

    if let Some(row) = rows.first() {
        let name: String = row.get(0)?;
        println!("User: {}", name);
    }

    Ok(())
}
```

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
