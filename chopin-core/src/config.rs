use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Enable SO_REUSEPORT multi-core accept loops for maximum throughput.
    ///
    /// When `true`, each CPU core gets its own TCP listener and
    /// single-threaded tokio runtime — the same architecture used by
    /// top TechEmpower Rust entries.
    ///
    /// When `false` (default), a single multi-thread tokio runtime
    /// handles all connections.
    ///
    /// FastRoute endpoints work in both modes — they always bypass
    /// Axum with zero allocation.
    pub reuseport: bool,

    /// Database connection URL (e.g. sqlite://chopin.db, postgres://...)
    pub database_url: String,

    /// JWT signing secret
    pub jwt_secret: String,

    /// JWT token expiry in hours (default: 24)
    pub jwt_expiry_hours: u64,

    /// Server host (default: 127.0.0.1)
    pub server_host: String,

    /// Server port (default: 3000)
    pub server_port: u16,

    /// Environment: development, production, test
    pub environment: String,

    /// Redis URL for caching (optional, e.g. redis://127.0.0.1:6379)
    pub redis_url: Option<String>,

    /// Upload directory for file storage (default: ./uploads)
    pub upload_dir: String,

    /// Max upload file size in bytes (default: 10MB)
    pub max_upload_size: u64,

    /// S3 bucket name (optional, enables S3 storage)
    pub s3_bucket: Option<String>,

    /// S3 region (default: us-east-1)
    pub s3_region: Option<String>,

    /// S3-compatible endpoint URL (for Cloudflare R2, MinIO, etc.)
    pub s3_endpoint: Option<String>,

    /// S3 access key ID (optional, falls back to AWS credential chain)
    pub s3_access_key_id: Option<String>,

    /// S3 secret access key (optional, falls back to AWS credential chain)
    pub s3_secret_access_key: Option<String>,

    /// Public base URL for S3 objects (e.g. CDN URL or R2 public domain)
    pub s3_public_url: Option<String>,

    /// S3 key prefix / folder (default: "uploads/")
    pub s3_prefix: Option<String>,
}

impl Config {
    /// Load configuration from environment variables (with .env support).
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // Load .env file if present (ignore errors if missing)
        let _ = dotenvy::dotenv();

        Ok(Config {
            reuseport: matches!(
                std::env::var("REUSEPORT")
                    .unwrap_or_default()
                    .to_lowercase()
                    .as_str(),
                "true" | "1" | "yes"
            ),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://chopin.db?mode=rwc".to_string()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "chopin-dev-secret-change-me".to_string()),
            jwt_expiry_hours: std::env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            redis_url: std::env::var("REDIS_URL").ok(),
            upload_dir: std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()),
            max_upload_size: std::env::var("MAX_UPLOAD_SIZE")
                .unwrap_or_else(|_| "10485760".to_string()) // 10MB
                .parse()
                .unwrap_or(10_485_760),
            s3_bucket: std::env::var("S3_BUCKET").ok(),
            s3_region: std::env::var("S3_REGION").ok(),
            s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
            s3_access_key_id: std::env::var("S3_ACCESS_KEY_ID").ok(),
            s3_secret_access_key: std::env::var("S3_SECRET_ACCESS_KEY").ok(),
            s3_public_url: std::env::var("S3_PUBLIC_URL").ok(),
            s3_prefix: std::env::var("S3_PREFIX").ok(),
        })
    }

    /// Check if running in development mode.
    pub fn is_dev(&self) -> bool {
        self.environment == "development"
    }

    /// Check if S3 storage is configured.
    pub fn has_s3(&self) -> bool {
        self.s3_bucket.is_some()
    }

    /// Get the full server address.
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
