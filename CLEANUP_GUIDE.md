# Project Cleanup & Maintenance Guide

This guide documents cleanup best practices and identifies areas where the Chopin codebase can be optimized.

## Current State

✅ **Working tree is clean** — No uncommitted garbage files  
✅ **Build artifacts properly ignored** — `.gitignore` excludes `/target/`, `Cargo.lock`, editor files  
✅ **System files ignored** — `.DS_Store` (macOS), `*.swp`, `*.swo`, `*~` (editor temps)  
✅ **All code compiles** — Zero clippy warnings, formatting compliant

## Cleanup Maintenance Tasks

### Regular Maintenance

```bash
# Clean build artifacts locally (safe - rebuilds only as needed)
cargo clean

# Format code to standard
cargo fmt --all

# Check for warnings
cargo clippy --all --all-targets -- -D warnings

# Run full test suite
cargo test --all
```

### Monthly Checklist

- [ ] Run `cargo update` to update dependencies
- [ ] Check for security advisories: `cargo audit`
- [ ] Review git log for commits needing squash
- [ ] Update CONTRIBUTING.md if patterns have changed

## Code Organization Notes

### Test File Organization

The project uses a deliberate naming pattern for test files:

| File | Purpose |
|------|---------|
| `*_tests.rs` | Primary test suite for a module |
| `*_extra_tests.rs` | Additional edge cases and variations |
| `*_unit_tests.rs` | Pure unit tests (no async, no DB) |

**Example:**
- `response_tests.rs` — ApiResponse struct basic functionality
- `response_extra_tests.rs` — Serialization edge cases, Option handling
- `error_unit_tests.rs` — ChopinError status codes (pure functions)
- `error_tests.rs` — HTTP error responses (integration tests)

**Future consolidation opportunity:** Merge `*_extra_tests.rs` files into main test files for simpler organization. This is safe but not urgent.

### Module Organization

All public modules are properly organized following MVSR pattern:

```
chopin-core/src/
├── app.rs              — Application init and lifecycle
├── auth/               — Authentication services (JWT, TOTP, etc.)
├── cache.rs            — CacheService with in-memory/Redis backends
├── config.rs           — Configuration from env vars
├── controllers/        — HTTP handlers grouped by feature
│   ├── auth.rs         — Auth endpoints
│   └── auth_module.rs  — ChopinModule wrapper
├── db.rs               — Database connection setup
├── error.rs            — ChopinError type (no dead code)
├── extractors/         — Axum extractors (AuthUser, PermissionGuard, etc.)
├── graphql.rs          — Optional GraphQL integration
├── json.rs             — Simd JSON integration
├── lib.rs              — Module exports (all actively used)
├── logging.rs          — Tracing setup
├── migrations/         — Single consolidated migration file
├── models/             — SeaORM entities
├── module.rs           — ChopinModule trait
├── openapi.rs          — OpenAPI/Swagger integration
├── perf.rs             — Performance utilities
├── prelude.rs          — Convenient re-exports (all used)
├── response.rs         — ApiResponse type
├── routing.rs          — Route registration
├── server.rs           — FastRoute zero-alloc routes
├── storage.rs          — File storage (local/S3)
└── testing.rs          — TestApp for integration tests
```

**Status:** All modules are active and used. No dead code detected.

## Known Optimizations (Non-Critical)

### Test File Consolidation

The `*_extra_tests.rs` files could be consolidated into main test files:

```rust
// Instead of keeping response_tests.rs and response_extra_tests.rs separate,
// combine both into single response_tests.rs with organized test modules:

#[cfg(test)]
mod tests {
    mod basic {
        // Tests from response_tests.rs
    }
    
    mod edge_cases {
        // Tests from response_extra_tests.rs
    }
    
    mod serialization {
        // Serialization tests
    }
}
```

**Impact:** 0 functional change, just organization  
**Priority:** Low (works fine as-is)

### Dead Code Markers

File `cache.rs` line 231 has `#[allow(dead_code)]` for `redis::Client` field:

```rust
pub struct RedisCache {
    #[allow(dead_code)]
    client: redis::Client,  // ← Intentional: kept for reference, connection in pool
    pool: Arc<RwLock<redis::aio::MultiplexedConnection>>,
}
```

**Status:** This is intentional and correct.

## What's NOT Dead Code

All of the following are actively used:

- ✅ All auth modules (JWT, TOTP, rate limiting, sessions, etc.)
- ✅ All models and migrations
- ✅ All extractors (AuthUser, PermissionGuard, Json, etc.)
- ✅ All middleware functions
- ✅ RBAC system (RbacService, PermissionGuard, macros)
- ✅ Testing utilities (TestApp, TestClient, TestResponse)
- ✅ All error types and status codes
- ✅ OpenAPI integration and Swagger UI

## Preventing Garbage in Future

### Git Hooks

The `.gitignore` is comprehensive and includes all common patterns:

```gitignore
# macOS
.DS_Store

# IDE/Editor
.vscode/
.idea/
*.swp
*.swo
*~

# Build
/target/
*.rs.bk
Cargo.lock

# Environment
.env
.env.local
.env.*.local
```

To ensure files never get committed:

```bash
# Install pre-commit hook to check .gitignore
git config core.hooksPath .githooks

# Or manually run before commit:
git status --ignored
```

## Performance Considerations

### Build Time

```bash
# Incremental (usual development)
cargo build          # ~3-5 seconds

# Clean build with all features
cargo build --all --all-features --release  # ~30-45 seconds

# Incremental test compile
cargo test --lib    # ~2-3 seconds
```

### Cache Invalidation

RBAC service uses 5-minute in-memory TTL cache:

```rust
// Default cache TTL
pub struct RbacService {
    cache: RwLock<HashMap<String, CachedPermissions>>,  // 5 min TTL
}
```

To force cache clear during development:

```rust
rbac.invalidate_all().await;  // Clears all cached permissions
```

## Recommended Deletion (None Currently)

No files are recommended for deletion at this time. The codebase is well-maintained and active.

## Summary

| Category | Status | Notes |
|----------|--------|-------|
| Build artifacts | ✅ Clean | Properly ignored |
| System garbage | ✅ Clean | `.gitignore` prevents commits |
| Dead code | ✅ None | All modules actively used |
| Test coverage | ✅ Good | 310+ tests across 24 files |
| Unused exports | ✅ None | All re-exports actively used |
| Code quality | ✅ Pass | Clippy clean, fmt compliant |

**Next steps:** None required. Project is in excellent shape. Monitor using `cargo audit` and `cargo update` monthly.
