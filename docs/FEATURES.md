# Chopin Features

## Core Features

| Feature | Status | Description |
|---------|--------|-------------|
| **Auth Module** | ✅ Opt-in | JWT + Argon2id, 2FA/TOTP, rate limiting, refresh tokens (vendor/chopin_auth) |
| **RBAC Permissions** | ✅ Core | Database-configurable role-based access control with caching |
| **Database ORM** | ✅ Core | SeaORM with auto-migrations (SQLite/PostgreSQL/MySQL) |
| **OpenAPI Docs** | ✅ Core | Auto-generated Scalar UI at `/api-docs` |
| **Caching** | ✅ Core | In-memory or Redis support |
| **File Storage** | ✅ Core | Local filesystem or S3-compatible (R2, MinIO) |
| **GraphQL** | ✅ Core | Optional async-graphql integration |
| **Testing Utils** | ✅ Core | `TestApp` with in-memory SQLite |
| **FastRoute** | ✅ Core | Zero-alloc static (~35ns) and per-request dynamic serialization (~100-150ns); bypasses Axum middleware with pre-computed headers, optional CORS, Cache-Control, method filtering |
| **Admin Panel** | 🔜 Opt-in | Django-style admin interface (vendor/chopin_admin) |
| **CMS Module** | 🔜 Opt-in | Content management system (vendor/chopin_cms) |

## Security Features (Production Default)

Chopin includes **9 production-ready security features**, all enabled by default:

| Feature | Endpoint / Mechanism | Description |
|---------|---------------------|-------------|
| **2FA/TOTP** | `POST /api/auth/totp/setup`, `/enable`, `/disable` | Google Authenticator compatible |
| **Rate Limiting** | Automatic on login | 5 attempts per 5 min (configurable) |
| **Account Lockout** | Automatic on login | Locks after 5 failed attempts for 15 min |
| **Refresh Tokens** | `POST /api/auth/refresh` | Automatic rotation with reuse detection |
| **Session Management** | `POST /api/auth/logout` | Server-side sessions, revoke one or all |
| **Password Reset** | `POST /api/auth/password-reset/request`, `/confirm` | Secure token-based flow |
| **Email Verification** | `POST /api/auth/verify-email` | Required on signup when enabled |
| **CSRF Protection** | Automatic | Token issued on login, verified on mutations |
| **IP/Device Tracking** | Automatic | Audit log of all login events |

## Security Configuration

Configure features and tune parameters via environment variables:

```bash
# Toggle features on/off
SECURITY_2FA=true
SECURITY_RATE_LIMIT=true
SECURITY_ACCOUNT_LOCKOUT=true
SECURITY_REFRESH_TOKENS=true
SECURITY_SESSION_MANAGEMENT=true
SECURITY_PASSWORD_RESET=true
SECURITY_EMAIL_VERIFICATION=true
SECURITY_CSRF=true
SECURITY_DEVICE_TRACKING=true

# Tune parameters
SECURITY_RATE_LIMIT_MAX=5            # Max attempts per window
SECURITY_RATE_LIMIT_WINDOW=300       # Window in seconds (5 min)
SECURITY_LOCKOUT_MAX=5               # Failed attempts before lockout
SECURITY_LOCKOUT_DURATION=900        # Lockout duration in seconds (15 min)
SECURITY_REFRESH_EXPIRY_DAYS=30      # Refresh token lifetime
SECURITY_RESET_EXPIRY=3600           # Password reset token TTL (1 hr)
SECURITY_EMAIL_VERIFY_EXPIRY=86400   # Email verification TTL (24 hrs)
SECURITY_MIN_PASSWORD_LENGTH=12      # Minimum password length
```

## RBAC Features Summary

Role-Based Access Control is built-in with:

- ✅ `#[login_required]` — Enforces JWT validation
- ✅ `#[permission_required("codename")]` — Enforces permission checks
- ✅ `PermissionGuard` extractor — Fine-grained conditional permission checks
- ✅ Database-configurable — Create/assign permissions at runtime without redeploying
- ✅ In-memory cache (5-min TTL) — Zero DB overhead for repeated checks
- ✅ Superuser bypass — `role = "superuser"` always passes all checks

See [modular-architecture.md](modular-architecture.md) for RBAC examples.

## What Makes Chopin Unique

### Django's Philosophy, Rust's Safety

- **ChopinModule Trait** — Every feature (Auth, Blog, Billing) is a self-contained module
- **Hub-and-Spoke** — Thin `chopin-core` hub prevents circular dependencies
- **MVSR Pattern** — Model-View-Service-Router separates HTTP from business logic
- **Compile-Time Verified** — Route conflicts and missing configs caught before deployment

### Performance at Scale

- **657K req/s** — Top-tier JSON throughput
- **3.75ms p99 latency** — Optimal for production
- **2-8x faster** than Node.js, Python, even competing Rust frameworks
- **50% cost savings** vs Node.js for same capacity
- **FastRoute** — 7-142× faster than Axum for predictable endpoints (static: ~35ns, dynamic: ~100-150ns)

### Production Ready

- **Zero-alloc hot paths** — Thread-local buffer reuse
- **Per-route optimization** — CORS, Cache-Control, method filtering, zero per-request cost
- **SO_REUSEPORT** — Per-core worker isolation in release mode
- **Automatic migrations** — SeaORM with version tracking

See [BENCHMARKS.md](BENCHMARKS.md) for detailed performance comparisons.
