# Configuration (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

Chopin loads configuration from environment variables with `.env` file support (via `dotenvy`).

## Server Mode

The `SERVER_MODE` variable controls how Chopin handles HTTP connections:

| Value | Description |
|-------|-------------|
| `standard` (default) | Full Axum pipeline with middleware, tracing, graceful shutdown |
| `performance` / `perf` / `fast` | Raw hyper HTTP/1.1 + SO_REUSEPORT multi-core accept loops |

```env
SERVER_MODE=performance
```

```rust
use chopin_core::config::ServerMode;

// Access programmatically
if config.server_mode == ServerMode::Performance {
    println!("Running in performance mode!");
}
```

## All Environment Variables

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVER_MODE` | `standard` | Server mode: `standard`, `performance`, `perf`, `fast` |
| `SERVER_HOST` | `127.0.0.1` | Bind address |
| `SERVER_PORT` | `3000` | Bind port |
| `ENVIRONMENT` | `development` | `development`, `production`, or `test` |
| `DATABASE_URL` | `sqlite://chopin.db?mode=rwc` | Database connection string |

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `JWT_SECRET` | `chopin-dev-secret-change-me` | JWT signing secret (HMAC-SHA256) |
| `JWT_EXPIRY_HOURS` | `24` | Token expiration in hours |

### Caching

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | *(none)* | Redis URL (e.g. `redis://127.0.0.1:6379`). Requires `redis` feature. |

### File Storage

| Variable | Default | Description |
|----------|---------|-------------|
| `UPLOAD_DIR` | `./uploads` | Local upload directory |
| `MAX_UPLOAD_SIZE` | `10485760` | Max file size in bytes (default 10 MB) |

### S3-Compatible Storage

Requires the `s3` feature flag.

| Variable | Default | Description |
|----------|---------|-------------|
| `S3_BUCKET` | *(none)* | S3 bucket name (enables S3 storage) |
| `S3_REGION` | *(none)* | AWS region (e.g. `us-east-1`) |
| `S3_ENDPOINT` | *(none)* | Custom endpoint (for R2, MinIO, etc.) |
| `S3_ACCESS_KEY_ID` | *(none)* | Access key (falls back to AWS credential chain) |
| `S3_SECRET_ACCESS_KEY` | *(none)* | Secret key |
| `S3_PUBLIC_URL` | *(none)* | Public base URL for objects (e.g. CDN URL) |
| `S3_PREFIX` | *(none)* | Key prefix / folder (e.g. `uploads/`) |

## Example `.env` Files

### Development

```env
DATABASE_URL=sqlite://dev.db?mode=rwc
JWT_SECRET=dev-secret-not-for-production
ENVIRONMENT=development
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
```

### Production (Standard Mode)

```env
DATABASE_URL=postgres://user:pass@db-host:5432/myapp
JWT_SECRET=a-very-long-random-secret-string-here
JWT_EXPIRY_HOURS=8
ENVIRONMENT=production
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
REDIS_URL=redis://redis-host:6379
```

### Production (Performance Mode)

```env
DATABASE_URL=postgres://user:pass@db-host:5432/myapp
JWT_SECRET=a-very-long-random-secret-string-here
ENVIRONMENT=production
SERVER_MODE=performance
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

## Accessing Config in Handlers

Configuration is available as `Arc<Config>` through the `AppState`:

```rust
use axum::extract::State;
use chopin_core::controllers::AppState;

async fn my_handler(State(state): State<AppState>) -> String {
    format!("Running on port {}", state.config.server_port)
}
```

## Config Struct Reference

```rust
pub struct Config {
    pub server_mode: ServerMode,       // Standard or Performance
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub server_host: String,
    pub server_port: u16,
    pub environment: String,           // development, production, test
    pub redis_url: Option<String>,
    pub upload_dir: String,
    pub max_upload_size: u64,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_public_url: Option<String>,
    pub s3_prefix: Option<String>,
}
```

### Helper Methods

```rust
config.is_dev()       // true when environment == "development"
config.has_s3()       // true when s3_bucket is set
config.server_addr()  // "127.0.0.1:3000"
```
