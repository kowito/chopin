# File Uploads

## Overview

Chopin supports file uploads with two storage backends:

- **Local filesystem** (default) — files stored on disk
- **S3-compatible** — AWS S3, Cloudflare R2, MinIO, etc. Requires the `s3` feature.

## Configuration

### Local Storage

```env
UPLOAD_DIR=./uploads
MAX_UPLOAD_SIZE=10485760   # 10 MB
```

### S3 Storage

```toml
# Cargo.toml
[dependencies]
chopin-core = { version = "0.1", features = ["s3"] }
```

```env
S3_BUCKET=my-bucket
S3_REGION=us-east-1
S3_ACCESS_KEY_ID=AKIA...
S3_SECRET_ACCESS_KEY=secret...

# Optional
S3_ENDPOINT=https://account.r2.cloudflarestorage.com   # For R2/MinIO
S3_PUBLIC_URL=https://cdn.example.com                    # Public base URL
S3_PREFIX=uploads/                                        # Key prefix
```

## Using FileUploadService

### Upload a File

```rust
use axum::extract::Multipart;
use chopin_core::storage::FileUploadService;

async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiResponse<FileInfo>, ChopinError> {
    let upload_service = FileUploadService::new(&state.config);

    while let Some(field) = multipart.next_field().await? {
        let filename = field.file_name().unwrap_or("unnamed").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let data = field.bytes().await?;

        let result = upload_service.upload(&filename, &data, &content_type).await?;
        return Ok(ApiResponse::success(FileInfo {
            path: result.path,
            url: result.url,
            size: data.len(),
        }));
    }

    Err(ChopinError::BadRequest("No file provided".into()))
}
```

### Delete a File

```rust
upload_service.delete(&file_path).await?;
```

### Get File URL

```rust
let url = upload_service.url(&file_path);
```

## File Validation

Chopin validates uploads automatically:

- **File size** — Rejects files exceeding `MAX_UPLOAD_SIZE`
- **Content type** — You can restrict allowed MIME types

```rust
use chopin_core::storage::validate_upload;

// Validate file size and type
validate_upload(
    &data,
    state.config.max_upload_size,
    &["image/jpeg", "image/png", "image/webp"],
)?;
```

## Storage Backends

### LocalStorage

Files are saved to the `UPLOAD_DIR` directory with UUID-based filenames to prevent collision:

```
uploads/
  a1b2c3d4-e5f6-7890-abcd-ef1234567890.jpg
  b2c3d4e5-f6a7-8901-bcde-f12345678901.png
```

### S3Storage

Files are uploaded to the configured S3 bucket with the optional prefix:

```
s3://my-bucket/uploads/a1b2c3d4.jpg
```

The public URL is constructed from `S3_PUBLIC_URL` if set, otherwise the S3 object URL.

## Serving Local Files

To serve uploaded files, add a static file route:

```rust
use tower_http::services::ServeDir;

let router = Router::new()
    .nest_service("/uploads", ServeDir::new(&config.upload_dir));
```
