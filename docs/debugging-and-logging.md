# Debugging and Logging in Chopin

This guide explains how to enable request logging and debugging in your Chopin application.

## Quick Start

**To see request logs in your console, you must initialize the tracing subscriber** before creating your App:

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ⚠️ IMPORTANT: Call this BEFORE App::new()
    init_logging();
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

**That's it!** Now you'll see:
- ✅ Server startup messages
- ✅ Database migration logs
- ✅ HTTP request logs (in development mode)
- ✅ Cache connection status
- ✅ Error messages and stack traces

## What You'll See

After calling `init_logging()`, you'll see output like:

```
2026-02-15T23:00:00.123456Z  INFO chopin_core::app: Running pending database migrations...
2026-02-15T23:00:00.234567Z  INFO chopin_core::app: Migrations complete.
2026-02-15T23:00:00.345678Z  INFO chopin_core::app: Using in-memory cache
2026-02-15T23:00:00.456789Z  INFO chopin_core::app: Chopin server running on http://127.0.0.1:3000 (reuseport: false)

# When requests come in (development mode only):
2026-02-15T23:00:05.123456Z  INFO tower_http::trace::on_request: started processing request method=GET uri=/api/users
2026-02-15T23:00:05.125678Z  INFO tower_http::trace::on_response: finished processing request latency=2 ms status=200
```

## Logging Functions

Chopin provides four logging functions:

### `init_logging()` - Simple default (recommended)

Uses the `RUST_LOG` environment variable, defaults to `info` level:

```rust
init_logging();
```

### `init_logging_with_level()` - Set level programmatically

```rust
// Development: show everything including request traces
init_logging_with_level("debug");

// Production: only warnings and errors
init_logging_with_level("warn");
```

Available levels (from most to least verbose):
- `"trace"` - Extremely verbose, shows every detail
- `"debug"` - Debug info including HTTP request traces
- `"info"` - General information (default, recommended)
- `"warn"` - Only warnings and errors
- `"error"` - Only errors

### `init_logging_pretty()` - Pretty-formatted output (development)

Colored, multi-line output with more details:

```rust
init_logging_pretty();
```

Output example:
```
  2026-02-15T23:00:05.123456Z  INFO tower_http::trace::on_request
    with method: GET, uri: /api/users
    at tower-http-0.6.0/src/trace/on_request.rs:123
    on tokio-runtime-worker thread_id: ThreadId(4)

  started processing request
```

### `init_logging_json()` - JSON format (production)

For log aggregation systems (ELK, Datadog, CloudWatch):

```rust
init_logging_json();
```

Output example:
```json
{"timestamp":"2026-02-15T23:00:05.123456Z","level":"INFO","target":"tower_http::trace::on_request","fields":{"message":"started processing request","method":"GET","uri":"/api/users"}}
```

## Environment Variable Control

All logging functions respect the `RUST_LOG` environment variable:

```bash
# Show all logs at debug level
RUST_LOG=debug cargo run

# Show only warnings and errors
RUST_LOG=warn cargo run

# Fine-grained control per module
RUST_LOG=chopin_core=debug,tower_http=debug,sqlx=warn cargo run

# Production: errors only
RUST_LOG=error cargo run
```

Even if you call `init_logging_with_level("info")`, setting `RUST_LOG=debug` will override it.

## Development vs Production

### Development Mode (`APP_ENV=development` or not set)

When your app runs in development mode:
- Request tracing middleware is **automatically enabled**
- You'll see detailed logs for every HTTP request
- Includes request method, URI, duration, and status code

### Production Mode (`APP_ENV=production`)

In production mode:
- Request tracing middleware is **disabled by default** (for performance)
- You'll still see important logs (migrations, startup, errors)
- Recommended to use `init_logging_json()` for structured logs

You can always see logs by calling any `init_logging*()` function, regardless of the environment.

## Common Patterns

### Different Logging for Dev/Prod

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        // Development build
        init_logging_pretty();
    } else {
        // Release build
        init_logging_json();
    }
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

### Environment-based Logging

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("APP_ENV").as_deref() {
        Ok("production") => init_logging_json(),
        Ok("staging") => init_logging_with_level("info"),
        _ => init_logging_pretty(),
    }
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

### Custom Logging in Your Code

Once you've called `init_logging()`, you can use `tracing` macros in your own code:

```rust
use tracing::{info, warn, error, debug};

async fn my_handler() -> ApiResponse<String> {
    info!("Handler called");
    debug!("Processing request with detailed info");
    
    if some_condition {
        warn!("Something unusual happened");
    }
    
    match dangerous_operation() {
        Ok(result) => {
            info!("Operation successful");
            ApiResponse::success(result)
        }
        Err(e) => {
            error!("Operation failed: {}", e);
            ApiResponse::error(StatusCode::INTERNAL_SERVER_ERROR, "Operation failed")
        }
    }
}
```

## Troubleshooting

### "I don't see any logs!"

Make sure you called `init_logging()` **before** `App::new()`:

```rust
// ❌ Wrong - called after App::new()
let app = App::new().await?;
init_logging();  // Too late!

// ✅ Correct - called before App::new()
init_logging();
let app = App::new().await?;
```

### "I don't see request traces!"

Request traces are only enabled in development mode. Make sure:
1. You've called `init_logging()` (or any variant)
2. Your `APP_ENV` is not set to `production`
3. Your log level is at least `info` (or `debug`/`trace`)

```bash
# Make sure these are set correctly
RUST_LOG=debug APP_ENV=development cargo run
```

### "Too many logs!"

Reduce the log level:

```rust
init_logging_with_level("warn");  // Only warnings and errors
```

Or use environment variable:
```bash
RUST_LOG=warn cargo run
```

### "Logs aren't colored in production"

JSON logging doesn't support colors (it's for machines, not humans). In production, use:

```rust
init_logging_json();
```

Then pipe logs to a log aggregation system.

## Performance Considerations

Logging has a performance cost:

- **`info` level**: ~1-5% overhead (negligible)
- **`debug` level**: ~5-10% overhead (acceptable for development)
- **`trace` level**: ~15-30% overhead (avoid in production)

For maximum performance:
1. Use `RUST_LOG=warn` in production
2. Disable request tracing (automatically disabled when `APP_ENV=production`)
3. Use `init_logging_json()` for efficient structured output

## Examples

See these examples in the `chopin-examples/` directory:
- `hello-world` - Basic logging setup
- `basic-api` - Development logging
- `performance-mode` - Minimal logging for benchmarks
- `benchmark` - Production-style logging

Each example shows how to initialize logging at the start of `main()`.

## Summary

**Simple rule:** Call `init_logging()` before `App::new()` and you're done!

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();  // ← Add this one line
    
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

Now debugging is easy! 🎉
