# Security Guide

Comprehensive security best practices for Chopin applications.

## Table of Contents

- [Authentication & Authorization](#authentication--authorization)
- [Password Security](#password-security)
- [JWT Security](#jwt-security)
- [Database Security](#database-security)
- [Input Validation](#input-validation)
- [HTTPS/TLS](#httpstls)
- [CORS](#cors)
- [Rate Limiting](#rate-limiting)
- [Security Headers](#security-headers)
- [Dependencies](#dependencies)
- [Production Checklist](#production-checklist)

## Authentication & Authorization

### Built-in Authentication

Chopin provides secure JWT-based authentication out of the box with:
- Password hashing (Argon2id)
- JWT token generation and validation
- Hardware-accelerated cryptography (ring crate)

### Protecting Endpoints

Use the `AuthUser` extractor:

```rust
use chopin_core::extractors::AuthUser;

async fn protected_endpoint(
    AuthUser(user_id): AuthUser,
) -> Result<ApiResponse<Data>, ChopinError> {
    // user_id is guaranteed to be valid
    // Token is verified automatically
}
```

**What it does**:
1. Extracts `Authorization: Bearer <token>` header
2. Validates JWT signature
3. Checks expiration
4. Returns 401 if invalid/missing

### Authorization Patterns

**Resource ownership**:

```rust
async fn delete_post(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
    Path(post_id): Path<i32>,
) -> Result<ApiResponse<()>, ChopinError> {
    let post = Post::find_by_id(post_id)
        .one(&app.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Post not found".to_string()))?;
    
    // Check ownership
    if post.author_id != user_id {
        return Err(ChopinError::Forbidden("Not your post".to_string()));
    }
    
    post.delete(&app.db).await?;
    Ok(ApiResponse::success(()))
}
```

**Role-based access** (custom implementation):

```rust
#[derive(Debug)]
pub enum Role {
    User,
    Admin,
}

async fn admin_only(
    AuthUser(user_id): AuthUser,
    State(app): State<AppState>,
) -> Result<ApiResponse<Data>, ChopinError> {
    let user = User::find_by_id(user_id)
        .one(&app.db)
        .await?
        .ok_or_else(|| ChopinError::Unauthorized("User not found".to_string()))?;
    
    if user.role != "admin" {
        return Err(ChopinError::Forbidden("Admin access required".to_string()));
    }
    
    // Admin-only logic
}
```

## Password Security

### Argon2id

Chopin uses **Argon2id** for password hashing:

- Memory-hard (resistant to GPU/ASIC attacks)
- Winner of Password Hashing Competition
- Automatic salt generation
- Configurable cost parameters

**Never store plaintext passwords!**

### Password Requirements

Enforce strong passwords:

```rust
use validator::Validate;

#[derive(Deserialize, Validate)]
struct SignupRequest {
    #[validate(email)]
    email: String,
    
    #[validate(length(min = 3, max = 50))]
    username: String,
    
    #[validate(length(min = 8, max = 128))]
    password: String,
}
```

**Recommended minimum**:
- 8 characters
- Mix of letters, numbers, symbols
- No common passwords
- Not username/email

### Password Validation

Add complexity checks:

```rust
fn is_strong_password(password: &str) -> bool {
    password.len() >= 8
        && password.chars().any(|c| c.is_lowercase())
        && password.chars().any(|c| c.is_uppercase())
        && password.chars().any(|c| c.is_numeric())
        && password.chars().any(|c| !c.is_alphanumeric())
}

async fn signup(
    Json(payload): Json<SignupRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    if !is_strong_password(&payload.password) {
        return Err(ChopinError::Validation(
            "Password must contain uppercase, lowercase, number, and symbol".to_string()
        ));
    }
    
    // Proceed with signup
}
```

### Password Reset

Implement securely:

```rust
// 1. Generate secure token
let reset_token = generate_secure_token();
let expires_at = Utc::now() + Duration::hours(1);

// 2. Store token (hashed)
let token_hash = hash_token(&reset_token);
save_reset_token(user_id, &token_hash, expires_at).await?;

// 3. Send email with token (not hash!)
send_reset_email(&user.email, &reset_token).await?;

// 4. Verify token on reset
let token_hash = hash_token(&submitted_token);
verify_reset_token(user_id, &token_hash).await?;
```

## JWT Security

### Secret Key

**Generate strong secret**:

```bash
# Minimum 32 bytes
openssl rand -base64 32

# Or 64 bytes for extra security
openssl rand -base64 64
```

**`.env.example`**:
```env
# Generate with: openssl rand -base64 32
JWT_SECRET=your-32-byte-or-longer-random-secret-here
```

**NEVER**:
- Use default/weak secrets in production
- Commit secrets to git
- Share secrets between environments
- Log or expose secrets

### Token Expiration

Set appropriate expiry:

```env
# Production: shorter expiry
JWT_EXPIRY_HOURS=6

# Development: longer for convenience
JWT_EXPIRY_HOURS=24
```

**Recommendations**:
- Public APIs: 1-6 hours
- Internal APIs: 6-24 hours
- High-security: 15-60 minutes + refresh tokens

### Refresh Tokens

Implement refresh token flow:

```rust
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    Json(payload): Json<RefreshRequest>,
) -> Result<ApiResponse<TokenResponse>, ChopinError> {
    // 1. Validate refresh token
    let user_id = validate_refresh_token(&payload.refresh_token)?;
    
    // 2. Generate new access token
    let access_token = jwt::generate(user_id, 6)?;
    
    // 3. Optionally rotate refresh token
    let new_refresh_token = generate_refresh_token(user_id)?;
    
    Ok(ApiResponse::success(TokenResponse {
        access_token,
        refresh_token: new_refresh_token,
    }))
}
```

### Token Revocation

Track revoked tokens:

```rust
// Add to database
CREATE TABLE revoked_tokens (
    token_hash VARCHAR(64) PRIMARY KEY,
    revoked_at TIMESTAMP NOT NULL
);

// Check on validation
async fn validate_token(token: &str) -> Result<Claims> {
    let claims = jwt::decode(token)?;
    
    let token_hash = hash_token(token);
    if is_token_revoked(&token_hash).await? {
        return Err(ChopinError::Unauthorized("Token revoked".to_string()));
    }
    
    Ok(claims)
}
```

## Database Security

### Connection Security

**Always use SSL/TLS in production**:

```env
# PostgreSQL
DATABASE_URL=postgres://user:pass@host/db?sslmode=require

# MySQL
DATABASE_URL=mysql://user:pass@host/db?ssl_mode=REQUIRED
```

### SQL Injection Prevention

SeaORM protects against SQL injection automatically:

```rust
// Safe - parameterized query
Post::find()
    .filter(Column::Title.contains(&user_input))
    .all(&db)
    .await?;

// Avoid raw SQL with user input
// If necessary, use parameterized statements
```

### Database Credentials

**Never hardcode credentials**:

```rust
// ❌ DON'T
let db_url = "postgres://user:password@host/db";

// ✅ DO
let db_url = std::env::var("DATABASE_URL")?;
```

### Least Privilege

Create database user with minimal permissions:

```sql
-- Create app-specific user
CREATE USER myapp WITH PASSWORD 'secure-password';

-- Grant only necessary permissions
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO myapp;

-- No DROP, ALTER, or admin privileges
```

### Sensitive Data

**Encrypt at rest**:
- Use encrypted database volumes
- Encrypt sensitive columns (credit cards, SSN)
- Use database encryption features

**Encrypted columns**:

```rust
use aes_gcm::{Aes256Gcm, Key, Nonce};

async fn store_sensitive(data: &str) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = generate_nonce();
    
    let encrypted = cipher.encrypt(&nonce, data.as_bytes())?;
    Ok(base64::encode(&encrypted))
}
```

## Input Validation

### Validate All Input

**Always validate user input**:

```rust
use validator::Validate;

#[derive(Deserialize, Validate)]
struct CreatePostRequest {
    #[validate(length(min = 1, max = 200))]
    title: String,
    
    #[validate(length(min = 1, max = 10000))]
    body: String,
    
    #[validate(url)]
    thumbnail_url: Option<String>,
}

async fn create(
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<Post>, ChopinError> {
    // Validate
    payload.validate()
        .map_err(|e| ChopinError::Validation(format!("{}", e)))?;
    
    // Proceed
}
```

### Sanitize Output

Prevent XSS when returning HTML/text:

```rust
use ammonia::clean;

fn sanitize_html(input: &str) -> String {
    clean(input)
}
```

### File Uploads

Validate file uploads:

```rust
const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const ALLOWED_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

async fn upload(
    multipart: Multipart,
) -> Result<ApiResponse<UploadResponse>, ChopinError> {
    // Validate size
    if file.len() > MAX_FILE_SIZE {
        return Err(ChopinError::Validation("File too large".to_string()));
    }
    
    // Validate type
    if !ALLOWED_TYPES.contains(&file.content_type()) {
        return Err(ChopinError::Validation("Invalid file type".to_string()));
    }
    
    // Scan for malware (if applicable)
    // Store with random filename
}
```

## HTTPS/TLS

### Use Reverse Proxy

**Nginx**:

```nginx
server {
    listen 443 ssl http2;
    server_name api.example.com;
    
    ssl_certificate /etc/letsencrypt/live/api.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.example.com/privkey.pem;
    
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    ssl_prefer_server_ciphers on;
    
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name api.example.com;
    return 301 https://$server_name$request_uri;
}
```

**Caddy** (automatic HTTPS):

```
api.example.com {
    reverse_proxy localhost:8080
}
```

### Let's Encrypt

Free SSL certificates:

```bash
# Install certbot
sudo apt install certbot

# Get certificate
sudo certbot certonly --standalone -d api.example.com
```

## CORS

### Configure CORS

Chopin uses permissive CORS by default (**change in production**):

```rust
use tower_http::cors::{CorsLayer, Any};

// Development: Allow all
let cors = CorsLayer::permissive();

// Production: Specific origins
let cors = CorsLayer::new()
    .allow_origin("https://app.example.com".parse::<HeaderValue>()?)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

router.layer(cors)
```

### Security Implications

**Permissive CORS allows**:
- Any origin to make requests
- Credentials to be sent
- All headers

**Production CORS should**:
- Whitelist specific origins
- Limit allowed methods
- Limit allowed headers

## Rate Limiting

### Implement Rate Limiting

Prevent abuse and DDoS:

```rust
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

// 100 requests per minute per IP
let governor_conf = Box::new(
    GovernorConfigBuilder::default()
        .per_millisecond(600) // 100 req/min
        .burst_size(10)
        .finish()
        .unwrap()
);

router.layer(GovernorLayer {
    config: Box::leak(governor_conf),
})
```

### Per-User Rate Limiting

```rust
async fn rate_limited_endpoint(
    AuthUser(user_id): AuthUser,
) -> Result<ApiResponse<Data>, ChopinError> {
    if !check_rate_limit(user_id).await? {
        return Err(ChopinError::TooManyRequests);
    }
    
    // Process request
}
```

## Security Headers

### Add Security Headers

```rust
use tower_http::set_header::SetResponseHeaderLayer;
use axum::http::header;

router
    .layer(SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        HeaderValue::from_static("x-xss-protection"),
        HeaderValue::from_static("1; mode=block"),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        HeaderValue::from_static("strict-transport-security"),
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    ))
```

**Headers explained**:
- `X-Content-Type-Options: nosniff` - Prevent MIME sniffing
- `X-Frame-Options: DENY` - Prevent clickjacking
- `X-XSS-Protection` - Enable XSS filter
- `Strict-Transport-Security` - Force HTTPS

## Dependencies

### Security Audits

Regularly check dependencies:

```bash
# Install cargo-audit
cargo install cargo-audit

# Run audit
cargo audit

# Fix vulnerabilities
cargo audit fix
```

### Keep Dependencies Updated

```bash
# Update dependencies
cargo update

# Check outdated
cargo outdated

# Update to latest
cargo upgrade
```

### Minimal Dependencies

- Only add necessary dependencies
- Review dependency code
- Check download counts and maintenance
- Prefer well-maintained crates

## Production Checklist

### ✅ Before Deployment

**Authentication & Authorization**:
- [ ] Strong JWT_SECRET (32+ bytes)
- [ ] Appropriate token expiry
- [ ] Protected endpoints use AuthUser
- [ ] Authorization checks implemented

**Passwords**:
- [ ] Using Argon2id (built-in)
- [ ] Password requirements enforced
- [ ] No passwords in logs

**Database**:
- [ ] SSL/TLS enabled
- [ ] Least-privilege user
- [ ] Sensitive data encrypted
- [ ] Backups enabled

**Network**:
- [ ] HTTPS enforced
- [ ] CORS configured (not permissive)
- [ ] Rate limiting enabled
- [ ] Security headers set

**Input/Output**:
- [ ] All inputs validated
- [ ] Outputs sanitized
- [ ] File uploads validated
- [ ] Error messages don't leak info

**Configuration**:
- [ ] Secrets in environment vars
- [ ] No secrets in code/git
- [ ] .env in .gitignore
- [ ] Production environment set

**Dependencies**:
- [ ] `cargo audit` clean
- [ ] Dependencies updated
- [ ] No unnecessary dependencies

**Monitoring**:
- [ ] Logging enabled
- [ ] Error tracking set up
- [ ] Alerts configured

### ✅ Post-Deployment

- [ ] Test authentication flow
- [ ] Verify HTTPS works
- [ ] Check security headers
- [ ] Test rate limiting
- [ ] Monitor error logs
- [ ] Set up security alerts

## Common Vulnerabilities

### Prevent SQL Injection

✅ Use SeaORM (automatic protection)  
❌ Avoid raw SQL with user input

### Prevent XSS

✅ Sanitize HTML output  
✅ Set Content-Type headers correctly  
✅ Use CSP headers (if serving HTML)

### Prevent CSRF

✅ Use SameSite cookies  
✅ Verify Origin header  
✅ Use CSRF tokens for state-changing operations

### Prevent Timing Attacks

```rust
use constant_time_eq::constant_time_eq;

// ✅ DO - constant time comparison
if constant_time_eq(hash1.as_bytes(), hash2.as_bytes()) {
    // Valid
}

// ❌ DON'T - timing attack vulnerable
if hash1 == hash2 {
    // Timing leak!
}
```

---

## Resources

- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [CWE Top 25](https://cwe.mitre.org/top25/)
- [Let's Encrypt](https://letsencrypt.org/)

Security is a continuous process. Stay vigilant and keep learning!
