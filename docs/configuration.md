# Configuration Guide

Chopin uses environment variables for all configuration, loaded via `.env` files.

## Quick Start

Create a `.env` file in your project root:

```env
DATABASE_URL=sqlite://app.db?mode=rwc
JWT_SECRET=your-secret-key-change-in-production
JWT_EXPIRY_HOURS=24
SERVER_PORT=3000
SERVER_HOST=127.0.0.1
ENVIRONMENT=development
```

## Configuration Options

### Database

#### `DATABASE_URL` (required)

Database connection string.

**SQLite** (default for development):
```env
DATABASE_URL=sqlite://app.db?mode=rwc
DATABASE_URL=sqlite::memory:  # In-memory (testing)
```

**PostgreSQL**:
```env
DATABASE_URL=postgres://username:password@localhost:5432/dbname
DATABASE_URL=postgres://user:pass@localhost/mydb?sslmode=require
```

**MySQL**:
```env
DATABASE_URL=mysql://username:password@localhost:3306/dbname
DATABASE_URL=mysql://root@localhost/mydb
```

**Connection Pool Options**:
```env
DATABASE_URL=postgres://user:pass@localhost/db?max_connections=100&min_connections=5
```

**Default**: `sqlite://chopin.db?mode=rwc`

---

### JWT Authentication

#### `JWT_SECRET` (required in production)

Secret key used to sign JWT tokens. **Must be kept secret**.

**Development**:
```env
JWT_SECRET=chopin-dev-secret-change-me
```

**Production** (generate strong secret):
```bash
# Generate a secure random secret
openssl rand -base64 32

# Use in .env
JWT_SECRET=your-generated-secret-here
```

**Important**:
- Use at least 32 characters
- Use random, unpredictable values
- Never commit secrets to version control
- Rotate periodically in production

**Default**: `chopin-dev-secret-change-me` (dev only)

#### `JWT_EXPIRY_HOURS`

Token expiration time in hours.

```env
JWT_EXPIRY_HOURS=24    # 1 day
JWT_EXPIRY_HOURS=168   # 1 week
JWT_EXPIRY_HOURS=1     # 1 hour (strict)
```

**Default**: `24`

**Recommendations**:
- Development: 24-168 hours
- Production: 1-24 hours
- High-security apps: 1-2 hours with refresh tokens

---

### Server

#### `SERVER_PORT`

HTTP server port.

```env
SERVER_PORT=3000     # Development
SERVER_PORT=8080     # Alternative
SERVER_PORT=80       # Production (requires privileges)
```

**Default**: `3000`

#### `SERVER_HOST`

HTTP server bind address.

```env
SERVER_HOST=127.0.0.1    # Localhost only
SERVER_HOST=0.0.0.0      # All interfaces (production)
```

**Default**: `127.0.0.1`

**Security**:
- Development: Use `127.0.0.1` (localhost only)
- Production: Use `0.0.0.0` behind a reverse proxy
- **Never** expose directly to internet without TLS

---

### Environment

#### `ENVIRONMENT`

Runtime environment identifier.

```env
ENVIRONMENT=development
ENVIRONMENT=production
ENVIRONMENT=staging
ENVIRONMENT=test
```

**Default**: `development`

**Effects**:
- Logging verbosity
- Error detail exposure
- Performance optimizations

---

### Caching

#### `REDIS_URL` (optional)

Redis connection URL for caching. If not provided, Chopin uses in-memory caching.

```env
# Not set - uses in-memory cache (default)
REDIS_URL=redis://127.0.0.1:6379
REDIS_URL=redis://username:password@host:6379/0
REDIS_URL=redis://localhost:6379?password=secret
```

**Default**: None (in-memory cache)

**When to use Redis**:
- Production environments
- Multi-instance deployments (shared cache)
- Large cache requirements
- Cross-request cache sharing

**Requires**: `redis` feature flag in `Cargo.toml`

```toml
chopin-core = { version = "0.1", features = ["redis"] }
```

---

### File Uploads

#### `UPLOAD_DIR`

Directory for storing uploaded files.

```env
UPLOAD_DIR=./uploads              # Relative to project root
UPLOAD_DIR=/var/www/uploads       # Absolute path
UPLOAD_DIR=/tmp/uploads           # Temporary storage
```

**Default**: `./uploads`

**Production recommendations**:
- Use absolute paths
- Ensure directory is writable
- Configure backups
- Consider object storage (S3) for scale

#### `MAX_UPLOAD_SIZE`

Maximum file upload size in bytes.

```env
MAX_UPLOAD_SIZE=10485760      # 10 MB (default)
MAX_UPLOAD_SIZE=52428800      # 50 MB
MAX_UPLOAD_SIZE=104857600     # 100 MB
MAX_UPLOAD_SIZE=1048576       # 1 MB
```

**Default**: `10485760` (10 MB)

**Calculation**:
- 1 MB = 1,048,576 bytes
- 10 MB = 10,485,760 bytes
- 50 MB = 52,428,800 bytes
- 100 MB = 104,857,600 bytes

**Usage in code**:
```rust
if config.is_dev() {
    // Development-only features
}
```

---

## Configuration Loading

### Priority Order

1. Environment variables (highest priority)
2. `.env` file
3. Defaults (lowest priority)

### Example

```env
# .env file
SERVER_PORT=3000
```

```bash
# Override with environment variable
SERVER_PORT=8080 cargo run
# → Server runs on port 8080
```

### Loading Process

```rust
use chopin_core::Config;

let config = Config::from_env()?;
```

Chopin automatically:
1. Reads `.env` file via `dotenvy`
2. Parses environment variables
3. Applies defaults for missing values
4. Validates required fields

---

## Environment-Specific Configuration

### Development

`.env` (local, gitignored):
```env
DATABASE_URL=sqlite://dev.db?mode=rwc
JWT_SECRET=dev-secret-not-for-production
JWT_EXPIRY_HOURS=168
SERVER_PORT=3000
SERVER_HOST=127.0.0.1
ENVIRONMENT=development
```

### Production

Set via environment variables (no .env file):
```bash
export DATABASE_URL="postgres://user:pass@db-host/prod"
export JWT_SECRET="$(openssl rand -base64 32)"
export JWT_EXPIRY_HOURS=24
export SERVER_PORT=8080
export SERVER_HOST=0.0.0.0
export ENVIRONMENT=production
```

### Testing

Tests use in-memory SQLite automatically:
```rust
// No .env needed for tests
let app = TestApp::new().await;
```

Override in tests:
```rust
std::env::set_var("JWT_EXPIRY_HOURS", "1");
let config = Config::from_env()?;
```

---

## Advanced Configuration

### Custom Configuration

Extend `Config` struct:

```rust
// In your app
pub struct AppConfig {
    pub chopin: chopin_core::Config,
    pub stripe_key: String,
    pub redis_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AppConfig {
            chopin: chopin_core::Config::from_env()?,
            stripe_key: std::env::var("STRIPE_KEY")?,
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost".to_string()),
        })
    }
}
```

### Runtime Configuration

Access config in handlers:

```rust
use axum::extract::State;

async fn handler(State(app): State<AppState>) -> impl IntoResponse {
    let db_url = &app.config.database_url;
    // Use config...
}
```

---

## Database Configuration

### Connection Pool

Default settings (good for most apps):
- Max connections: 100
- Min connections: 5
- Connect timeout: 8s
- Idle timeout: 8s

Customize via URL query params:
```env
DATABASE_URL=postgres://user:pass@host/db?max_connections=200&min_connections=10
```

### SQLite Options

```env
# Read-write-create mode
DATABASE_URL=sqlite://app.db?mode=rwc

# Read-only mode
DATABASE_URL=sqlite://app.db?mode=ro

# In-memory database
DATABASE_URL=sqlite::memory:

# WAL mode (better concurrency)
DATABASE_URL=sqlite://app.db?mode=rwc&journal_mode=wal
```

### PostgreSQL Options

```env
# SSL required
DATABASE_URL=postgres://user:pass@host/db?sslmode=require

# Specific schema
DATABASE_URL=postgres://user:pass@host/db?search_path=myschema

# Connection timeout
DATABASE_URL=postgres://user:pass@host/db?connect_timeout=10
```

### MySQL Options

```env
# SSL mode
DATABASE_URL=mysql://user:pass@host/db?ssl_mode=required

# Timezone
DATABASE_URL=mysql://user:pass@host/db?time_zone=%2B00:00
```

---

## Security Best Practices

### ✅ DO

- Use strong, random `JWT_SECRET` in production
- Store secrets in environment variables, not code
- Use different secrets per environment
- Rotate secrets periodically
- Use `.gitignore` to exclude `.env`
- Use SSL/TLS for database connections in production
- Limit JWT expiry time

### ❌ DON'T

- Commit `.env` to version control
- Use default secrets in production
- Share secrets between environments
- Log sensitive configuration values
- Expose secrets in error messages
- Use weak or predictable secrets

---

## Configuration Validation

Chopin validates configuration at startup:

```rust
let config = Config::from_env()?;
// Error if required fields missing or invalid
```

**Validation checks**:
- `DATABASE_URL` is parseable
- `JWT_SECRET` is non-empty
- `SERVER_PORT` is valid (1-65535)
- `JWT_EXPIRY_HOURS` is positive

---

## .env.example

Always commit a `.env.example` template:

```env
# Database
DATABASE_URL=sqlite://app.db?mode=rwc

# JWT
JWT_SECRET=your-secret-key-here
JWT_EXPIRY_HOURS=24

# Server
SERVER_PORT=3000
SERVER_HOST=127.0.0.1

# Environment
ENVIRONMENT=development
```

Team members copy to `.env` and customize.

---

## Troubleshooting

### "Failed to load .env file"

**Cause**: `.env` file not found.

**Solution**: Create `.env` in project root or set environment variables.

### "DATABASE_URL must be set"

**Cause**: Missing required configuration.

**Solution**: Add to `.env`:
```env
DATABASE_URL=sqlite://app.db?mode=rwc
```

### "Failed to connect to database"

**Cause**: Invalid connection string or database unavailable.

**Solutions**:
- Check `DATABASE_URL` format
- Verify database server is running
- Check network connectivity
- Verify credentials

### Token Validation Fails

**Cause**: `JWT_SECRET` changed after tokens issued.

**Solution**: Regenerate tokens (users must log in again).

---

## Examples

### Minimal Production Config

```bash
export DATABASE_URL="postgres://appuser:$(cat /secrets/db-password)@db-primary.internal:5432/production"
export JWT_SECRET="$(cat /secrets/jwt-secret)"
export JWT_EXPIRY_HOURS=6
export SERVER_HOST=0.0.0.0
export SERVER_PORT=8080
export ENVIRONMENT=production
```

### Docker Compose

```yaml
version: '3.8'
services:
  app:
    image: my-chopin-app
    environment:
      DATABASE_URL: postgres://user:password@db:5432/myapp
      JWT_SECRET: ${JWT_SECRET}
      SERVER_PORT: 8080
      SERVER_HOST: 0.0.0.0
      ENVIRONMENT: production
    ports:
      - "8080:8080"
  
  db:
    image: postgres:16
    environment:
      POSTGRES_DB: myapp
      POSTGRES_USER: user
      POSTGRES_PASSWORD: password
```

### Kubernetes ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: chopin-config
data:
  DATABASE_URL: postgres://user:password@postgres-service:5432/prod
  SERVER_PORT: "8080"
  SERVER_HOST: 0.0.0.0
  ENVIRONMENT: production
  JWT_EXPIRY_HOURS: "6"
```

---

## Summary

✅ Use `.env` for local development  
✅ Use environment variables in production  
✅ Keep secrets in `.env` (gitignored)  
✅ Document all options in `.env.example`  
✅ Validate configuration at startup  
✅ Use strong secrets in production  

Chopin's configuration system is simple, secure, and production-ready.
