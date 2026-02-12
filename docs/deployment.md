# Deployment (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Build for Production

```bash
# Standard mode
cargo build --release

# Performance mode with mimalloc
cargo build --release --features perf
```

The binary is at `target/release/<your-crate-name>`.

## Environment Setup

### Required Variables

```env
DATABASE_URL=postgres://user:pass@db-host:5432/myapp
JWT_SECRET=generate-a-long-random-string-here
ENVIRONMENT=production
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

### Optional: Performance Mode

```env
SERVER_MODE=performance
```

## Docker

### Dockerfile

```dockerfile
# Build stage
FROM rust:1.82-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --features perf

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/my-app /usr/local/bin/app
EXPOSE 8080
CMD ["app"]
```

### Docker Compose

```yaml
version: "3.8"
services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://chopin:chopin@db:5432/chopin
      JWT_SECRET: ${JWT_SECRET}
      ENVIRONMENT: production
      SERVER_MODE: performance
      SERVER_HOST: 0.0.0.0
      SERVER_PORT: 8080
    depends_on:
      - db

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: chopin
      POSTGRES_PASSWORD: chopin
      POSTGRES_DB: chopin
    volumes:
      - pgdata:/var/lib/postgresql/data
    ports:
      - "5432:5432"

volumes:
  pgdata:
```

## Systemd Service

```ini
[Unit]
Description=Chopin App
After=network.target postgresql.service

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/my-app
ExecStart=/opt/my-app/app
Restart=always
RestartSec=5

Environment=DATABASE_URL=postgres://user:pass@localhost:5432/myapp
Environment=JWT_SECRET=your-secret-here
Environment=ENVIRONMENT=production
Environment=SERVER_MODE=performance
Environment=SERVER_HOST=0.0.0.0
Environment=SERVER_PORT=8080

# Performance tuning
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

## Reverse Proxy (Nginx)

```nginx
upstream chopin {
    server 127.0.0.1:8080;
    keepalive 64;
}

server {
    listen 80;
    server_name api.example.com;

    location / {
        proxy_pass http://chopin;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## OS Tuning (Linux)

For maximum performance, tune the kernel:

```bash
# /etc/sysctl.conf
net.core.somaxconn = 65536
net.core.netdev_max_backlog = 65536
net.ipv4.tcp_max_syn_backlog = 65536
net.ipv4.ip_local_port_range = 1024 65535
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 15
```

```bash
# Increase file descriptor limit
ulimit -n 65536
```

## Health Checks

The root endpoint `/` returns a JSON status:

```bash
curl http://localhost:8080/
# {"message":"Welcome to Chopin! 🎹","docs":"/api-docs","status":"running"}
```

## Database Migrations

Chopin runs migrations automatically on startup. For manual control:

```bash
chopin db migrate    # Run pending migrations
chopin db rollback   # Rollback last migration
chopin db status     # Show migration status
```

## Security Checklist

- [ ] Set a strong `JWT_SECRET` (at least 32 random characters)
- [ ] Set `ENVIRONMENT=production`
- [ ] Use PostgreSQL (not SQLite) for production
- [ ] Run behind a reverse proxy with TLS
- [ ] Set `SERVER_HOST=0.0.0.0` (not `127.0.0.1`) if behind a proxy
- [ ] Limit `MAX_UPLOAD_SIZE` appropriately
- [ ] Configure `REDIS_URL` for distributed caching
