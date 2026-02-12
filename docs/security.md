# Security (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Authentication

Chopin provides built-in JWT authentication with Argon2id password hashing.

### Password Hashing

Passwords are hashed using **Argon2id** (the winner of the Password Hashing Competition):

```rust
use chopin_core::auth::password;

// Hash a password
let hash = password::hash("my-secret-password")?;

// Verify a password against a hash
let valid = password::verify("my-secret-password", &hash)?;
```

Argon2id is resistant to:
- Brute-force attacks (memory-hard)
- GPU cracking (memory-hard)
- Side-channel attacks (data-independent)

### JWT Tokens

Tokens use **HMAC-SHA256** signing:

```rust
use chopin_core::auth::jwt;

// Create a token
let token = jwt::create_token(user_id, &role, &config.jwt_secret, config.jwt_expiry_hours)?;

// Validate a token
let claims = jwt::validate_token(&token, &config.jwt_secret)?;
// claims.sub = user_id, claims.role = "user"
```

Token payload:

```json
{
  "sub": "1",           // User ID
  "role": "user",       // Role string
  "exp": 1707696000,    // Expiration timestamp
  "iat": 1707609600     // Issued-at timestamp
}
```

### Auth Endpoints

#### POST `/api/auth/signup`

```bash
curl -X POST http://localhost:3000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@example.com","username":"alice","password":"secret123"}'
```

```json
{
  "success": true,
  "data": {
    "access_token": "eyJ...",
    "user": {
      "id": 1,
      "email": "alice@example.com",
      "username": "alice",
      "role": "user"
    }
  }
}
```

#### POST `/api/auth/login`

```bash
curl -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@example.com","password":"secret123"}'
```

### Using Auth in Requests

Include the JWT in the `Authorization` header:

```bash
curl http://localhost:3000/api/protected \
  -H "Authorization: Bearer eyJ..."
```

### AuthUser Extractor

Extract the authenticated user in any handler:

```rust
use chopin_core::extractors::AuthUser;

async fn my_profile(user: AuthUser) -> ApiResponse<UserInfo> {
    // user.user_id — the user's database ID
    // user.role — the user's Role enum
    println!("User {} is authenticated", user.user_id);
    // ...
}
```

If the token is missing or invalid, Chopin returns a `401 Unauthorized` error automatically.

## Role-Based Access Control

See [Roles & Permissions](roles-permissions.md) for details.

## Security Best Practices

### JWT Secret

```env
# BAD — default dev secret
JWT_SECRET=chopin-dev-secret-change-me

# GOOD — random 32+ character string
JWT_SECRET=a8f2b9c4d5e6f7g8h9i0j1k2l3m4n5o6p7q8r9s0t1u2v3w4
```

Generate one:

```bash
openssl rand -base64 32
```

### Environment

```env
# Development: enables tracing middleware, verbose errors
ENVIRONMENT=development

# Production: disables tracing middleware, minimal error info
ENVIRONMENT=production
```

### CORS

Chopin uses `CorsLayer::permissive()` by default. For production, configure specific origins in your router setup.

### Rate Limiting

Use a reverse proxy (Nginx, Cloudflare) for rate limiting, or add tower middleware:

```rust
use tower::limit::RateLimitLayer;
use std::time::Duration;

let app = router.layer(RateLimitLayer::new(100, Duration::from_secs(1)));
```
