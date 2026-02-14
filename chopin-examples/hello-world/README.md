# Hello World Example

The simplest possible Chopin application. One file, zero config.

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
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let app = chopin::App::new().await?;
    app.run().await?;
    Ok(())
}
```

That's it. Two lines of actual code.
