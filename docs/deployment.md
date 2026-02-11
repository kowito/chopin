# Deployment Guide

Complete guide to deploying Chopin applications to production.

## Table of Contents

- [Quick Start](#quick-start)
- [Building for Production](#building-for-production)
- [Docker Deployment](#docker-deployment)
- [Cloud Platforms](#cloud-platforms)
- [Database Setup](#database-setup)
- [Environment Configuration](#environment-configuration)
- [Monitoring & Logging](#monitoring--logging)
- [Security Checklist](#security-checklist)

## Quick Start

1. Build release binary
2. Configure production environment
3. Set up database
4. Deploy and run

```bash
# Build
cargo build --release

# Configure
export DATABASE_URL="postgres://user:pass@host/db"
export JWT_SECRET="$(openssl rand -base64 32)"

# Run
./target/release/my-api
```

## Building for Production

### Release Build

```bash
cargo build --release
```

The binary is in `target/release/`.

### Optimizations

Your `Cargo.toml` should include:

```toml
[profile.release]
opt-level = 3              # Maximum optimization
lto = "fat"               # Full link-time optimization
codegen-units = 1          # Single codegen unit
strip = true              # Strip debug symbols
```

### Binary Size

Reduce binary size:

```toml
[profile.release]
opt-level = "z"           # Optimize for size
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

### Build Scripts

Create `build.sh`:

```bash
#!/bin/bash
set -e

echo "Building for production..."
cargo build --release

echo "Binary size:"
ls -lh target/release/my-api

echo "Testing binary..."
./target/release/my-api --version

echo "Build complete!"
```

## Docker Deployment

### Dockerfile

Multi-stage build for minimal size:

```dockerfile
# Build Stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source
COPY src ./src

# Build release
RUN cargo build --release

# Runtime Stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/my-api /app/my-api

# Create non-root user
RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /app
USER appuser

# Expose port
EXPOSE 8080

# Run
CMD ["./my-api"]
```

### .dockerignore

```
target/
.env
.git/
*.db
*.log
```

### Build Image

```bash
docker build -t my-api:latest .
```

### Run Container

```bash
docker run -d \
  --name my-api \
  -p 8080:8080 \
  -e DATABASE_URL="postgres://user:pass@db/myapp" \
  -e JWT_SECRET="your-secret" \
  -e SERVER_PORT=8080 \
  -e SERVER_HOST=0.0.0.0 \
  -e ENVIRONMENT=production \
  my-api:latest
```

### Docker Compose

Complete stack with database:

```yaml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://myapp:password@db:5432/myapp
      JWT_SECRET: ${JWT_SECRET}
      SERVER_PORT: 8080
      SERVER_HOST: 0.0.0.0
      ENVIRONMENT: production
      RUST_LOG: info
    depends_on:
      db:
        condition: service_healthy
    restart: unless-stopped

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: myapp
      POSTGRES_USER: myapp
      POSTGRES_PASSWORD: password
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U myapp"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: unless-stopped

volumes:
  postgres_data:
```

### Health Checks

Add health endpoint:

```rust
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

// In router
.route("/health", get(health))
```

Update Dockerfile:

```dockerfile
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```

## Cloud Platforms

### AWS (EC2 + RDS)

**1. Build and Upload**:

```bash
# Build for Linux
cargo build --release --target x86_64-unknown-linux-musl

# Upload to EC2
scp target/x86_64-unknown-linux-musl/release/my-api ec2-user@instance:/home/ec2-user/
```

**2. Systemd Service**:

```ini
# /etc/systemd/system/my-api.service
[Unit]
Description=My Chopin API
After=network.target

[Service]
Type=simple
User=ec2-user
WorkingDirectory=/home/ec2-user
ExecStart=/home/ec2-user/my-api
Restart=always
RestartSec=10

Environment="DATABASE_URL=postgres://user:pass@rds-endpoint:5432/mydb"
Environment="JWT_SECRET=your-secret"
Environment="SERVER_PORT=8080"
Environment="SERVER_HOST=0.0.0.0"
Environment="ENVIRONMENT=production"
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable my-api
sudo systemctl start my-api
sudo systemctl status my-api
```

**3. RDS Configuration**:

```bash
export DATABASE_URL="postgres://username:password@mydb.abc123.us-east-1.rds.amazonaws.com:5432/myapp"
```

### AWS (ECS Fargate)

**task-definition.json**:

```json
{
  "family": "my-api",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "256",
  "memory": "512",
  "containerDefinitions": [
    {
      "name": "my-api",
      "image": "123456789.dkr.ecr.us-east-1.amazonaws.com/my-api:latest",
      "portMappings": [
        {
          "containerPort": 8080,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {"name": "SERVER_PORT", "value": "8080"},
        {"name": "SERVER_HOST", "value": "0.0.0.0"},
        {"name": "ENVIRONMENT", "value": "production"}
      ],
      "secrets": [
        {
          "name": "DATABASE_URL",
          "valueFrom": "arn:aws:secretsmanager:region:account:secret:db-url"
        },
        {
          "name": "JWT_SECRET",
          "valueFrom": "arn:aws:secretsmanager:region:account:secret:jwt-secret"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/my-api",
          "awslogs-region": "us-east-1",
          "awslogs-stream-prefix": "ecs"
        }
      }
    }
  ]
}
```

### Google Cloud Platform (Cloud Run)

**Deploy**:

```bash
# Build and push
gcloud builds submit --tag gcr.io/PROJECT_ID/my-api

# Deploy
gcloud run deploy my-api \
  --image gcr.io/PROJECT_ID/my-api \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --set-env-vars="SERVER_PORT=8080,SERVER_HOST=0.0.0.0" \
  --set-secrets="DATABASE_URL=database-url:latest,JWT_SECRET=jwt-secret:latest"
```

### DigitalOcean (App Platform)

**app.yaml**:

```yaml
name: my-api
services:
  - name: api
    github:
      repo: your-username/your-repo
      branch: main
      deploy_on_push: true
    build_command: cargo build --release
    run_command: ./target/release/my-api
    http_port: 8080
    instance_size_slug: basic-xxs
    instance_count: 1
    envs:
      - key: SERVER_PORT
        value: "8080"
      - key: SERVER_HOST
        value: "0.0.0.0"
      - key: ENVIRONMENT
        value: "production"
      - key: DATABASE_URL
        type: SECRET
        value: ${db.DATABASE_URL}
      - key: JWT_SECRET
        type: SECRET
databases:
  - name: db
    engine: PG
    version: "16"
```

### Fly.io

**fly.toml**:

```toml
app = "my-api"
primary_region = "sjc"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 1

[[services]]
  protocol = "tcp"
  internal_port = 8080

  [[services.ports]]
    port = 80
    handlers = ["http"]

  [[services.ports]]
    port = 443
    handlers = ["tls", "http"]

[env]
  SERVER_PORT = "8080"
  SERVER_HOST = "0.0.0.0"
  ENVIRONMENT = "production"
```

**Deploy**:

```bash
# Set secrets
fly secrets set JWT_SECRET="$(openssl rand -base64 32)"
fly secrets set DATABASE_URL="postgres://..."

# Deploy
fly deploy
```

### Heroku

**Procfile**:

```
web: ./target/release/my-api
```

**Deploy**:

```bash
heroku create my-api
heroku addons:create heroku-postgresql:mini

heroku config:set JWT_SECRET="$(openssl rand -base64 32)"
heroku config:set SERVER_HOST="0.0.0.0"
heroku config:set RUST_BUILDPACK_URL="https://github.com/emk/heroku-buildpack-rust.git"

git push heroku main
```

## Database Setup

### PostgreSQL (Production)

**1. Install**:

```bash
# Ubuntu/Debian
sudo apt install postgresql postgresql-contrib

# macOS
brew install postgresql@16
```

**2. Create Database**:

```sql
CREATE DATABASE myapp;
CREATE USER myapp WITH ENCRYPTED PASSWORD 'secure-password';
GRANT ALL PRIVILEGES ON DATABASE myapp TO myapp;
```

**3. Connection String**:

```bash
export DATABASE_URL="postgres://myapp:secure-password@localhost:5432/myapp"
```

**4. Migrations**:

Migrations run automatically on startup. For manual control:

```bash
# SSH to server
./my-api # Migrations apply on startup
```

### MySQL

**Connection String**:

```bash
export DATABASE_URL="mysql://user:password@localhost:3306/myapp"
```

### Managed Databases

**AWS RDS**:
```
postgres://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:5432/myapp
```

**Google Cloud SQL**:
```
postgres://user:pass@/mydb?host=/cloudsql/project:region:instance
```

**DigitalOcean**:
```
postgres://user:pass@db-postgresql-nyc3-12345.b.db.ondigitalocean.com:25060/myapp?sslmode=require
```

## Environment Configuration

### Production .env

**Never commit to git!**

```env
# Database
DATABASE_URL=postgres://user:secure-pass@db-host:5432/production_db

# JWT (generate strong secret)
JWT_SECRET=<output of: openssl rand -base64 32>
JWT_EXPIRY_HOURS=6

# Server
SERVER_PORT=8080
SERVER_HOST=0.0.0.0

# Environment
ENVIRONMENT=production
RUST_LOG=info
```

### Secrets Management

**AWS Secrets Manager**:

```bash
aws secretsmanager create-secret \
  --name my-api/jwt-secret \
  --secret-string "$(openssl rand -base64 32)"

# Retrieve in app
JWT_SECRET=$(aws secretsmanager get-secret-value --secret-id my-api/jwt-secret --query SecretString --output text)
```

**Kubernetes Secrets**:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-api-secrets
type: Opaque
stringData:
  jwt-secret: "your-secret-here"
  database-url: "postgres://..."
```

**HashiCorp Vault**:

```bash
vault kv put secret/my-api \
  jwt_secret="$(openssl rand -base64 32)" \
  database_url="postgres://..."
```

## Monitoring & Logging

### Logging

**Configure log level**:

```bash
export RUST_LOG=info
export RUST_LOG=my_api=debug,chopin_core=info
```

**Structured logging**:

```rust
use tracing::{info, error};

info!(user_id = %user.id, "User logged in");
error!(error = %e, "Database connection failed");
```

### Metrics

Add metrics endpoint:

```rust
async fn metrics() -> impl IntoResponse {
    // Export Prometheus metrics
    ApiResponse::success(serde_json::json!({
        "requests_total": REQUESTS.get(),
        "errors_total": ERRORS.get(),
    }))
}
```

### Health Checks

```rust
async fn health() -> impl IntoResponse {
    StatusCode::OK
}

async fn readiness(State(app): State<AppState>) -> impl IntoResponse {
    // Check database
    match app.db.ping().await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}
```

### External Monitoring

**DataDog**:
```bash
docker run -d \
  --name datadog-agent \
  -e DD_API_KEY=<api-key> \
  -e DD_SITE=datadoghq.com \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  datadog/agent:latest
```

**New Relic**:
```bash
export NEW_RELIC_LICENSE_KEY=<key>
export NEW_RELIC_APP_NAME="My API"
```

**Sentry** (error tracking):
```rust
sentry::init(("https://key@sentry.io/project", sentry::ClientOptions {
    release: sentry::release_name!(),
    ..Default::default()
}));
```

## Security Checklist

### ✅ Pre-Deployment

- [ ] Strong JWT_SECRET (32+ random bytes)
- [ ] Secure DATABASE_URL with strong password
- [ ] TLS/SSL enabled for database connections
- [ ] HTTPS enforced (reverse proxy)
- [ ] Secrets not in code/git
- [ ] Dependencies updated (`cargo update`)
- [ ] Security audit (`cargo audit`)
- [ ] Rate limiting enabled
- [ ] CORS configured correctly
- [ ] Input validation on all endpoints

### ✅ Infrastructure

- [ ] Firewall rules (only necessary ports)
- [ ] Database not publicly accessible
- [ ] Regular backups enabled
- [ ] Monitoring and alerting set up
- [ ] SSL/TLS certificates valid
- [ ] Non-root user running application
- [ ] Resource limits configured

### ✅ Post-Deployment

- [ ] Test all endpoints
- [ ] Verify authentication works
- [ ] Check logs for errors
- [ ] Monitor resource usage
- [ ] Set up automated backups
- [ ] Document deployment process

## Troubleshooting

### Server Won't Start

**Check logs**:
```bash
journalctl -u my-api -f
```

**Common issues**:
- Port already in use
- Database unreachable
- Missing environment variables
- Permission denied

### Database Connection Fails

**Test connection**:
```bash
psql "$DATABASE_URL"
```

**Check**:
- Connection string format
- Network connectivity
- Firewall rules
- Database server running

### High Memory Usage

**Profile memory**:
```bash
cargo install cargo-profie
cargo profile --release
```

**Solutions**:
- Increase connection pool limits
- Add pagination
- Use streaming responses

### Slow Performance

**Check**:
- Database indexes
- N+1 queries
- Connection pool size
- Resource allocation

---

## Quick Reference

### Common Commands

```bash
# Build release
cargo build --release

# Build Docker image
docker build -t my-api .

# Run tests
cargo test

# Check security
cargo audit

# Update dependencies
cargo update
```

### Environment Variables

```bash
DATABASE_URL=postgres://user:pass@host/db
JWT_SECRET=<32+ byte secret>
JWT_EXPIRY_HOURS=6
SERVER_PORT=8080
SERVER_HOST=0.0.0.0
ENVIRONMENT=production
RUST_LOG=info
```

---

## Resources

- [Docker Documentation](https://docs.docker.com/)
- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [AWS Documentation](https://docs.aws.amazon.com/)
- [Configuration Guide](configuration.md)
- [Security Guide](security.md)

Deploy with confidence using Chopin!
