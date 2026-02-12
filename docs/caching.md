# Caching (v0.1.1)

**Current Version:** 0.1.1 | **Last Updated:** February 2026

## Overview

Chopin provides a unified `CacheService` with two backends:

- **In-memory** (default) — no external dependencies, suitable for single-instance deployments
- **Redis** — for distributed caching across multiple instances. Requires the `redis` feature.

## Configuration

### In-Memory (Default)

Works out of the box with no configuration. Data lives in a `DashMap` within the process.

### Redis

```toml
# Cargo.toml
[dependencies]
chopin-core = { version = "0.1", features = ["redis"] }
```

```env
REDIS_URL=redis://127.0.0.1:6379
```

Chopin automatically connects to Redis if `REDIS_URL` is set and the `redis` feature is enabled. If Redis is unavailable, it falls back to in-memory caching.

## Usage

### Access Cache in Handlers

```rust
use axum::extract::State;
use chopin_core::controllers::AppState;

async fn get_data(State(state): State<AppState>) -> ApiResponse<String> {
    // Check cache first
    if let Some(cached) = state.cache.get("my-key").await {
        return ApiResponse::success(cached);
    }

    // Compute value
    let value = "expensive computation result".to_string();

    // Cache for 5 minutes (300 seconds)
    state.cache.set("my-key", &value, Some(300)).await;

    ApiResponse::success(value)
}
```

### CacheService API

```rust
// Get a value
let value: Option<String> = cache.get("key").await;

// Set with TTL (seconds)
cache.set("key", "value", Some(3600)).await;

// Set without TTL (never expires, memory cache only)
cache.set("key", "value", None).await;

// Delete
cache.delete("key").await;
```

### Direct Initialization

```rust
use chopin_core::cache::CacheService;

// In-memory cache
let cache = CacheService::in_memory();

// Redis cache (requires redis feature)
#[cfg(feature = "redis")]
let cache = {
    let redis = chopin_core::cache::RedisCache::new("redis://127.0.0.1:6379").await?;
    CacheService::new(redis)
};
```

## Caching Patterns

### Cache-Aside Pattern

```rust
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<UserResponse>, ChopinError> {
    let cache_key = format!("user:{}", id);

    // 1. Check cache
    if let Some(cached) = state.cache.get(&cache_key).await {
        let user: UserResponse = serde_json::from_str(&cached).unwrap();
        return Ok(ApiResponse::success(user));
    }

    // 2. Query database
    let user = User::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ChopinError::NotFound("User not found".into()))?;

    let response = UserResponse::from(user);

    // 3. Store in cache (10 minutes)
    let json = serde_json::to_string(&response).unwrap();
    state.cache.set(&cache_key, &json, Some(600)).await;

    Ok(ApiResponse::success(response))
}
```

### Cache Invalidation

```rust
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateUser>,
) -> Result<ApiResponse<UserResponse>, ChopinError> {
    // Update database...
    let updated = update_in_db(&state.db, id, body).await?;

    // Invalidate cache
    state.cache.delete(&format!("user:{}", id)).await;

    Ok(ApiResponse::success(updated))
}
```

## Implementation Details

### In-Memory Cache

- Uses `DashMap<String, CacheEntry>` (lock-free concurrent hash map)
- TTL is checked on every `get()` call (lazy expiration)
- No background cleanup (expired entries remain until accessed)
- Memory is bounded by the number of unique keys

### Redis Cache

- Uses `redis::aio::ConnectionManager` for connection pooling
- TTL is handled natively by Redis (`SETEX` command)
- Supports distributed caching across multiple instances
- Automatic reconnection on connection loss
