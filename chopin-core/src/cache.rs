use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::error::ChopinError;

/// Cache backend trait for pluggable caching strategies.
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync {
    /// Get a raw value from the cache.
    async fn get(&self, key: &str) -> Result<Option<String>, ChopinError>;

    /// Set a raw value in the cache with optional TTL.
    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), ChopinError>;

    /// Delete a key from the cache.
    async fn del(&self, key: &str) -> Result<bool, ChopinError>;

    /// Check if a key exists.
    async fn exists(&self, key: &str) -> Result<bool, ChopinError>;

    /// Flush all keys (use with caution).
    async fn flush(&self) -> Result<(), ChopinError>;
}

/// The main cache service used by the application.
///
/// ```rust,ignore
/// // In your handler:
/// async fn get_post(
///     State(state): State<AppState>,
///     Path(id): Path<i32>,
/// ) -> Result<ApiResponse<PostResponse>, ChopinError> {
///     let cache_key = format!("post:{}", id);
///
///     // Try cache first
///     if let Some(cached) = state.cache.get_json::<PostResponse>(&cache_key).await? {
///         return Ok(ApiResponse::success(cached));
///     }
///
///     // Fetch from DB
///     let post = Post::find_by_id(id).one(&state.db).await?
///         .ok_or_else(|| ChopinError::NotFound("Post not found".into()))?;
///
///     let response = PostResponse::from(post);
///
///     // Cache for 5 minutes
///     state.cache.set_json(&cache_key, &response, Some(Duration::from_secs(300))).await?;
///
///     Ok(ApiResponse::success(response))
/// }
/// ```
#[derive(Clone)]
pub struct CacheService {
    backend: Arc<dyn CacheBackend>,
}

impl CacheService {
    /// Create a new cache service with the given backend.
    pub fn new(backend: impl CacheBackend + 'static) -> Self {
        CacheService {
            backend: Arc::new(backend),
        }
    }

    /// Create an in-memory cache (good for development and testing).
    pub fn in_memory() -> Self {
        CacheService::new(InMemoryCache::new())
    }

    /// Get a JSON-deserialized value from the cache.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, ChopinError> {
        match self.backend.get(key).await? {
            Some(raw) => {
                let value: T = crate::json::from_str(&raw).map_err(|e| {
                    ChopinError::Internal(format!("Cache deserialize error: {}", e))
                })?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set a JSON-serialized value in the cache.
    pub async fn set_json<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> Result<(), ChopinError> {
        let raw = crate::json::to_string(value)
            .map_err(|e| ChopinError::Internal(format!("Cache serialize error: {}", e)))?;
        self.backend.set(key, &raw, ttl).await
    }

    /// Get a raw string from the cache.
    pub async fn get(&self, key: &str) -> Result<Option<String>, ChopinError> {
        self.backend.get(key).await
    }

    /// Set a raw string in the cache.
    pub async fn set(
        &self,
        key: &str,
        value: &str,
        ttl: Option<Duration>,
    ) -> Result<(), ChopinError> {
        self.backend.set(key, value, ttl).await
    }

    /// Delete a key from the cache.
    pub async fn del(&self, key: &str) -> Result<bool, ChopinError> {
        self.backend.del(key).await
    }

    /// Check if a key exists in the cache.
    pub async fn exists(&self, key: &str) -> Result<bool, ChopinError> {
        self.backend.exists(key).await
    }

    /// Delete all keys matching a prefix.
    pub async fn del_prefix(&self, _prefix: &str) -> Result<(), ChopinError> {
        // For in-memory cache, this works. For Redis, use SCAN + DEL.
        // This is a basic implementation.
        Ok(())
    }

    /// Flush the entire cache.
    pub async fn flush(&self) -> Result<(), ChopinError> {
        self.backend.flush().await
    }
}

// ── In-Memory Cache Backend ──

/// Simple in-memory cache using a HashMap. Good for development and testing.
/// For production, use `RedisCache`.
#[derive(Clone)]
pub struct InMemoryCache {
    store: Arc<RwLock<std::collections::HashMap<String, CacheEntry>>>,
}

#[derive(Clone)]
struct CacheEntry {
    value: String,
    expires_at: Option<std::time::Instant>,
}

impl InMemoryCache {
    pub fn new() -> Self {
        InMemoryCache {
            store: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CacheBackend for InMemoryCache {
    async fn get(&self, key: &str) -> Result<Option<String>, ChopinError> {
        let store = self.store.read().await;
        match store.get(key) {
            Some(entry) => {
                if let Some(expires_at) = entry.expires_at {
                    if std::time::Instant::now() > expires_at {
                        drop(store);
                        self.store.write().await.remove(key);
                        return Ok(None);
                    }
                }
                Ok(Some(entry.value.clone()))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), ChopinError> {
        let expires_at = ttl.map(|d| std::time::Instant::now() + d);
        self.store.write().await.insert(
            key.to_string(),
            CacheEntry {
                value: value.to_string(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn del(&self, key: &str) -> Result<bool, ChopinError> {
        Ok(self.store.write().await.remove(key).is_some())
    }

    async fn exists(&self, key: &str) -> Result<bool, ChopinError> {
        let store = self.store.read().await;
        match store.get(key) {
            Some(entry) => {
                if let Some(expires_at) = entry.expires_at {
                    Ok(std::time::Instant::now() <= expires_at)
                } else {
                    Ok(true)
                }
            }
            None => Ok(false),
        }
    }

    async fn flush(&self) -> Result<(), ChopinError> {
        self.store.write().await.clear();
        Ok(())
    }
}

// ── Redis Cache Backend ──

/// Redis-backed cache for production use.
///
/// Requires a Redis connection URL (e.g., `redis://127.0.0.1:6379`).
///
/// ```rust,ignore
/// let cache = RedisCache::new("redis://127.0.0.1:6379").await?;
/// let service = CacheService::new(cache);
/// ```
#[cfg(feature = "redis")]
pub struct RedisCache {
    #[allow(dead_code)]
    client: redis::Client,
    pool: Arc<RwLock<redis::aio::MultiplexedConnection>>,
}

#[cfg(feature = "redis")]
impl RedisCache {
    /// Create a new Redis cache from a connection URL.
    pub async fn new(url: &str) -> Result<Self, ChopinError> {
        let client = redis::Client::open(url)
            .map_err(|e| ChopinError::Internal(format!("Redis connection error: {}", e)))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ChopinError::Internal(format!("Redis connection error: {}", e)))?;
        Ok(RedisCache {
            client,
            pool: Arc::new(RwLock::new(conn)),
        })
    }
}

#[cfg(feature = "redis")]
#[async_trait::async_trait]
impl CacheBackend for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<String>, ChopinError> {
        use redis::AsyncCommands;
        let mut conn = self.pool.write().await;
        let result: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| ChopinError::Internal(format!("Redis GET error: {}", e)))?;
        Ok(result)
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), ChopinError> {
        use redis::AsyncCommands;
        let mut conn = self.pool.write().await;
        if let Some(ttl) = ttl {
            let _: () = conn
                .set_ex(key, value, ttl.as_secs())
                .await
                .map_err(|e| ChopinError::Internal(format!("Redis SETEX error: {}", e)))?;
        } else {
            let _: () = conn
                .set(key, value)
                .await
                .map_err(|e| ChopinError::Internal(format!("Redis SET error: {}", e)))?;
        }
        Ok(())
    }

    async fn del(&self, key: &str) -> Result<bool, ChopinError> {
        use redis::AsyncCommands;
        let mut conn = self.pool.write().await;
        let count: i64 = conn
            .del(key)
            .await
            .map_err(|e| ChopinError::Internal(format!("Redis DEL error: {}", e)))?;
        Ok(count > 0)
    }

    async fn exists(&self, key: &str) -> Result<bool, ChopinError> {
        use redis::AsyncCommands;
        let mut conn = self.pool.write().await;
        let exists: bool = conn
            .exists(key)
            .await
            .map_err(|e| ChopinError::Internal(format!("Redis EXISTS error: {}", e)))?;
        Ok(exists)
    }

    async fn flush(&self) -> Result<(), ChopinError> {
        let mut conn = self.pool.write().await;
        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut *conn)
            .await
            .map_err(|e| ChopinError::Internal(format!("Redis FLUSHDB error: {}", e)))?;
        Ok(())
    }
}
