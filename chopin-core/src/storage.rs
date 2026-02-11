use axum::extract::Multipart;
use serde::Serialize;
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

use crate::error::ChopinError;

#[cfg(feature = "s3")]
use crate::config::Config;

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

// ---------------------------------------------------------------------------
// S3-compatible object storage backend (AWS S3, Cloudflare R2, MinIO, etc.)
// ---------------------------------------------------------------------------
#[cfg(feature = "s3")]
mod s3_backend {
    use super::*;
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::config::{Credentials, Region};
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::Client;

    /// S3-compatible object storage backend.
    ///
    /// Works with AWS S3, Cloudflare R2, MinIO, DigitalOcean Spaces,
    /// Backblaze B2, and any S3-compatible service.
    ///
    /// ```rust,ignore
    /// // AWS S3
    /// let storage = S3Storage::from_config(&config).await?;
    ///
    /// // Cloudflare R2
    /// // S3_ENDPOINT=https://<account_id>.r2.cloudflarestorage.com
    /// // S3_BUCKET=my-bucket
    /// let storage = S3Storage::from_config(&config).await?;
    /// ```
    #[derive(Clone)]
    pub struct S3Storage {
        client: Client,
        bucket: String,
        prefix: String,
        public_url: Option<String>,
    }

    impl S3Storage {
        /// Create an S3 storage backend from application configuration.
        ///
        /// Reads `S3_BUCKET`, `S3_REGION`, `S3_ENDPOINT`, `S3_ACCESS_KEY_ID`,
        /// `S3_SECRET_ACCESS_KEY`, `S3_PUBLIC_URL`, and `S3_PREFIX` from config.
        pub async fn from_config(config: &Config) -> Result<Self, ChopinError> {
            let bucket = config
                .s3_bucket
                .as_deref()
                .ok_or_else(|| {
                    ChopinError::Internal("S3_BUCKET must be set to use S3 storage".to_string())
                })?
                .to_string();

            let region = config
                .s3_region
                .as_deref()
                .unwrap_or("us-east-1")
                .to_string();

            let mut s3_config_builder = aws_sdk_s3::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new(region.clone()))
                .force_path_style(true);

            // Custom endpoint for R2, MinIO, etc.
            if let Some(endpoint) = &config.s3_endpoint {
                s3_config_builder = s3_config_builder.endpoint_url(endpoint);
            }

            // Explicit credentials (or fall back to AWS credential chain)
            if let (Some(access_key), Some(secret_key)) =
                (&config.s3_access_key_id, &config.s3_secret_access_key)
            {
                let credentials = Credentials::new(
                    access_key,
                    secret_key,
                    None, // session token
                    None, // expiry
                    "chopin-env",
                );
                s3_config_builder = s3_config_builder.credentials_provider(credentials);
            } else {
                // Use default AWS credential chain (env vars, IAM roles, etc.)
                let shared_config = aws_config::defaults(BehaviorVersion::latest())
                    .region(Region::new(region))
                    .load()
                    .await;
                let creds_provider = shared_config
                    .credentials_provider()
                    .ok_or_else(|| {
                        ChopinError::Internal(
                            "No S3 credentials found. Set S3_ACCESS_KEY_ID and \
                             S3_SECRET_ACCESS_KEY, or configure AWS credentials."
                                .to_string(),
                        )
                    })?
                    .clone();
                s3_config_builder = s3_config_builder.credentials_provider(creds_provider);
            }

            let client = Client::from_conf(s3_config_builder.build());

            let prefix = config
                .s3_prefix
                .as_deref()
                .unwrap_or("uploads/")
                .to_string();

            Ok(S3Storage {
                client,
                bucket,
                prefix,
                public_url: config.s3_public_url.clone(),
            })
        }

        /// Create an S3 storage backend with explicit parameters.
        pub async fn new(
            bucket: impl Into<String>,
            region: impl Into<String>,
            endpoint: Option<String>,
            access_key: impl Into<String>,
            secret_key: impl Into<String>,
            public_url: Option<String>,
            prefix: Option<String>,
        ) -> Result<Self, ChopinError> {
            let region_str = region.into();
            let credentials = Credentials::new(
                access_key.into(),
                secret_key.into(),
                None,
                None,
                "chopin-explicit",
            );

            let mut s3_config_builder = aws_sdk_s3::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new(region_str))
                .credentials_provider(credentials)
                .force_path_style(true);

            if let Some(ep) = &endpoint {
                s3_config_builder = s3_config_builder.endpoint_url(ep);
            }

            let client = Client::from_conf(s3_config_builder.build());

            Ok(S3Storage {
                client,
                bucket: bucket.into(),
                prefix: prefix.unwrap_or_else(|| "uploads/".to_string()),
                public_url,
            })
        }

        /// Get the full S3 key (prefix + stored_name).
        fn object_key(&self, stored_name: &str) -> String {
            format!("{}{}", self.prefix, stored_name)
        }
    }

    #[async_trait::async_trait]
    impl StorageBackend for S3Storage {
        async fn store(
            &self,
            filename: &str,
            content_type: &str,
            data: &[u8],
        ) -> Result<UploadedFile, ChopinError> {
            let ext = Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin");
            let stored_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            let key = self.object_key(&stored_name);

            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(data.to_vec()))
                .content_type(content_type)
                .send()
                .await
                .map_err(|e| {
                    ChopinError::Internal(format!("S3 upload failed: {}", e))
                })?;

            let path = match &self.public_url {
                Some(base) => {
                    let base = base.trim_end_matches('/');
                    format!("{}/{}", base, key)
                }
                None => key.clone(),
            };

            Ok(UploadedFile {
                filename: filename.to_string(),
                stored_name,
                content_type: content_type.to_string(),
                size: data.len() as u64,
                path,
            })
        }

        async fn delete(&self, stored_name: &str) -> Result<(), ChopinError> {
            let key = self.object_key(stored_name);

            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| {
                    ChopinError::Internal(format!("S3 delete failed: {}", e))
                })?;

            Ok(())
        }

        async fn exists(&self, stored_name: &str) -> Result<bool, ChopinError> {
            let key = self.object_key(stored_name);

            match self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
            {
                Ok(_) => Ok(true),
                Err(e) => {
                    let service_err = e.into_service_error();
                    if service_err.is_not_found() {
                        Ok(false)
                    } else {
                        Err(ChopinError::Internal(format!(
                            "S3 head_object failed: {}",
                            service_err
                        )))
                    }
                }
            }
        }

        async fn url(&self, stored_name: &str) -> Result<String, ChopinError> {
            let key = self.object_key(stored_name);

            match &self.public_url {
                Some(base) => {
                    let base = base.trim_end_matches('/');
                    Ok(format!("{}/{}", base, key))
                }
                None => {
                    // Generate a presigned URL (valid for 1 hour)
                    let presigning = aws_sdk_s3::presigning::PresigningConfig::builder()
                        .expires_in(std::time::Duration::from_secs(3600))
                        .build()
                        .map_err(|e| {
                            ChopinError::Internal(format!("Presigning config error: {}", e))
                        })?;

                    let presigned = self
                        .client
                        .get_object()
                        .bucket(&self.bucket)
                        .key(&key)
                        .presigned(presigning)
                        .await
                        .map_err(|e| {
                            ChopinError::Internal(format!("S3 presign failed: {}", e))
                        })?;

                    Ok(presigned.uri().to_string())
                }
            }
        }
    }
}

#[cfg(feature = "s3")]
pub use s3_backend::S3Storage;

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
