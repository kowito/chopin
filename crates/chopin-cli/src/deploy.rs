use anyhow::Result;
use colored::*;
use std::path::Path;

pub fn generate_dockerfile(project_dir: &Path) -> Result<()> {
    let docker_path = project_dir.join("Dockerfile");
    let compose_path = project_dir.join("docker-compose.yml");

    if docker_path.exists() {
        println!("{} Dockerfile already exists.", "⚠".yellow());
    } else {
        let docker_content = r#"# syntax=docker/dockerfile:1
# ---------------------------------------------------
# Stage 1: Build Environment
# ---------------------------------------------------
FROM rust:1.85-slim AS builder

WORKDIR /usr/src/app
COPY . .

# Build for release with optimizations
RUN cargo build --release

# ---------------------------------------------------
# Stage 2: Runtime Environment
# ---------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/app/target/release/*-app /app/server
COPY --from=builder /usr/src/app/Chopin.toml /app/Chopin.toml

# Set default env vars for production
ENV HOST=0.0.0.0
ENV PORT=8080

EXPOSE 8080

CMD ["./server"]
"#;
        std::fs::write(&docker_path, docker_content)?;
        println!(
            "{} Generated optimized {}",
            "✓".green().bold(),
            "Dockerfile".cyan()
        );
    }

    if compose_path.exists() {
        println!("{} docker-compose.yml already exists.", "⚠".yellow());
    } else {
        let compose_content = r#"version: '3.8'

services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db:5432/postgres
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:15-alpine
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: postgres
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
"#;
        std::fs::write(&compose_path, compose_content)?;
        println!(
            "{} Generated {}",
            "✓".green().bold(),
            "docker-compose.yml".cyan()
        );
    }

    println!("\n{} Deployment ready!", "🚀".bold());
    println!(
        "  Run {} to start the production stack.",
        "docker-compose up --build -d".yellow()
    );

    Ok(())
}
