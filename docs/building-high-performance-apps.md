# Building High-Performance Applications with Chopin

**Last Updated:** February 2026

> **Goal:** This guide teaches you how to build production-ready, high-performance applications with Chopin. You'll learn architectural patterns, optimization techniques, and deployment strategies for maximum throughput and minimal latency.

## Table of Contents

1. [Quick Start for Performance](#quick-start-for-performance)
2. [Project Setup](#project-setup)
3. [Database Optimization](#database-optimization)
4. [Caching Strategy](#caching-strategy)
5. [Efficient Route Handlers](#efficient-route-handlers)
6. [Response Optimization](#response-optimization)
7. [File Upload Performance](#file-upload-performance)
8. [Production Deployment](#production-deployment)
9. [Monitoring & Profiling](#monitoring--profiling)
10. [Real-World Example](#real-world-example)

---

## Quick Start for Performance

The fastest way to get a high-performance Chopin app running:

```bash
# Create a new project
chopin new my-fast-api
cd my-fast-api

# Create .cargo/config.toml for CPU-specific optimizations
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
# For Apple Silicon (M1/M2/M3/M4)
[target.'cfg(target_arch = "aarch64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]

# For x86_64 Linux/macOS servers (Intel/AMD)
[target.'cfg(target_arch = "x86_64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+avx2,+aes,+sse4.2"]
EOF

# Configure for performance mode
cat >> .env << 'EOF'
SERVER_MODE=performance
ENVIRONMENT=production
DATABASE_URL=sqlite://app.db?mode=rwc
RUST_LOG=warn
EOF

# Build and run with all performance features enabled
# Requires: SERVER_MODE=performance + --release + --features perf
cargo run --release --features perf
```

### Register Fast Routes (Optional)

For TechEmpower-style benchmarks or zero-allocation static endpoints:

```rust
// src/main.rs
use chopin_core::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let app = App::new().await?
        // Register benchmark endpoints — bypass Axum entirely
        .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!"));
    
    app.run().await?;
    Ok(())
}
```

**That's it!** You now have a server optimized for extreme throughput:
- **SERVER_MODE=performance** — Raw hyper HTTP/1.1, SO_REUSEPORT, per-core runtimes
- **--release** — LTO, codegen-units=1, opt-level=3
- **--features perf** — mimalloc allocator + sonic-rs SIMD JSON

Expected: **100K+ req/s** on modest hardware, **600K+ req/s** on high-end CPUs.

---

## Project Setup

### 1. CPU-Specific Compilation

Create `.cargo/config.toml` in your project root:

```toml
# For Apple Silicon (M1/M2/M3/M4)
[target.'cfg(target_arch = "aarch64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]

# For x86_64 Linux/macOS servers (Intel/AMD)
[target.'cfg(target_arch = "x86_64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+avx2,+aes,+sse4.2"]
```

**Why?** Enables native CPU instructions (NEON/AVX2) for better codegen and performance.

### 2. Environment Configuration

Create or update your `.env` file:

```env
# Critical for performance
SERVER_MODE=performance
ENVIRONMENT=production

# Server config
SERVER_HOST=0.0.0.0
SERVER_PORT=3000

# Database (PostgreSQL recommended for production)
DATABASE_URL=postgresql://user:pass@localhost/mydb

# Redis caching (optional but recommended)
REDIS_URL=redis://localhost:6379

# JWT settings
JWT_SECRET=your-256-bit-secret-key-change-this
JWT_EXPIRY_HOURS=24

# Logging (use 'warn' or 'error' in production)
RUST_LOG=warn
```

### 3. Choose the Right Database

| Database | Best For | Performance |
|----------|----------|-------------|
| **PostgreSQL** | Production APIs, complex queries | ⭐⭐⭐⭐⭐ |
| **SQLite** | Development, embedded apps | ⭐⭐⭐ |
| **MySQL** | Existing infrastructure | ⭐⭐⭐⭐ |

**For maximum performance, use PostgreSQL with connection pooling:**

```env
DATABASE_URL=postgresql://user:pass@localhost/mydb?sslmode=disable&pool_max_size=20
```

### 4. Cargo Features

Add these to your `Cargo.toml` dependencies:

```toml
[dependencies]
chopin-core = { version = "0.1", features = ["redis", "s3"] }

[features]
perf = ["chopin-core/perf"]  # Enables mimalloc allocator
```

Build with:
```bash
cargo build --release --features perf
```

---

## Database Optimization

### Connection Pooling

SeaORM automatically pools connections. Configure pool size based on your workload:

```rust
// config.rs - add to Config struct
pub struct Config {
    // ... existing fields
    pub db_max_connections: u32,  // e.g., 20
    pub db_min_connections: u32,  // e.g., 5
}
```

**Rule of thumb:** `max_connections = (CPU cores × 2) + 1` for I/O-bound workloads.

### Indexing

Always index foreign keys and frequently queried columns:

```rust
// migrations/m20250213_create_posts_table.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Posts::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(Posts::Title).string().not_null())
                    .col(ColumnDef::new(Posts::Body).text().not_null())
                    .col(ColumnDef::new(Posts::UserId).integer().not_null())
                    .col(ColumnDef::new(Posts::CreatedAt).timestamp().not_null())
                    .col(ColumnDef::new(Posts::UpdatedAt).timestamp().not_null())
                    // INDEX on foreign key
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-posts-user_id")
                            .from(Posts::Table, Posts::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx-posts-user_id")
                    .table(Posts::Table)
                    .col(Posts::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-posts-created_at")
                    .table(Posts::Table)
                    .col(Posts::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    // ... down() method
}

#[derive(Iden)]
enum Posts {
    Table,
    Id,
    Title,
    Body,
    UserId,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
```

### Query Optimization

**❌ Bad (N+1 queries):**
```rust
async fn get_posts_with_authors(db: &DatabaseConnection) -> Vec<PostWithAuthor> {
    let posts = posts::Entity::find().all(db).await.unwrap();
    let mut results = Vec::new();
    
    for post in posts {
        let author = users::Entity::find_by_id(post.user_id).one(db).await.unwrap();
        results.push(PostWithAuthor { post, author });
    }
    
    results
}
```

**✅ Good (1 query with JOIN):**
```rust
use sea_orm::{JoinType, QuerySelect};

async fn get_posts_with_authors(db: &DatabaseConnection) -> Vec<(posts::Model, Option<users::Model>)> {
    posts::Entity::find()
        .find_also_related(users::Entity)
        .all(db)
        .await
        .unwrap()
}

// Or use select_also + join
async fn get_posts_with_authors_explicit(db: &DatabaseConnection) -> Vec<(posts::Model, users::Model)> {
    posts::Entity::find()
        .join(JoinType::InnerJoin, posts::Relation::Users.def())
        .select_also(users::Entity)
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(post, user)| user.map(|u| (post, u)))
        .collect()
}
```

### Pagination

Always paginate large result sets:

```rust
use chopin_core::extractors::Pagination;
use sea_orm::{EntityTrait, PaginatorTrait, QueryOrder};

async fn list_posts(
    State(state): State<AppState>,
    pagination: Pagination,
) -> Result<ApiResponse<PaginatedResponse<Vec<PostResponse>>>, ChopinError> {
    // Count total (cached if possible)
    let total = posts::Entity::find().count(&state.db).await? as u64;
    
    // Fetch page
    let items = posts::Entity::find()
        .order_by_desc(posts::Column::CreatedAt)
        .offset(Some(pagination.offset()))
        .limit(Some(pagination.limit()))
        .all(&state.db)
        .await?
        .into_iter()
        .map(PostResponse::from)
        .collect();
    
    Ok(ApiResponse::success(pagination.response(items, total)))
}
```

**Pro tip:** Cache the total count for 60 seconds to avoid expensive `COUNT(*)` on every page:

```rust
let cache_key = "posts:total_count";
let total = match state.cache.get::<u64>(cache_key).await {
    Some(count) => count,
    None => {
        let count = posts::Entity::find().count(&state.db).await? as u64;
        state.cache.set(cache_key, count, Some(60)).await; // 60 second TTL
        count
    }
};
```

---

## Caching Strategy

### Cache Layers

Implement a multi-layer caching strategy:

```
Client → CDN → Redis → Database
   ↓       ↓       ↓        ↓
  60s     5m     10m     source
```

### Redis Setup

Enable Redis caching:

```bash
# Install Redis
brew install redis  # macOS
sudo apt install redis-server  # Ubuntu

# Start Redis
redis-server
```

Update `.env`:
```env
REDIS_URL=redis://localhost:6379
```

Build with Redis feature:
```bash
cargo build --release --features redis,perf
```

### Caching Pattern

```rust
use chopin_core::{AppState, ApiResponse, ChopinError};
use axum::extract::{State, Path};

async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let cache_key = format!("post:{}", id);
    
    // Try cache first
    if let Some(cached) = state.cache.get::<PostResponse>(&cache_key).await {
        return Ok(ApiResponse::success(cached));
    }
    
    // Cache miss - fetch from database
    let post = posts::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ChopinError::NotFound("Post not found".into()))?;
    
    let response = PostResponse::from(post);
    
    // Store in cache (5 minute TTL)
    state.cache.set(&cache_key, &response, Some(300)).await;
    
    Ok(ApiResponse::success(response))
}
```

### Cache Invalidation

Always invalidate cache when data changes:

```rust
async fn update_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdatePostRequest>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    // Update database
    let post = posts::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ChopinError::NotFound("Post not found".into()))?;
    
    let mut active: posts::ActiveModel = post.into();
    if let Some(title) = body.title {
        active.title = Set(title);
    }
    let updated = active.update(&state.db).await?;
    
    // Invalidate cache
    let cache_key = format!("post:{}", id);
    state.cache.delete(&cache_key).await;
    
    let response = PostResponse::from(updated);
    Ok(ApiResponse::success(response))
}
```

### What to Cache

| Data Type | Cache? | TTL | Why |
|-----------|--------|-----|-----|
| User profiles | ✅ Yes | 5-10 min | Rarely change, frequently accessed |
| Post list (paginated) | ✅ Yes | 1-2 min | Balance freshness vs performance |
| Individual posts | ✅ Yes | 5-10 min | Relatively stable |
| Comments | ⚠️ Maybe | 30 sec | May change frequently |
| Auth tokens | ❌ No | — | Security risk |
| Search results | ✅ Yes | 5-10 min | Expensive queries |
| Aggregations/stats | ✅ Yes | 10-60 min | Expensive to compute |

---

## Efficient Route Handlers

### Minimize Allocations

**❌ Bad:**
```rust
async fn bad_handler(State(state): State<AppState>) -> Result<ApiResponse<Vec<String>>, ChopinError> {
    let items = expensive_query(&state.db).await?;
    
    // Multiple allocations
    let mut processed = Vec::new();
    for item in items {
        let formatted = format!("Item: {}", item.name);
        processed.push(formatted);
    }
    
    Ok(ApiResponse::success(processed))
}
```

**✅ Good:**
```rust
async fn good_handler(State(state): State<AppState>) -> Result<ApiResponse<Vec<String>>, ChopinError> {
    let items = expensive_query(&state.db).await?;
    
    // Single allocation with capacity
    let processed: Vec<String> = items
        .into_iter()
        .map(|item| format!("Item: {}", item.name))
        .collect();
    
    Ok(ApiResponse::success(processed))
}

// Even better - preallocate
async fn best_handler(State(state): State<AppState>) -> Result<ApiResponse<Vec<String>>, ChopinError> {
    let items = expensive_query(&state.db).await?;
    
    let mut processed = Vec::with_capacity(items.len());
    for item in items {
        processed.push(format!("Item: {}", item.name));
    }
    
    Ok(ApiResponse::success(processed))
}
```

### Use References Where Possible

```rust
// Instead of cloning
async fn clone_heavy(State(state): State<AppState>) -> String {
    expensive_operation(state.config.database_url.clone())  // ❌ Unnecessary clone
}

// Use references
async fn reference_based(State(state): State<AppState>) -> String {
    expensive_operation(&state.config.database_url)  // ✅ No allocation
}
```

### Batch Operations

**❌ Bad (N database queries):**
```rust
async fn update_multiple(State(state): State<AppState>, ids: Vec<i32>) -> Result<(), ChopinError> {
    for id in ids {
        posts::Entity::update_many()
            .filter(posts::Column::Id.eq(id))
            .col_expr(posts::Column::ViewCount, Expr::col(posts::Column::ViewCount).add(1))
            .exec(&state.db)
            .await?;
    }
    Ok(())
}
```

**✅ Good (1 database query):**
```rust
async fn update_multiple(State(state): State<AppState>, ids: Vec<i32>) -> Result<(), ChopinError> {
    posts::Entity::update_many()
        .filter(posts::Column::Id.is_in(ids))
        .col_expr(posts::Column::ViewCount, Expr::col(posts::Column::ViewCount).add(1))
        .exec(&state.db)
        .await?;
    Ok(())
}
```

---

## Response Optimization

### Use ApiResponse Correctly

Chopin's `ApiResponse` uses optimized `crate::json::to_writer` for fast JSON serialization:
- **With `perf` feature**: sonic-rs (SIMD-accelerated, ~40% faster)
- **Without `perf`**: serde_json (stable fallback)

```rust
// ✅ This is optimized
Ok(ApiResponse::success(data))

// ❌ Don't convert to JSON manually
Ok(axum::Json(json!({"data": data})))  // Slower!
```

### Stream Large Responses

For large payloads, consider streaming:

```rust
use axum::response::{IntoResponse, Response};
use axum::body::Body;
use futures::stream::{self, StreamExt};

async fn stream_large_file() -> Result<Response, ChopinError> {
    let file = tokio::fs::File::open("large_file.json").await?;
    let reader = tokio::io::BufReader::new(file);
    let stream = tokio_util::io::ReaderStream::new(reader);
    let body = Body::from_stream(stream);
    
    Ok(Response::builder()
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap())
}
```

### Compress Responses

Add compression middleware for large responses:

```rust
use tower_http::compression::CompressionLayer;

fn main() {
    let app = Router::new()
        .merge(controllers::routes())
        .layer(CompressionLayer::new())  // gzip, br, deflate
        .with_state(state);
}
```

**Note:** In performance mode, compression is disabled by default for benchmark endpoints.

---

## File Upload Performance

### S3 for Production

Always use S3 (or S3-compatible storage) in production:

```env
# AWS S3
S3_BUCKET=my-app-uploads
S3_REGION=us-east-1
S3_ACCESS_KEY_ID=AKIA...
S3_SECRET_ACCESS_KEY=secret...

# Or MinIO (self-hosted S3-compatible)
S3_BUCKET=uploads
S3_ENDPOINT=https://minio.example.com
S3_ACCESS_KEY_ID=minioadmin
S3_SECRET_ACCESS_KEY=minioadmin
S3_PUBLIC_URL=https://cdn.example.com
```

Build with S3 feature:
```bash
cargo build --release --features s3,perf
```

### Upload Handler

```rust
use chopin_core::storage::FileUploadService;
use axum::extract::Multipart;

async fn upload_file(
    State(state): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<ApiResponse<FileUploadResponse>, ChopinError> {
    let upload_service = FileUploadService::new(&state.config);
    
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("file") {
            let filename = field.file_name()
                .ok_or(ChopinError::BadRequest("No filename".into()))?
                .to_string();
            
            let content_type = field.content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            
            let data = field.bytes().await?;
            
            // Upload to S3 or local storage
            let result = upload_service.upload(&filename, &data, &content_type).await?;
            
            return Ok(ApiResponse::success(FileUploadResponse {
                url: result.url,
                path: result.path,
            }));
        }
    }
    
    Err(ChopinError::BadRequest("No file in request".into()))
}
```

### Optimize Max Upload Size

```env
# 50MB for production use
MAX_UPLOAD_SIZE=52428800
```

---

## Production Deployment

### Build for Production

```bash
# Full optimizations
RUSTFLAGS="-C target-cpu=native" cargo build --release --features redis,s3,perf

# Binary will be at target/release/my-app
```

### System Tuning (Linux)

```bash
# Increase file descriptor limit
ulimit -n 65536

# Kernel network tuning
sudo sysctl -w net.core.somaxconn=65536
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65536
sudo sysctl -w net.ipv4.ip_local_port_range="1024 65535"
sudo sysctl -w net.ipv4.tcp_tw_reuse=1

# Make permanent
sudo tee -a /etc/sysctl.conf << EOF
net.core.somaxconn=65536
net.ipv4.tcp_max_syn_backlog=65536
net.ipv4.ip_local_port_range=1024 65535
net.ipv4.tcp_tw_reuse=1
EOF
```

### Systemd Service

Create `/etc/systemd/system/my-app.service`:

```ini
[Unit]
Description=My Chopin App
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/my-app
EnvironmentFile=/opt/my-app/.env
ExecStart=/opt/my-app/my-app
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable my-app
sudo systemctl start my-app
sudo systemctl status my-app
```

### Nginx Reverse Proxy

```nginx
upstream chopin_backend {
    # If using performance mode with SO_REUSEPORT, 
    # just proxy to the single port
    server 127.0.0.1:3000;
}

server {
    listen 80;
    server_name api.example.com;
    
    # Buffering
    client_body_buffer_size 128k;
    client_max_body_size 50m;
    
    location / {
        proxy_pass http://chopin_backend;
        proxy_http_version 1.1;
        
        # Headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
        
        # Keep-alive
        proxy_set_header Connection "";
    }
    
    # Health check
    location /health {
        access_log off;
        proxy_pass http://chopin_backend/;
    }
}
```

### Docker Deployment

Create `Dockerfile`:

```dockerfile
FROM rust:1.75-slim as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .cargo ./.cargo

# Build with all optimizations
RUN cargo build --release --features redis,s3,perf

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/my-app /app/my-app

# Non-root user
RUN useradd -m -u 1000 appuser && chown -R appuser:appuser /app
USER appuser

EXPOSE 3000

ENV RUST_LOG=warn
ENV SERVER_MODE=performance
ENV ENVIRONMENT=production

CMD ["/app/my-app"]
```

Build and run:
```bash
docker build -t my-app .
docker run -p 3000:3000 --env-file .env my-app
```

---

## Monitoring & Profiling

### Health Check Endpoint

Add a health check:

```rust
// src/controllers/health.rs
use axum::{Router, routing::get};
use chopin_core::{AppState, ApiResponse};

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

async fn health_check(State(state): State<AppState>) -> ApiResponse<HealthResponse> {
    // Check database
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();
    
    // Check Redis
    let cache_ok = state.cache.get::<String>("health_check").await.is_some() 
        || state.cache.set("health_check", "ok", Some(60)).await.is_ok();
    
    ApiResponse::success(HealthResponse {
        status: "ok",
        database: if db_ok { "connected" } else { "error" },
        cache: if cache_ok { "connected" } else { "error" },
        timestamp: chrono::Utc::now().timestamp(),
    })
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
    database: &'static str,
    cache: &'static str,
    timestamp: i64,
}
```

### Profiling with cargo-flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Run with profiling (requires perf on Linux)
cargo flamegraph --release --features perf --bin my-app

# Or use samply (cross-platform)
cargo install samply
samply record cargo run --release --features perf
```

### Load Testing

Use `wrk` or `bombardier`:

```bash
# Install wrk (Linux/macOS)
brew install wrk  # macOS
# or compile from source on Linux

# Test endpoint
wrk -t4 -c256 -d30s --latency http://localhost:3000/api/posts

# Results will show:
# - Requests/sec
# - Latency distribution (50th, 75th, 90th, 99th percentiles)
# - Transfer/sec
```

### Metrics with Prometheus

Add `metrics` and `metrics-exporter-prometheus`:

```toml
[dependencies]
metrics = "0.22"
metrics-exporter-prometheus = "0.13"
```

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup Prometheus exporter on :9090/metrics
    let builder = PrometheusBuilder::new();
    builder.install()?;
    
    // Your app
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

---

## Real-World Example

Here's a complete high-performance API for a social media app:

### Project Structure

```bash
my-social-api/
├── Cargo.toml
├── .env
├── .cargo/
│   └── config.toml
├── src/
│   ├── main.rs
│   ├── controllers/
│   │   ├── mod.rs
│   │   ├── posts.rs
│   │   ├── comments.rs
│   │   └── feed.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── post.rs
│   │   └── comment.rs
│   └── migrations/
│       └── ...
```

### main.rs

```rust
use chopin_core::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let app = App::new().await?
        // Optional: register benchmark/health endpoints as FastRoutes
        .fast_json("/health", br#"{"status":"ok"}"#);
    
    app.run().await?;
    
    Ok(())
}
```

### FastRoute for Zero-Allocation Endpoints

For ultimate performance on static responses:

```rust
use chopin_core::{App, FastRoute};

let app = App::new().await?
    // JSON endpoint (TechEmpower benchmark spec)
    .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
    
    // Plaintext endpoint
    .fast_route(FastRoute::text("/plaintext", b"Hello, World!"))
    
    // HTML endpoint
    .fast_route(FastRoute::html("/health", b"<html><body>OK</body></html>"))
    
    // Custom content-type
    .fast_route(FastRoute::new("/metrics", b"# HELP...", "text/plain; version=0.0.4"));
```

**How FastRoute works:**
- **Body:** Embedded in binary's `.rodata` section, stored as `ChopinBody::Fast(Option<Bytes>)` inline on the stack — **zero heap allocation** (no `Box` like `Body::from(Bytes)` does)
- **Headers:** Built directly on the response from individual `HeaderValue`s — **no `HeaderMap` clone**. `Content-Type` from `from_static` is a pointer-copy.
- **Date header:** Cached and updated every 500ms by background task
- **Routing:** Bypasses Axum Router entirely in performance mode (linear scan over 1-5 routes)
- **Fallback:** All other paths go through Axum Router with full middleware

### High-Performance Feed Controller

```rust
// src/controllers/feed.rs
use axum::{Router, routing::get, extract::{State, Path}};
use chopin_core::{
    AppState, ApiResponse, ChopinError,
    extractors::{AuthUser, Pagination, PaginatedResponse},
};
use sea_orm::{EntityTrait, QueryOrder, QuerySelect};
use crate::models::{posts, users};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/feed", get(get_feed))
        .route("/api/feed/:user_id", get(get_user_feed))
}

#[derive(serde::Serialize)]
struct FeedPost {
    id: i32,
    title: String,
    body: String,
    author_username: String,
    created_at: String,
    like_count: i32,
}

/// Get personalized feed (cached for 60 seconds)
async fn get_feed(
    State(state): State<AppState>,
    user: AuthUser,
    pagination: Pagination,
) -> Result<ApiResponse<PaginatedResponse<Vec<FeedPost>>>, ChopinError> {
    let cache_key = format!("feed:{}:{}:{}", user.user_id, pagination.page, pagination.per_page);
    
    // Try cache
    if let Some(cached) = state.cache.get::<PaginatedResponse<Vec<FeedPost>>>(&cache_key).await {
        return Ok(ApiResponse::success(cached));
    }
    
    // Fetch with JOIN (efficient!)
    let posts_with_authors = posts::Entity::find()
        .find_also_related(users::Entity)
        .order_by_desc(posts::Column::CreatedAt)
        .offset(Some(pagination.offset()))
        .limit(Some(pagination.limit()))
        .all(&state.db)
        .await?;
    
    let feed_posts: Vec<FeedPost> = posts_with_authors
        .into_iter()
        .filter_map(|(post, author)| {
            author.map(|a| FeedPost {
                id: post.id,
                title: post.title,
                body: post.body,
                author_username: a.username,
                created_at: post.created_at.to_string(),
                like_count: 0, // TODO: aggregate from likes table
            })
        })
        .collect();
    
    let total = posts::Entity::find().count(&state.db).await? as u64;
    let response = pagination.response(feed_posts, total);
    
    // Cache for 60 seconds
    state.cache.set(&cache_key, &response, Some(60)).await;
    
    Ok(ApiResponse::success(response))
}

/// Get specific user's feed (cached for 120 seconds)
async fn get_user_feed(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
    pagination: Pagination,
) -> Result<ApiResponse<PaginatedResponse<Vec<FeedPost>>>, ChopinError> {
    let cache_key = format!("user_feed:{}:{}:{}", user_id, pagination.page, pagination.per_page);
    
    if let Some(cached) = state.cache.get::<PaginatedResponse<Vec<FeedPost>>>(&cache_key).await {
        return Ok(ApiResponse::success(cached));
    }
    
    let posts_with_authors = posts::Entity::find()
        .filter(posts::Column::UserId.eq(user_id))
        .find_also_related(users::Entity)
        .order_by_desc(posts::Column::CreatedAt)
        .offset(Some(pagination.offset()))
        .limit(Some(pagination.limit()))
        .all(&state.db)
        .await?;
    
    let feed_posts: Vec<FeedPost> = posts_with_authors
        .into_iter()
        .filter_map(|(post, author)| {
            author.map(|a| FeedPost {
                id: post.id,
                title: post.title,
                body: post.body,
                author_username: a.username,
                created_at: post.created_at.to_string(),
                like_count: 0,
            })
        })
        .collect();
    
    let total = posts::Entity::find()
        .filter(posts::Column::UserId.eq(user_id))
        .count(&state.db)
        .await? as u64;
    
    let response = pagination.response(feed_posts, total);
    state.cache.set(&cache_key, &response, Some(120)).await;
    
    Ok(ApiResponse::success(response))
}
```

### Benchmark Endpoints (Optional)

For TechEmpower Framework Benchmarks or performance testing, register fast routes:

```rust
// src/main.rs
use chopin_core::{App, FastRoute};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let app = App::new().await?
        // FastRoute — zero allocation, bypasses Axum
        .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
        .fast_route(FastRoute::text("/plaintext", b"Hello, World!"))
        // Or use convenience methods
        .fast_json("/api/health", br#"{"status":"ok"}"#)
        .fast_text("/ping", b"pong");
    
    app.run().await?;
    Ok(())
}
```

### Performance Results

With this setup on a modest 4-core server:

```
Endpoint: GET /api/feed (via Axum)
├── Cold start (no cache): ~5-10ms
├── Cached: ~0.5-1ms
└── Throughput: 50,000+ req/s

Endpoint: GET /api/posts (via Axum)
├── Cold start: ~3-5ms
├── Cached: ~0.5ms
└── Throughput: 80,000+ req/s

Endpoint: GET /json (FastRoute)
├── Latency: ~0.1ms
└── Throughput: 200,000+ req/s

Endpoint: GET /plaintext (FastRoute)
├── Latency: ~0.05ms
└── Throughput: 300,000+ req/s
```

---

## Summary Checklist

For maximum performance, ensure:

- [x] `SERVER_MODE=performance` in `.env`
- [x] Build with `--release --features perf`
- [x] CPU-specific flags in `.cargo/config.toml`
- [x] PostgreSQL with connection pool of 20+
- [x] Redis caching enabled
- [x] Database indexes on all foreign keys and query columns
- [x] Pagination on all list endpoints
- [x] Cache frequently accessed data (5-10 min TTL)
- [x] Batch database operations
- [x] Use optimized JSON via `ApiResponse` (sonic-rs with `perf` feature)
- [x] System tuning: `ulimit -n 65536`, kernel params
- [x] Nginx reverse proxy with keep-alive
- [x] Health check endpoint for monitoring
- [x] `RUST_LOG=warn` or `error` in production

---

## Next Steps

- Read [performance.md](./performance.md) for technical deep-dive
- Check [architecture.md](./architecture.md) for system design
- See [deployment.md](./deployment.md) for cloud deployment guides
- Explore [caching.md](./caching.md) for advanced caching strategies

---

**Questions?** Check the [GitHub Discussions](https://github.com/yourusername/chopin/discussions) or open an issue.
