use chopin::storage::{validate_content_type, validate_extension, LocalStorage, StorageBackend};
use std::path::PathBuf;

// ═══ validate_extension ═══

#[test]
fn test_validate_extension_allowed() {
    let result = validate_extension("photo.jpg", &["jpg", "png", "gif"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_extension_case_insensitive() {
    let result = validate_extension("photo.JPG", &["jpg", "png"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_extension_rejected() {
    let result = validate_extension("script.exe", &["jpg", "png", "gif"]);
    assert!(result.is_err());
}

#[test]
fn test_validate_extension_no_extension() {
    let result = validate_extension("README", &["txt", "md"]);
    assert!(result.is_err());
}

#[test]
fn test_validate_extension_empty_allowed() {
    let result = validate_extension("photo.jpg", &[]);
    assert!(result.is_err());
}

#[test]
fn test_validate_extension_dot_in_name() {
    let result = validate_extension("my.photo.png", &["png"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_extension_allowed_mixed_case_list() {
    let result = validate_extension("doc.PDF", &["pdf", "doc"]);
    assert!(result.is_ok());
}

// ═══ validate_content_type ═══

#[test]
fn test_validate_content_type_allowed() {
    let result = validate_content_type("image/jpeg", &["image/"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_content_type_rejected() {
    let result = validate_content_type("application/pdf", &["image/"]);
    assert!(result.is_err());
}

#[test]
fn test_validate_content_type_exact_match() {
    let result = validate_content_type("image/png", &["image/png", "image/jpeg"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_content_type_prefix_match() {
    let result = validate_content_type("image/svg+xml", &["image/"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_content_type_empty_allowed() {
    let result = validate_content_type("image/png", &[]);
    assert!(result.is_err());
}

#[test]
fn test_validate_content_type_application() {
    let result = validate_content_type("application/json", &["application/json", "text/plain"]);
    assert!(result.is_ok());
}

#[test]
fn test_validate_content_type_text_not_in_list() {
    let result = validate_content_type("text/html", &["application/json"]);
    assert!(result.is_err());
}

// ═══ LocalStorage construction ═══

#[test]
fn test_local_storage_new() {
    let storage = LocalStorage::new("./uploads");
    assert_eq!(storage.upload_dir, PathBuf::from("./uploads"));
}

#[test]
fn test_local_storage_new_custom_path() {
    let storage = LocalStorage::new("/var/data/files");
    assert_eq!(storage.upload_dir, PathBuf::from("/var/data/files"));
}

#[test]
fn test_local_storage_clone() {
    let storage = LocalStorage::new("./uploads");
    let cloned = storage.clone();
    assert_eq!(storage.upload_dir, cloned.upload_dir);
}

// ═══ LocalStorage async operations ═══

#[tokio::test]
async fn test_local_storage_ensure_dir() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    // Directory should not exist yet
    assert!(!std::path::Path::new(&dir).exists());

    // ensure_dir should create it
    storage.ensure_dir().await.expect("ensure_dir failed");
    assert!(std::path::Path::new(&dir).exists());

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_ensure_dir_idempotent() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    // Call twice — should not error
    storage.ensure_dir().await.expect("first call");
    storage.ensure_dir().await.expect("second call");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_store_and_exists() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    let uploaded = storage
        .store("test.txt", "text/plain", b"hello world")
        .await
        .expect("store failed");

    assert_eq!(uploaded.filename, "test.txt");
    assert_eq!(uploaded.content_type, "text/plain");
    assert_eq!(uploaded.size, 11);
    assert!(uploaded.stored_name.ends_with(".txt"));
    assert!(uploaded.path.starts_with("uploads/"));

    // File should exist
    let exists = storage
        .exists(&uploaded.stored_name)
        .await
        .expect("exists failed");
    assert!(exists);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_store_no_extension() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    let uploaded = storage
        .store("Makefile", "application/octet-stream", b"all: build")
        .await
        .expect("store failed");

    // Should default to .bin extension
    assert!(uploaded.stored_name.ends_with(".bin"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_url() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    let uploaded = storage
        .store("photo.jpg", "image/jpeg", b"\xFF\xD8\xFF")
        .await
        .expect("store failed");

    let url = storage
        .url(&uploaded.stored_name)
        .await
        .expect("url failed");
    assert!(url.starts_with("/uploads/"));
    assert!(url.contains(&uploaded.stored_name));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_delete() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    let uploaded = storage
        .store("temp.txt", "text/plain", b"temporary")
        .await
        .expect("store failed");

    // File exists
    let exists = storage.exists(&uploaded.stored_name).await.expect("exists");
    assert!(exists);

    // Delete
    storage
        .delete(&uploaded.stored_name)
        .await
        .expect("delete failed");

    // File should no longer exist
    let exists = storage
        .exists(&uploaded.stored_name)
        .await
        .expect("exists after delete");
    assert!(!exists);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_delete_nonexistent() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);
    storage.ensure_dir().await.expect("ensure_dir");

    // Deleting a non-existent file should not error
    let result = storage.delete("nonexistent.txt").await;
    assert!(result.is_ok());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_exists_nonexistent() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);
    storage.ensure_dir().await.expect("ensure_dir");

    let exists = storage.exists("no-such-file.txt").await.expect("exists");
    assert!(!exists);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_local_storage_multiple_files() {
    let dir = format!("/tmp/chopin_test_{}", uuid::Uuid::new_v4());
    let storage = LocalStorage::new(&dir);

    let f1 = storage
        .store("a.txt", "text/plain", b"aaa")
        .await
        .expect("store a");
    let f2 = storage
        .store("b.txt", "text/plain", b"bbb")
        .await
        .expect("store b");
    let f3 = storage
        .store("c.txt", "text/plain", b"ccc")
        .await
        .expect("store c");

    // All should have unique stored names
    assert_ne!(f1.stored_name, f2.stored_name);
    assert_ne!(f2.stored_name, f3.stored_name);

    // All should exist
    assert!(storage.exists(&f1.stored_name).await.unwrap());
    assert!(storage.exists(&f2.stored_name).await.unwrap());
    assert!(storage.exists(&f3.stored_name).await.unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}
