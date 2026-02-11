use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
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
}

impl Config {
    /// Load configuration from environment variables (with .env support).
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // Load .env file if present (ignore errors if missing)
        let _ = dotenvy::dotenv();

        Ok(Config {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://chopin.db?mode=rwc".to_string()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "chopin-dev-secret-change-me".to_string()),
            jwt_expiry_hours: std::env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            server_host: std::env::var("SERVER_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            environment: std::env::var("ENVIRONMENT")
                .unwrap_or_else(|_| "development".to_string()),
        })
    }

    /// Check if running in development mode.
    pub fn is_dev(&self) -> bool {
        self.environment == "development"
    }

    /// Get the full server address.
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
