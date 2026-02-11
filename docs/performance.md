# Performance Guide

Optimize your Chopin application for maximum throughput and minimal latency.

## Table of Contents

- [Quick Wins](#quick-wins)
- [Compilation Optimization](#compilation-optimization)
- [Database Optimization](#database-optimization)
- [Query Optimization](#query-optimization)
- [Caching Strategies](#caching-strategies)
- [Connection Pooling](#connection-pooling)
- [JSON Performance](#json-performance)
- [Profiling](#profiling)
- [Benchmarking](#benchmarking)
- [Apple Silicon Optimizations](#apple-silicon-optimizations)

## Quick Wins

### 1. Release Build

Always use release mode in production:

```bash
cargo build --release
```

**Performance difference**: 10-100x faster than debug builds

### 2. LTO (Link-Time Optimization)

Enable in `Cargo.toml`:

```toml
[profile.release]
lto = "fat"              # Full LTO
codegen-units = 1         # Single codegen unit for better optimization
```

**Impact**: 5-15% performance improvement

### 3. Database Indexes

Add indexes to frequently queried columns:

```rust
// In migration
.col(
    ColumnDef::new(Posts::AuthorId)
        .integer()
        .not_null()
)
.index(Index::create().name("idx_posts_author_id").col(Posts::AuthorId))
```

**Impact**: 10-1000x faster queries

### 4. Connection Reuse

Use Chopin's built-in connection pooling (automatic).

### 5. Pagination

Always paginate list endpoints:

```rust
use chopin_core::extractors::Pagination;

async fn list(
    pagination: Pagination,
) -> Result<ApiResponse<Vec<Item>>, ChopinError> {
    let p = pagination.clamped(); // Max 100
    
    let items = Entity::find()
        .limit(p.limit)
        .offset(p.offset)
        .all(&db)
        .await?;
    
    Ok(ApiResponse::success(items))
}
```

## Compilation Optimization

### Profile Settings

**Maximum Performance**:

```toml
[profile.release]
opt-level = 3              # Maximum optimization
lto = "fat"               # Full link-time optimization
codegen-units = 1          # Single codegen unit
strip = true              # Strip debug symbols
panic = "abort"           # Smaller binary, no unwinding
```

**Balanced** (faster compile, good performance):

```toml
[profile.release]
opt-level = 2
lto = "thin"
codegen-units = 16
```

### CPU-Specific Optimization

In `.cargo/config.toml`:

```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```

**Impact**: 5-10% improvement by using CPU-specific instructions

### Apple Silicon Optimization

Chopin automatically optimizes for Apple Silicon (see [Apple Silicon Optimizations](#apple-silicon-optimizations)).

## Database Optimization

### Connection Pool Sizing

Tune pool size based on load:

```env
# High-traffic API
DATABASE_URL=postgres://user:pass@host/db?max_connections=100&min_connections=20

# Low-traffic API
DATABASE_URL=postgres://user:pass@host/db?max_connections=20&min_connections=5
```

**Rule of thumb**: `max_connections = (CPU cores * 2) + effective_spindle_count`

### PostgreSQL Configuration

Optimize PostgreSQL settings:

```sql
-- Increase shared buffers (25% of RAM)
ALTER SYSTEM SET shared_buffers = '4GB';

-- Increase work memory
ALTER SYSTEM SET work_mem = '64MB';

-- Increase effective cache size
ALTER SYSTEM SET effective_cache_size = '12GB';

-- Restart PostgreSQL
```

### Analyze Tables

Keep statistics updated:

```sql
ANALYZE posts;
ANALYZE users;

-- Or all tables
ANALYZE;
```

## Query Optimization

### Use Indexes

Identify slow queries:

```sql
-- PostgreSQL
EXPLAIN ANALYZE SELECT * FROM posts WHERE author_id = 123;
```

Add indexes:

```rust
// In migration
.index(Index::create().name("idx_posts_author_id").col(Posts::AuthorId))
```

### Avoid N+1 Queries

**Bad** (N+1 problem):

```rust
// Fetches posts
let posts = Post::find().all(&db).await?;

// N queries for authors (one per post)
for post in posts {
    let author = User::find_by_id(post.author_id).one(&db).await?;
    // Use author...
}
```

**Good** (single additional query):

```rust
let posts = Post::find().all(&db).await?;

// Single query for all authors
let author_ids: Vec<i32> = posts.iter().map(|p| p.author_id).collect();
let authors = User::find()
    .filter(user::Column::Id.is_in(author_ids))
    .all(&db)
    .await?;

// Build lookup map
let author_map: HashMap<i32, User> = authors.into_iter()
    .map(|a| (a.id, a))
    .collect();
```

**Best** (eager loading with join):

```rust
let posts = Post::find()
    .find_also_related(User)
    .all(&db)
    .await?;

for (post, author) in posts {
    // Use post and author...
}
```

### Select Only Needed Columns

```rust
// ❌ Don't - fetches all columns
let users = User::find().all(&db).await?;

// ✅ Do - select specific columns
let emails = User::find()
    .select_only()
    .column(user::Column::Email)
    .into_tuple::<String>()
    .all(&db)
    .await?;
```

### Use Exists Over Count

**Slow**:
```rust
let count = Post::find()
    .filter(post::Column::AuthorId.eq(user_id))
    .count(&db)
    .await?;

if count > 0 {
    // Has posts
}
```

**Fast**:
```rust
let has_posts = Post::find()
    .filter(post::Column::AuthorId.eq(user_id))
    .limit(1)
    .one(&db)
    .await?
    .is_some();
```

### Batch Operations

**Slow** (N queries):
```rust
for item in items {
    item.delete(&db).await?;
}
```

**Fast** (single query):
```rust
Post::delete_many()
    .filter(post::Column::Id.is_in(item_ids))
    .exec(&db)
    .await?;
```

## Caching Strategies

### In-Memory Cache

Use `moka` for caching:

```toml
[dependencies]
moka = { version = "0.12", features = ["future"] }
```

```rust
use moka::future::Cache;
use std::sync::Arc;

pub struct AppState {
    pub db: DatabaseConnection,
    pub cache: Arc<Cache<i32, User>>,
}

// Setup
let cache = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(300)) // 5 minutes
    .build();

// Usage
async fn get_user(
    user_id: i32,
    State(app): State<AppState>,
) -> Result<User, ChopinError> {
    // Try cache first
    if let Some(user) = app.cache.get(&user_id).await {
        return Ok(user);
    }
    
    // Fetch from database
    let user = User::find_by_id(user_id)
        .one(&app.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;
    
    // Cache result
    app.cache.insert(user_id, user.clone()).await;
    
    Ok(user)
}
```

### Redis Cache

For distributed caching:

```toml
[dependencies]
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }
```

```rust
use redis::AsyncCommands;

async fn get_cached<T: DeserializeOwned>(
    redis: &redis::Client,
    key: &str,
) -> Result<Option<T>> {
    let mut conn = redis.get_async_connection().await?;
    let data: Option<String> = conn.get(key).await?;
    
    match data {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

async fn set_cached<T: Serialize>(
    redis: &redis::Client,
    key: &str,
    value: &T,
    ttl_seconds: usize,
) -> Result<()> {
    let mut conn = redis.get_async_connection().await?;
    let json = serde_json::to_string(value)?;
    conn.set_ex(key, json, ttl_seconds).await?;
    Ok(())
}
```

### HTTP Caching

Set cache headers:

```rust
use axum::response::{Response, IntoResponse};
use axum::http::header;

async fn cached_endpoint() -> Response {
    let mut response = ApiResponse::success(data).into_response();
    
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "public, max-age=3600".parse().unwrap(),
    );
    
    response.headers_mut().insert(
        header::ETAG,
        format!("\"{}\"", content_hash).parse().unwrap(),
    );
    
    response
}
```

## Connection Pooling

### Tune Pool Size

```env
DATABASE_URL=postgres://user:pass@host/db?max_connections=100&min_connections=10
```

**Guidelines**:
- Start with `(CPU cores * 2) + 4`
- Monitor connection usage
- Increase if connections exhausted
- Decrease if idle connections high

### Monitor Pool Usage

```rust
let pool_status = db.get_pool_status();
println!("Active: {}, Idle: {}, Max: {}", 
    pool_status.active, 
    pool_status.idle, 
    pool_status.max
);
```

## JSON Performance

### sonic-rs

Chopin uses `sonic-rs` for fast JSON (ARM NEON SIMD):

**Automatic** - Just use the `Json` extractor:

```rust
use chopin_core::extractors::Json;

async fn handler(
    Json(payload): Json<Request>,
) -> Result<ApiResponse<Response>, ChopinError> {
    // sonic-rs used automatically
}
```

**Performance**: 10-15% faster than serde_json on Apple Silicon

### Reduce Payload Size

Return only necessary fields:

```rust
// ❌ Don't expose entire model
pub struct UserResponse {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub password_hash: String,  // DON'T expose this!
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    // ... many fields
}

// ✅ Do return minimal response
pub struct UserResponse {
    pub id: i32,
    pub username: String,
}
```

### Streaming Responses

For large datasets, use streaming:

```rust
use axum::response::sse::{Event, KeepAlive};
use tokio_stream::StreamExt;

async fn stream() -> Sse<impl Stream<Item = Result<Event>>> {
    let stream = async_stream::stream! {
        let mut offset = 0;
        loop {
            let items = fetch_batch(offset, 100).await?;
            if items.is_empty() {
                break;
            }
            
            for item in items {
                yield Ok(Event::default().data(serde_json::to_string(&item)?));
            }
            
            offset += 100;
        }
    };
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

## Profiling

### CPU Profiling

Use `cargo-flamegraph`:

```bash
cargo install cargo-flamegraph

# Profile application
cargo flamegraph --release

# View flamegraph.svg
```

### Memory Profiling

Use `cargo-profb uild with Linux `perf`:

```bash
# Build with debug info
cargo build --release --features debug-info

# Run with perf
perf record -g ./target/release/my-api

# Analyze
perf report
```

### Tracing

Add timing instrumentation:

```rust
use tracing::{instrument, info};

#[instrument(skip(db))]
async fn fetch_data(db: &DatabaseConnection) -> Result<Vec<Data>> {
    let start = std::time::Instant::now();
    
    let data = Query::find().all(db).await?;
    
    info!(duration_ms = start.elapsed().as_millis(), "Query completed");
    
    Ok(data)
}
```

## Benchmarking

### Load Testing

Use `wrk` or `hey`:

```bash
# Install wrk
brew install wrk  # macOS
apt install wrk   # Linux

# Basic load test
wrk -t12 -c400 -d30s http://localhost:8080/api/posts

# Results show:
# - Requests/sec (throughput)
# - Latency (p50, p99)
# - Errors
```

### Application Benchmarks

Create benchmarks:

```rust
// benches/api_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn endpoint_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("list_posts", |b| {
        b.to_async(&rt).iter(|| async {
            let response = client.get("/api/posts").send().await;
            black_box(response)
        });
    });
}

criterion_group!(benches, endpoint_benchmark);
criterion_main!(benches);
```

Run benchmarks:

```bash
cargo bench
```

## Apple Silicon Optimizations

Chopin is heavily optimized for Apple Silicon (M1/M2/M3/M4).

### Automatic Optimizations

Chopin automatically enables:

1. **sonic-rs** - ARM NEON SIMD for JSON (10-15% faster)
2. **ring** - Hardware AES for JWT (5-10% faster)
3. **Native CPU targeting** - Via `.cargo/config.toml`

### Manual Tuning

In `.cargo/config.toml`:

```toml
[target.'cfg(target_arch = "aarch64")']
rustflags = [
    "-C", "target-cpu=native",
    "-C", "target-feature=+aes,+neon"
]
```

### Benchmark Results

On Apple M4:

| Operation | Throughput |
|-----------|------------|
| Simple GET | ~90,000 req/sec |
| JSON POST | ~85,000 req/sec |
| Auth endpoint | ~70,000 req/sec |
| Database query | ~50,000 req/sec |

**vs. x86_64**: 15-25% faster on ARM

## Performance Checklist

### ✅ Compilation

- [ ] Release build (`--release`)
- [ ] LTO enabled
- [ ] Target CPU native
- [ ] Stripped binaries

### ✅ Database

- [ ] Indexes on frequently queried columns
- [ ] Connection pool tuned
- [ ] No N+1 queries
- [ ] Batch operations where possible
- [ ] Analyze tables regularly

### ✅ Application

- [ ] Pagination on list endpoints
- [ ] Caching for hot data
- [ ] Minimal response payloads
- [ ] Connection reuse
- [ ] Async throughout

### ✅ Monitoring

- [ ] Profiling in development
- [ ] Load testing before deployment
- [ ] Metrics collection
- [ ] Slow query logging

---

## Performance Targets

### Realistic Expectations

**Apple M4 (single process)**:
- Simple endpoints: 80-90k req/sec
- Database queries: 40-60k req/sec
- Authentication: 60-80k req/sec

**x86_64 (modern CPU)**:
- Simple endpoints: 60-70k req/sec
- Database queries: 30-50k req/sec
- Authentication: 50-60k req/sec

### Scaling Beyond

For >100k req/sec:
- Horizontal scaling (multiple instances)
- Load balancer (Nginx, HAProxy)
- Database read replicas
- Redis caching
- CDN for static assets

---

## Resources

- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Database Performance Tips](https://wiki.postgresql.org/wiki/Performance_Optimization)
- [Tokio Performance](https://tokio.rs/tokio/topics/performance)

Optimize early, optimize often, measure everything!
