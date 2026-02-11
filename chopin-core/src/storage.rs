use axum::extract::Multipart;
use serde::Serialize;
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

use crate::error::ChopinError;

/// Metadata about an uploaded file.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadedFile {
    /// Original filename from the upload
    pub filename: String,
    /// Stored filename (UUID-based to avoid collisions)
    pub stored_name: String,
    /// MIME content type
    pub content_type: String,
    /// File size in bytes
    pub size: u64,
    /// Relative path from upload directory
    pub path: String,
}

/// Storage backend trait for pluggable file storage.
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store file bytes and return the stored path.
    async fn store(
        &self,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<UploadedFile, ChopinError>;

    /// Delete a file by its stored name.
    async fn delete(&self, stored_name: &str) -> Result<(), ChopinError>;

    /// Check if a file exists.
    async fn exists(&self, stored_name: &str) -> Result<bool, ChopinError>;

    /// Get the public URL or path for a file.
    async fn url(&self, stored_name: &str) -> Result<String, ChopinError>;
}

/// Local filesystem storage backend.
///
/// Files are stored in the configured upload directory with UUID-based names.
///
/// ```rust,ignore
/// let storage = LocalStorage::new("./uploads");
/// let file = storage.store("photo.jpg", "image/jpeg", &bytes).await?;
/// println!("Stored at: {}", file.path);
/// ```
#[derive(Clone)]
pub struct LocalStorage {
    pub upload_dir: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage backend.
    pub fn new(upload_dir: impl Into<PathBuf>) -> Self {
        LocalStorage {
            upload_dir: upload_dir.into(),
        }
    }

    /// Ensure the upload directory exists.
    pub async fn ensure_dir(&self) -> Result<(), ChopinError> {
        tokio::fs::create_dir_all(&self.upload_dir)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to create upload dir: {}", e)))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl StorageBackend for LocalStorage {
    async fn store(
        &self,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<UploadedFile, ChopinError> {
        self.ensure_dir().await?;

        // Generate a unique stored name
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let stored_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let file_path = self.upload_dir.join(&stored_name);

        tokio::fs::write(&file_path, data)
            .await
            .map_err(|e| ChopinError::Internal(format!("Failed to write file: {}", e)))?;

        Ok(UploadedFile {
            filename: filename.to_string(),
            stored_name: stored_name.clone(),
            content_type: content_type.to_string(),
            size: data.len() as u64,
            path: format!("uploads/{}", stored_name),
        })
    }

    async fn delete(&self, stored_name: &str) -> Result<(), ChopinError> {
        let file_path = self.upload_dir.join(stored_name);
        if file_path.exists() {
            tokio::fs::remove_file(&file_path)
                .await
                .map_err(|e| ChopinError::Internal(format!("Failed to delete file: {}", e)))?;
        }
        Ok(())
    }

    async fn exists(&self, stored_name: &str) -> Result<bool, ChopinError> {
        let file_path = self.upload_dir.join(stored_name);
        Ok(file_path.exists())
    }

    async fn url(&self, stored_name: &str) -> Result<String, ChopinError> {
        Ok(format!("/uploads/{}", stored_name))
    }
}

/// File upload service for handlers.
///
/// ```rust,ignore
/// use axum::extract::Multipart;
/// use chopin_core::storage::FileUploadService;
///
/// async fn upload_handler(
///     State(state): State<AppState>,
///     multipart: Multipart,
/// ) -> Result<ApiResponse<Vec<UploadedFile>>, ChopinError> {
///     let storage = LocalStorage::new(&state.config.upload_dir);
///     let files = FileUploadService::process_upload(multipart, &storage, 10_485_760).await?;
///     Ok(ApiResponse::success(files))
/// }
/// ```
pub struct FileUploadService;

impl FileUploadService {
    /// Process a multipart upload and store files.
    ///
    /// Returns a list of uploaded file metadata.
    pub async fn process_upload(
        mut multipart: Multipart,
        storage: &dyn StorageBackend,
        max_size: u64,
    ) -> Result<Vec<UploadedFile>, ChopinError> {
        let mut files = Vec::new();

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| ChopinError::BadRequest(format!("Multipart error: {}", e)))?
        {
            let filename = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string());

            let content_type = field
                .content_type()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    mime_guess::from_path(&filename)
                        .first_or_octet_stream()
                        .to_string()
                });

            let data = field
                .bytes()
                .await
                .map_err(|e| ChopinError::BadRequest(format!("Failed to read field: {}", e)))?;

            if data.len() as u64 > max_size {
                return Err(ChopinError::BadRequest(format!(
                    "File '{}' exceeds maximum size of {} bytes",
                    filename, max_size
                )));
            }

            let uploaded = storage.store(&filename, &content_type, &data).await?;
            files.push(uploaded);
        }

        if files.is_empty() {
            return Err(ChopinError::BadRequest("No files uploaded".to_string()));
        }

        Ok(files)
    }

    /// Process a single file upload.
    pub async fn process_single_upload(
        mut multipart: Multipart,
        storage: &dyn StorageBackend,
        max_size: u64,
    ) -> Result<UploadedFile, ChopinError> {
        let field = multipart
            .next_field()
            .await
            .map_err(|e| ChopinError::BadRequest(format!("Multipart error: {}", e)))?
            .ok_or_else(|| ChopinError::BadRequest("No file provided".to_string()))?;

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                mime_guess::from_path(&filename)
                    .first_or_octet_stream()
                    .to_string()
            });

        let data = field
            .bytes()
            .await
            .map_err(|e| ChopinError::BadRequest(format!("Failed to read field: {}", e)))?;

        if data.len() as u64 > max_size {
            return Err(ChopinError::BadRequest(format!(
                "File exceeds maximum size of {} bytes",
                max_size
            )));
        }

        storage.store(&filename, &content_type, &data).await
    }
}

/// Helper to validate allowed file extensions.
pub fn validate_extension(filename: &str, allowed: &[&str]) -> Result<(), ChopinError> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !allowed.iter().any(|a| a.to_lowercase() == ext) {
        return Err(ChopinError::BadRequest(format!(
            "File type '.{}' not allowed. Allowed: {:?}",
            ext, allowed
        )));
    }
    Ok(())
}

/// Helper to validate allowed MIME types.
pub fn validate_content_type(content_type: &str, allowed: &[&str]) -> Result<(), ChopinError> {
    if !allowed.iter().any(|a| content_type.starts_with(a)) {
        return Err(ChopinError::BadRequest(format!(
            "Content type '{}' not allowed. Allowed: {:?}",
            content_type, allowed
        )));
    }
    Ok(())
}
