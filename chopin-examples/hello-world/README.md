# Hello World Example

The simplest possible Chopin application. One file, zero config.

Perfect for learning the basics before moving to [modular architecture](../../docs/modular-architecture.md).

## Run

```bash
cargo run -p chopin-hello-world
```

## What You Get

With zero configuration, Chopin provides:

| Endpoint | Description |
|----------|-------------|
| `GET /` | Welcome JSON |
| `POST /api/auth/signup` | User registration |
| `POST /api/auth/login` | User login |
| `GET /api-docs` | Scalar OpenAPI UI |
| `GET /api-docs/openapi.json` | Raw OpenAPI spec |

## Try It

```bash
# Welcome page
curl http://localhost:3000/

# Sign up
curl -X POST http://localhost:3000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@test.com","username":"alice","password":"secret123"}'

# Login
curl -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@test.com","password":"secret123"}'
```

## Source

```rust
use chopin_core::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();  // Enable console logs and request traces
    let app = App::new().await?;
    app.run().await?;
    Ok(())
}
```

That's it. Two lines of actual code.

## Next Steps

- See [basic-api example](../basic-api/) for MVSR pattern with modules
- Read [Modular Architecture Guide](../../docs/modular-architecture.md)
- Check [Multi-Tenancy Guide](../../docs/multi-tenancy.md)

**Note:** The `init_logging()` call enables console output for server startup, database migrations, and HTTP request traces. Without it, you won't see any logs. See the [Debugging & Logging Guide](../../docs/debugging-and-logging.md) for more details.
