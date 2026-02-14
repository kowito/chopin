use chopin_core::cache::CacheService;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CachedUser {
    id: u32,
    username: String,
    email: String,
}

#[tokio::test]
async fn test_in_memory_cache_basic_operations() {
    let cache = CacheService::in_memory();

    // Initially, key should not exist
    let result = cache.get("test_key").await.unwrap();
    assert!(result.is_none());

    // Set a value
    cache
        .set("test_key", "test_value", None)
        .await
        .expect("Failed to set");

    // Get the value back
    let result = cache.get("test_key").await.unwrap();
    assert_eq!(result, Some("test_value".to_string()));

    // Check existence
    let exists = cache.exists("test_key").await.unwrap();
    assert!(exists);

    // Delete the key
    let deleted = cache.del("test_key").await.unwrap();
    assert!(deleted);

    // Key should no longer exist
    let result = cache.get("test_key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_cache_json_operations() {
    let cache = CacheService::in_memory();

    let user = CachedUser {
        id: 1,
        username: "john_doe".to_string(),
        email: "john@example.com".to_string(),
    };

    // Set JSON
    cache
        .set_json("user:1", &user, None)
        .await
        .expect("Failed to set JSON");

    // Get JSON back
    let retrieved: Option<CachedUser> = cache.get_json("user:1").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), user);

    // Non-existent key should return None
    let missing: Option<CachedUser> = cache.get_json("user:999").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_cache_with_ttl() {
    let cache = CacheService::in_memory();

    // Set with 1 second TTL
    cache
        .set("expiring_key", "value", Some(Duration::from_millis(100)))
        .await
        .expect("Failed to set with TTL");

    // Should exist immediately
    let result = cache.get("expiring_key").await.unwrap();
    assert_eq!(result, Some("value".to_string()));

    // Wait for expiration
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should be expired now
    let result = cache.get("expiring_key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_cache_overwrite() {
    let cache = CacheService::in_memory();

    cache.set("key", "value1", None).await.unwrap();
    let result = cache.get("key").await.unwrap();
    assert_eq!(result, Some("value1".to_string()));

    // Overwrite
    cache.set("key", "value2", None).await.unwrap();
    let result = cache.get("key").await.unwrap();
    assert_eq!(result, Some("value2".to_string()));
}

#[tokio::test]
async fn test_cache_flush() {
    let cache = CacheService::in_memory();

    cache.set("key1", "value1", None).await.unwrap();
    cache.set("key2", "value2", None).await.unwrap();
    cache.set("key3", "value3", None).await.unwrap();

    // Verify all exist
    assert!(cache.exists("key1").await.unwrap());
    assert!(cache.exists("key2").await.unwrap());
    assert!(cache.exists("key3").await.unwrap());

    // Flush all
    cache.flush().await.expect("Failed to flush");

    // All should be gone
    assert!(!cache.exists("key1").await.unwrap());
    assert!(!cache.exists("key2").await.unwrap());
    assert!(!cache.exists("key3").await.unwrap());
}

#[tokio::test]
async fn test_cache_delete_nonexistent() {
    let cache = CacheService::in_memory();

    let deleted = cache.del("nonexistent_key").await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_cache_concurrent_access() {
    let cache = CacheService::in_memory();

    let tasks: Vec<_> = (0..10)
        .map(|i| {
            let cache = cache.clone();
            tokio::spawn(async move {
                let key = format!("key_{}", i);
                let value = format!("value_{}", i);
                cache.set(&key, &value, None).await.unwrap();
                let retrieved = cache.get(&key).await.unwrap();
                assert_eq!(retrieved, Some(value));
            })
        })
        .collect();

    for task in tasks {
        task.await.expect("Task panicked");
    }
}

#[tokio::test]
async fn test_cache_large_value() {
    let cache = CacheService::in_memory();

    let large_users: Vec<CachedUser> = (0..1000)
        .map(|i| CachedUser {
            id: i,
            username: format!("user_{}", i),
            email: format!("user_{}@example.com", i),
        })
        .collect();

    cache
        .set_json("large_list", &large_users, None)
        .await
        .expect("Failed to set large value");

    let retrieved: Option<Vec<CachedUser>> = cache.get_json("large_list").await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.len(), 1000);
    assert_eq!(retrieved[0].id, 0);
    assert_eq!(retrieved[999].id, 999);
}
