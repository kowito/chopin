# Quick Reference: Enable Logging in Chopin

## The Problem

When you start a Chopin server, you don't see any console output when requests come in, making debugging difficult.

## The Solution

Add **one line** before creating your App:

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();  // ← Add this line
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

## What You'll See

```
2026-02-15T23:00:00.123Z  INFO chopin_core::app: Running pending database migrations...
2026-02-15T23:00:00.234Z  INFO chopin_core::app: Migrations complete.
2026-02-15T23:00:00.345Z  INFO chopin_core::app: Using in-memory cache
2026-02-15T23:00:00.456Z  INFO chopin_core::app: Chopin server running on http://127.0.0.1:3000

# When requests come in:
2026-02-15T23:00:05.123Z  INFO tower_http::trace::on_request: started processing request method=GET uri=/api/users
2026-02-15T23:00:05.125Z  INFO tower_http::trace::on_response: finished processing request latency=2 ms status=200
```

## Other Options

```rust
// Pretty output for development
init_logging_pretty();

// JSON for production/log aggregation
init_logging_json();

// Custom log level
init_logging_with_level("debug");
```

## Control via Environment

```bash
# Show debug logs
RUST_LOG=debug cargo run

# Show only errors
RUST_LOG=error cargo run
```

## Full Documentation

See [docs/debugging-and-logging.md](./debugging-and-logging.md) for complete guide.
