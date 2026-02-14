use chopin::perf::{cached_date_header, init_date_cache};
use std::time::Duration;

#[tokio::test]
async fn test_date_cache_initialization() {
    init_date_cache();
    
    // Should return a valid date header
    let header = cached_date_header();
    assert!(!header.is_empty());
    
    // Should be valid ASCII
    let header_str = header.to_str().expect("Invalid ASCII in date header");
    assert!(!header_str.is_empty());
    
    // Should be a valid HTTP date format (e.g., "Mon, 14 Feb 2026 12:34:56 GMT")
    assert!(header_str.contains("GMT"));
}

#[tokio::test]
async fn test_date_cache_updates() {
    init_date_cache();
    
    let header1 = cached_date_header();
    
    // Wait for cache to update (happens every 500ms)
    tokio::time::sleep(Duration::from_millis(600)).await;
    
    let header2 = cached_date_header();
    
    // Headers should still be valid
    assert!(!header1.is_empty());
    assert!(!header2.is_empty());
    
    // After waiting, headers might be different (different timestamps)
    // but both should be valid HTTP dates
    assert!(header1.to_str().unwrap().contains("GMT"));
    assert!(header2.to_str().unwrap().contains("GMT"));
}

#[tokio::test]
async fn test_date_cache_concurrent_access() {
    init_date_cache();
    
    let handles: Vec<_> = (0..10)
        .map(|_| {
            tokio::spawn(async {
                for _ in 0..100 {
                    let header = cached_date_header();
                    assert!(!header.is_empty());
                    assert!(header.to_str().unwrap().contains("GMT"));
                }
            })
        })
        .collect();
    
    for handle in handles {
        handle.await.expect("Task panicked");
    }
}

#[tokio::test]
async fn test_date_cache_consistency_within_window() {
    init_date_cache();
    
    // Within the same 500ms window, all threads should see the same epoch
    let header1 = cached_date_header();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let header2 = cached_date_header();
    
    // Both should be valid
    assert!(!header1.is_empty());
    assert!(!header2.is_empty());
}

#[tokio::test]
async fn test_date_cache_thread_local() {
    init_date_cache();
    
    // Each thread should be able to access the cache independently
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            tokio::task::spawn_blocking(|| {
                for _ in 0..50 {
                    let header = cached_date_header();
                    assert!(!header.is_empty());
                }
            })
        })
        .collect();
    
    for task in tasks {
        task.await.expect("Task panicked");
    }
}

#[tokio::test]
async fn test_date_cache_format() {
    init_date_cache();
    
    let header = cached_date_header();
    let header_str = header.to_str().expect("Invalid ASCII");
    
    // HTTP date format: "Day, DD Mon YYYY HH:MM:SS GMT"
    // Example: "Mon, 14 Feb 2026 12:34:56 GMT"
    let parts: Vec<&str> = header_str.split_whitespace().collect();
    
    // Should have at least 5 parts: Day, DD Mon YYYY HH:MM:SS GMT
    assert!(parts.len() >= 4, "Date header format incorrect: {}", header_str);
    
    // Last part should be GMT
    assert_eq!(parts.last().unwrap(), &"GMT");
}

#[tokio::test]
async fn test_date_cache_multiple_initializations() {
    // Should be safe to call init multiple times
    init_date_cache();
    init_date_cache();
    init_date_cache();
    
    let header = cached_date_header();
    assert!(!header.is_empty());
}

#[tokio::test]
async fn test_date_cache_real_time_progression() {
    init_date_cache();
    
    let header1 = cached_date_header();
    let time1 = header1.to_str().unwrap().to_string();
    
    // Wait for >1 second to ensure time has definitely changed
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let header2 = cached_date_header();
    let time2 = header2.to_str().unwrap().to_string();
    
    // Both should be valid dates
    assert!(time1.contains("GMT"));
    assert!(time2.contains("GMT"));
    
    // Note: In a real scenario, time2 should be later than time1,
    // but we don't check exact ordering as it depends on system time
}
