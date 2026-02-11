# Caching Guide

Chopin provides a flexible caching layer with support for in-memory caching (default) and Redis (optional).

## Quick Start

Caching is available out-of-the-box via `AppState`:

```rust
use chopin_core::{CacheService, ApiResponse, ChopinError};
use std::time::Duration;

async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let cache_key = format!("post:{}", id);

    // Try cache first
    if let Some(cached) = state.cache.get_json::<PostResponse>(&cache_key).await? {
        return Ok(ApiResponse::success(cached));
    }

    // Fetch from database
    let post = Post::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("Post not found".into()))?;

    let response = PostResponse::from(post);

    // Cache for 5 minutes
    state.cache.set_json(&cache_key, &response, Some(Duration::from_secs(300))).await?;

    Ok(ApiResponse::success(response))
}
```

## Cache Backends

### In-Memory Cache (Default)

The default cache backend uses a HashMap stored in memory. It's:
- **Fast** - No network overhead
- **Simple** - Zero configuration
- **Limited** - Doesn't share across instances

Perfect for:
- Development
- Single-instance deployments
- Small cache requirements
- Testing

**No configuration needed** - automatically enabled.

### Redis Cache (Production)

For production deployments, enable Redis caching:

**1. Enable the feature flag:**

```toml
# Cargo.toml
[dependencies]
chopin-core = { version = "0.1", features = ["redis"] }
```

**2. Configure Redis URL:**

```env
# .env
REDIS_URL=redis://127.0.0.1:6379
```

**3. Chopin automatically uses Redis when available:**

```rust
// In app initialization, Chopin checks for REDIS_URL
// and falls back to in-memory if not configured
let app = App::new().await?;
```

Redis is ideal for:
- Production environments
- Multi-instance deployments (shared cache)
- Large cache requirements
- Cross-request cache sharing
- Persistent caching across restarts

## Cache API

### `get_json<T>` / `set_json<T>`

Store and retrieve Rust types as JSON:

```rust
#[derive(Serialize, Deserialize)]
struct UserProfile {
    id: i32,
    username: String,
    bio: String,
}

// Set
let profile = UserProfile { ... };
state.cache.set_json("user:123", &profile, Some(Duration::from_secs(3600))).await?;

// Get
if let Some(cached) = state.cache.get_json::<UserProfile>("user:123").await? {
    return Ok(cached);
}
```

### `get` / `set`

Store raw strings:

```rust
// Set
state.cache.set("session:abc", "user_id:123", Some(Duration::from_secs(1800))).await?;

// Get
if let Some(data) = state.cache.get("session:abc").await? {
    println!("Session data: {}", data);
}
```

### `del`

Delete a key:

```rust
// Returns true if key existed
let existed = state.cache.del("post:123").await?;
```

### `exists`

Check if a key exists:

```rust
if state.cache.exists("user:123").await? {
    println!("User is cached");
}
```

### `flush`

Clear all cache entries (use with caution):

```rust
state.cache.flush().await?;
```

## TTL (Time To Live)

Set expiration times for cache entries:

```rust
use std::time::Duration;

// 5 minutes
state.cache.set_json("key", &value, Some(Duration::from_secs(300))).await?;

// 1 hour
state.cache.set_json("key", &value, Some(Duration::from_secs(3600))).await?;

// 1 day
state.cache.set_json("key", &value, Some(Duration::from_secs(86400))).await?;

// No expiration (permanent until manually deleted)
state.cache.set_json("key", &value, None).await?;
```

## Cache Key Patterns

Use descriptive, hierarchical keys:

```rust
// Good ✓
format!("user:{}", user_id)
format!("post:{}:comments", post_id)
format!("session:{}", token)
format!("api:v1:products:{}:inventory", product_id)

// Bad ✗
"data"
"123"
"temp"
```

## Caching Strategies

### Cache-Aside (Lazy Loading)

Fetch from cache first, then database:

```rust
async fn get_product(id: i32, state: &AppState) -> Result<Product, ChopinError> {
    let key = format!("product:{}", id);
    
    // 1. Try cache
    if let Some(cached) = state.cache.get_json(&key).await? {
        return Ok(cached);
    }
    
    // 2. Fetch from DB
    let product = Product::find_by_id(id).one(&state.db).await?
        .ok_or_else(|| ChopinError::NotFound("Product not found".into()))?;
    
    // 3. Store in cache
    state.cache.set_json(&key, &product, Some(Duration::from_secs(600))).await?;
    
    Ok(product)
}
```

### Write-Through

Update cache whenever data changes:

```rust
async fn update_product(
    id: i32,
    data: UpdateProductRequest,
    state: &AppState,
) -> Result<Product, ChopinError> {
    // Update database
    let product = Product::find_by_id(id).one(&state.db).await?
        .ok_or_else(|| ChopinError::NotFound("Product not found".into()))?;
    
    let mut active: product::ActiveModel = product.into();
    active.name = Set(data.name);
    active.price = Set(data.price);
    let updated = active.update(&state.db).await?;
    
    // Update cache
    let key = format!("product:{}", id);
    state.cache.set_json(&key, &updated, Some(Duration::from_secs(600))).await?;
    
    Ok(updated)
}
```

### Cache Invalidation

Delete stale cache entries:

```rust
async fn delete_post(id: i32, state: &AppState) -> Result<(), ChopinError> {
    // Delete from database
    Post::delete_by_id(id).exec(&state.db).await?;
    
    // Invalidate cache
    state.cache.del(&format!("post:{}", id)).await?;
    
    // Invalidate related caches
    state.cache.del(&format!("post:{}:comments", id)).await?;
    state.cache.del("posts:list").await?;
    
    Ok(())
}
```

## Best Practices

### 1. Cache Expensive Operations

Cache database queries, API calls, and computations:

```rust
// ✓ Good - expensive query
let key = "dashboard:stats";
if let Some(stats) = state.cache.get_json(key).await? {
    return Ok(stats);
}
let stats = calculate_expensive_stats(&state.db).await?;
state.cache.set_json(key, &stats, Some(Duration::from_secs(300))).await?;

// ✗ Bad - simple primary key lookup (fast already)
let user = User::find_by_id(id).one(&state.db).await?;
```

### 2. Set Appropriate TTLs

Match TTL to data volatility:

```rust
// Static data - long TTL (1 day)
state.cache.set_json("categories", &cats, Some(Duration::from_secs(86400))).await?;

// User session - medium TTL (30 minutes)
state.cache.set_json("session:abc", &sess, Some(Duration::from_secs(1800))).await?;

// Live data - short TTL (1 minute)
state.cache.set_json("stock:TSLA", &price, Some(Duration::from_secs(60))).await?;
```

### 3. Invalidate on Writes

Always invalidate cache when data changes:

```rust
// After updating a post
state.cache.del(&format!("post:{}", id)).await?;

// After adding a comment
state.cache.del(&format!("post:{}:comments", post_id)).await?;
```

### 4. Handle Cache Failures Gracefully

Cache misses should not break your application:

```rust
// ✓ Good - fall back to DB on cache error
let product = match state.cache.get_json(&key).await {
    Ok(Some(p)) => p,
    _ => fetch_from_db(id, &state.db).await?,
};

// ✗ Bad - propagate cache errors
let product = state.cache.get_json(&key).await?
    .ok_or_else(|| ChopinError::NotFound("Not found".into()))?;
```

### 5. Namespace Your Keys

Use prefixes to organize cache entries:

```rust
format!("api:v1:user:{}", id)      // API version
format!("db:posts:{}", slug)        // Database entities
format!("session:{}", token)        // Sessions
format!("temp:upload:{}", uuid)     // Temporary data
```

## Performance Tips

- **Batch operations** when possible
- Use **appropriate serialization** (JSON vs raw strings)
- **Monitor cache hit rates** in production
- Consider **cache warming** for frequently accessed data
- Use **Redis pipelining** for multiple operations

## Testing with Cache

In tests, use in-memory cache (always enabled):

```rust
#[tokio::test]
async fn test_caching() {
    let app = TestApp::new().await;
    
    // Cache is available in tests
    app.client.get("/api/products/1").await;  // Miss
    app.client.get("/api/products/1").await;  // Hit
}
```

## Production Deployment

### Redis Configuration

```yaml
# docker-compose.yml
services:
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data
    command: redis-server --appendonly yes

volumes:
  redis_data:
```

### Environment Variables

```env
# Production .env
REDIS_URL=redis://redis:6379
# Or with password
REDIS_URL=redis://:password@redis:6379
# Or cloud provider
REDIS_URL=redis://user:pass@redis-12345.cloud.redislabs.com:12345
```

## Monitoring

Track cache performance:

```rust
// Log cache hits/misses
if let Some(cached) = state.cache.get_json(&key).await? {
    tracing::debug!("Cache HIT: {}", key);
    return Ok(cached);
}
tracing::debug!("Cache MISS: {}", key);
```

Consider adding metrics:
- Cache hit rate
- Average response time (cache vs DB)
- Cache size / memory usage
- Eviction rate (Redis)
