# File Uploads Guide

Chopin provides built-in file upload handling with support for multipart forms, local storage, and future S3 integration.

## Quick Start

```rust
use axum::extract::Multipart;
use chopin_core::{storage::{FileUploadService, LocalStorage}, ApiResponse, ChopinError};

async fn upload_file(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<ApiResponse<Vec<UploadedFile>>, ChopinError> {
    let storage = LocalStorage::new(&state.config.upload_dir);
    let files = FileUploadService::process_upload(
        multipart,
        &storage,
        state.config.max_upload_size,
    ).await?;
    
    Ok(ApiResponse::success(files))
}
```

## Configuration

Configure upload settings in `.env`:

```env
# Upload directory (relative or absolute path)
UPLOAD_DIR=./uploads

# Maximum file size in bytes (10MB default)
MAX_UPLOAD_SIZE=10485760
```

| Size | Bytes |
|------|-------|
| 1 MB | 1,048,576 |
| 10 MB | 10,485,760 |
| 50 MB | 52,428,800 |
| 100 MB | 104,857,600 |

## Single File Upload

```rust
use axum::routing::post;

#[utoipa::path(
    post,
    path = "/api/upload",
    request_body(content = inline(Object), content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "File uploaded", body = ApiResponse<UploadedFile>),
    ),
    tag = "uploads"
)]
async fn upload_single(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<ApiResponse<UploadedFile>, ChopinError> {
    let storage = LocalStorage::new(&state.config.upload_dir);
    let file = FileUploadService::process_single_upload(
        multipart,
        &storage,
        state.config.max_upload_size,
    ).await?;
    
    Ok(ApiResponse::success(file))
}

// Add to router
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_single))
}
```

## Multiple File Upload

```rust
async fn upload_multiple(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<ApiResponse<Vec<UploadedFile>>, ChopinError> {
    let storage = LocalStorage::new(&state.config.upload_dir);
    let files = FileUploadService::process_upload(
        multipart,
        &storage,
        state.config.max_upload_size,
    ).await?;
    
    Ok(ApiResponse::success(files))
}
```

## Upload Response

The `UploadedFile` struct contains metadata:

```rust
#[derive(Serialize)]
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
```

Example response:

```json
{
  "success": true,
  "data": {
    "filename": "photo.jpg",
    "stored_name": "a1b2c3d4-e5f6-7890-abcd-ef1234567890.jpg",
    "content_type": "image/jpeg",
    "size": 245680,
    "path": "uploads/a1b2c3d4-e5f6-7890-abcd-ef1234567890.jpg"
  }
}
```

## File Validation

### By Extension

```rust
use chopin_core::storage::validate_extension;

async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiResponse<UploadedFile>, ChopinError> {
    let field = multipart.next_field().await
        .map_err(|e| ChopinError::BadRequest(format!("Multipart error: {}", e)))?
        .ok_or_else(|| ChopinError::BadRequest("No file provided".into()))?;
    
    let filename = field.file_name()
        .ok_or_else(|| ChopinError::BadRequest("No filename".into()))?;
    
    // Validate extension
    validate_extension(filename, &["jpg", "jpeg", "png", "gif", "webp"])?;
    
    // Continue with upload...
}
```

### By Content Type

```rust
use chopin_core::storage::validate_content_type;

// Validate MIME type
validate_content_type(&content_type, &["image/"])?;
```

### By Size

```rust
let max_size = 5_242_880; // 5 MB

if data.len() as u64 > max_size {
    return Err(ChopinError::BadRequest(
        format!("File exceeds maximum size of {} bytes", max_size)
    ));
}
```

## Custom Upload Handler

Full control over the upload process:

```rust
async fn custom_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiResponse<UploadedFile>, ChopinError> {
    let storage = LocalStorage::new(&state.config.upload_dir);
    
    // Get form field
    let field = multipart.next_field().await
        .map_err(|e| ChopinError::BadRequest(format!("Multipart error: {}", e)))?
        .ok_or_else(|| ChopinError::BadRequest("No file provided".into()))?;
    
    // Extract metadata
    let filename = field.file_name()
        .unwrap_or("unnamed")
        .to_string();
    
    let content_type = field.content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    
    // Validate
    validate_extension(&filename, &["pdf", "doc", "docx"])?;
    validate_content_type(&content_type, &["application/"])?;
    
    // Read bytes
    let data = field.bytes().await
        .map_err(|e| ChopinError::BadRequest(format!("Failed to read file: {}", e)))?;
    
    // Check size
    if data.len() as u64 > state.config.max_upload_size {
        return Err(ChopinError::BadRequest("File too large".into()));
    }
    
    // Store file
    let uploaded = storage.store(&filename, &content_type, &data).await?;
    
    Ok(ApiResponse::success(uploaded))
}
```

## Serving Uploaded Files

Create a route to serve uploaded files:

```rust
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::fs;

async fn serve_file(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response, ChopinError> {
    let file_path = format!("{}/{}", state.config.upload_dir, filename);
    
    // Check if file exists
    if !tokio::fs::metadata(&file_path).await.is_ok() {
        return Err(ChopinError::NotFound("File not found".into()));
    }
    
    // Read file
    let contents = fs::read(&file_path).await
        .map_err(|_| ChopinError::Internal("Failed to read file".into()))?;
    
    // Determine content type
    let content_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .to_string();
    
    // Return response
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        contents,
    ).into_response())
}

// Add route
Router::new()
    .route("/uploads/:filename", get(serve_file))
```

## Image Processing Example

Integrate with `image` crate for thumbnails:

```toml
[dependencies]
image = "0.25"
```

```rust
use image::ImageFormat;

async fn upload_with_thumbnail(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<ApiResponse<UploadResponse>, ChopinError> {
    let storage = LocalStorage::new(&state.config.upload_dir);
    let uploaded = FileUploadService::process_single_upload(
        multipart,
        &storage,
        state.config.max_upload_size,
    ).await?;
    
    // Create thumbnail
    let img = image::open(format!("{}/{}", state.config.upload_dir, uploaded.stored_name))
        .map_err(|e| ChopinError::Internal(format!("Image processing error: {}", e)))?;
    
    let thumbnail = img.thumbnail(200, 200);
    let thumb_name = format!("thumb_{}", uploaded.stored_name);
    let thumb_path = format!("{}/{}", state.config.upload_dir, thumb_name);
    
    thumbnail.save(&thumb_path)
        .map_err(|e| ChopinError::Internal(format!("Failed to save thumbnail: {}", e)))?;
    
    Ok(ApiResponse::success(UploadResponse {
        original: uploaded,
        thumbnail_path: format!("uploads/{}", thumb_name),
    }))
}
```

## Storage Backends

### Local Storage (Default)

Stores files in the local filesystem:

```rust
use chopin_core::storage::LocalStorage;

let storage = LocalStorage::new("./uploads");
storage.ensure_dir().await?;
```

**Pros:**
- Simple, no dependencies
- Fast local access
- Good for development and testing

**Cons:**
- Not scalable across servers
- Requires disk space management
- No built-in CDN

### Custom Storage Backend

Implement the `StorageBackend` trait for S3, GCS, etc.:

```rust
use async_trait::async_trait;
use chopin_core::storage::{StorageBackend, UploadedFile};

pub struct S3Storage {
    bucket: String,
    region: String,
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn store(
        &self,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<UploadedFile, ChopinError> {
        // S3 upload implementation
        todo!()
    }
    
    async fn delete(&self, stored_name: &str) -> Result<(), ChopinError> {
        // S3 delete implementation
        todo!()
    }
    
    async fn exists(&self, stored_name: &str) -> Result<bool, ChopinError> {
        // S3 exists check
        todo!()
    }
    
    async fn url(&self, stored_name: &str) -> Result<String, ChopinError> {
        Ok(format!("https://{}.s3.amazonaws.com/{}", self.bucket, stored_name))
    }
}
```

## Security Best Practices

### 1. Validate File Types

Always validate both extension and content type:

```rust
validate_extension(&filename, &ALLOWED_EXTENSIONS)?;
validate_content_type(&content_type, &ALLOWED_TYPES)?;
```

### 2. Limit File Sizes

Enforce strict size limits:

```rust
const MAX_IMAGE_SIZE: u64 = 5_242_880; // 5 MB
const MAX_VIDEO_SIZE: u64 = 104_857_600; // 100 MB
```

### 3. Use Random Filenames

Never use user-provided filenames directly:

```rust
// ✓ Good - UUID-based (default in Chopin)
let stored_name = format!("{}.{}", Uuid::new_v4(), ext);

// ✗ Bad - user-controlled
let stored_name = field.file_name().unwrap();
```

### 4. Scan for Malware

Consider integrating virus scanning for user uploads:

```rust
async fn scan_file(path: &str) -> Result<bool, ChopinError> {
    // ClamAV or similar
    todo!()
}
```

### 5. Set Proper Permissions

Ensure upload directory has correct permissions:

```bash
chmod 755 uploads/
chown www-data:www-data uploads/
```

### 6. Rate Limiting

Limit uploads per user/IP:

```rust
// Use tower-http rate limiting
use tower_http::limit::RequestBodyLimitLayer;

Router::new()
    .route("/upload", post(upload_file))
    .layer(RequestBodyLimitLayer::new(10_485_760)) // 10 MB
```

## Production Deployment

### Serve Static Files via Nginx

```nginx
location /uploads/ {
    alias /var/www/app/uploads/;
    expires 1y;
    add_header Cache-Control "public, immutable";
}
```

### Use CDN for Uploaded Files

Configure CloudFront, Cloudflare, or similar to serve from your upload directory.

### Backup Strategy

Regularly backup uploaded files:

```bash
# Daily backup
rsync -avz /var/www/app/uploads/ /backups/uploads/$(date +%Y%m%d)/
```

## Testing File Uploads

```rust
#[tokio::test]
async fn test_file_upload() {
    let app = TestApp::new().await;
    
    let multipart_body = /* create multipart body */;
    
    let res = app.client
        .post(&app.url("/api/upload"))
        .header("Content-Type", "multipart/form-data")
        .body(multipart_body)
        .send()
        .await;
    
    assert_eq!(res.status, 200);
}
```
