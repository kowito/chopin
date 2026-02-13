# CLI Cheatsheet

**Last Updated:** February 2026

Quick reference for all `chopin` CLI commands.

## Project

```bash
chopin new my-app                  # Create a new project
chopin run                         # Start the dev server
chopin info                        # Show framework info
```

## Code Generation

```bash
chopin generate model <name> [fields...]      # Generate model + migration
chopin generate controller <name>             # Generate controller with CRUD
```

### Field Types

```bash
chopin generate model post \
  title:string \
  body:text \
  views:integer \
  score:float \
  published:boolean \
  published_at:datetime \
  author_id:integer
```

## Database

```bash
chopin db migrate                  # Run pending migrations
chopin db rollback                 # Rollback last migration
chopin db reset                    # Drop & recreate all tables
chopin db seed                     # Run seed data
chopin db status                   # Show migration status
chopin createsuperuser             # Create admin user
```

## Documentation

```bash
chopin docs export openapi.json    # Export OpenAPI spec
chopin docs export openapi.yaml    # Export as YAML
```

## Running

```bash
# Development (standard mode)
cargo run

# Production (standard mode)
cargo run --release

# Production (performance mode)
SERVER_MODE=performance cargo run --release

# Maximum performance
SERVER_MODE=performance cargo run --release --features perf
```
