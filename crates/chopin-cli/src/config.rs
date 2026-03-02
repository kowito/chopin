use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

/// Top-level Chopin.toml configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ChopinConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    #[allow(dead_code)]
    pub workers: usize, // 0 = auto-detect
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_url")]
    pub url: String,
    #[serde(default = "default_pool_size")]
    #[allow(dead_code)]
    pub pool_size: usize,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_db_url() -> String {
    "postgres://postgres:postgres@localhost:5432/postgres".to_string()
}
fn default_pool_size() -> usize {
    5
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            workers: 0,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_db_url(),
            pool_size: default_pool_size(),
        }
    }
}

impl ChopinConfig {
    /// Load config from `Chopin.toml` in the given directory.
    /// Falls back to defaults if the file doesn't exist.
    /// Environment variables override file values.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let config_path = project_dir.join("Chopin.toml");

        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            // Interpolate environment variables: ${VAR_NAME}
            let interpolated = interpolate_env_vars(&content);
            toml::from_str(&interpolated)?
        } else {
            ChopinConfig {
                server: ServerConfig::default(),
                database: DatabaseConfig::default(),
            }
        };

        // Environment variables always override
        if let Ok(url) = std::env::var("DATABASE_URL") {
            config.database.url = url;
        }
        if let Ok(port) = std::env::var("PORT")
            && let Ok(p) = port.parse()
        {
            config.server.port = p;
        }
        if let Ok(host) = std::env::var("HOST") {
            config.server.host = host;
        }

        Ok(config)
    }
}

/// Replace `${VAR_NAME}` patterns with environment variable values.
fn interpolate_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    // Simple regex-free interpolation
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ChopinConfig {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
        };
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.database.pool_size, 5);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[server]
host = "127.0.0.1"
port = 3000

[database]
url = "postgres://user:pass@db:5432/mydb"
pool_size = 10
"#;
        let config: ChopinConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.database.pool_size, 10);
    }

    #[test]
    fn test_interpolate_env_vars() {
        unsafe { std::env::set_var("TEST_CHOPIN_VAR", "hello") };
        let result = interpolate_env_vars("value = \"${TEST_CHOPIN_VAR}\"");
        assert_eq!(result, "value = \"hello\"");
        unsafe { std::env::remove_var("TEST_CHOPIN_VAR") };
    }

    #[test]
    fn test_interpolate_missing_var_becomes_empty() {
        unsafe { std::env::remove_var("CHOPIN_DEFINITELY_NOT_SET_XYZ") };
        let result = interpolate_env_vars("x=${CHOPIN_DEFINITELY_NOT_SET_XYZ}");
        assert_eq!(result, "x=");
    }

    #[test]
    fn test_interpolate_multiple_vars() {
        unsafe {
            std::env::set_var("CHOPIN_TEST_HOST", "myhost");
            std::env::set_var("CHOPIN_TEST_PORT", "9999");
        }
        let tpl = "${CHOPIN_TEST_HOST}:${CHOPIN_TEST_PORT}";
        let result = interpolate_env_vars(tpl);
        assert_eq!(result, "myhost:9999");
        unsafe {
            std::env::remove_var("CHOPIN_TEST_HOST");
            std::env::remove_var("CHOPIN_TEST_PORT");
        }
    }

    #[test]
    fn test_server_config_default_fields() {
        let s = ServerConfig::default();
        assert_eq!(s.host, "0.0.0.0");
        assert_eq!(s.port, 8080);
        assert_eq!(s.workers, 0);
    }

    #[test]
    fn test_database_config_default_fields() {
        let d = DatabaseConfig::default();
        assert!(d.url.contains("5432"), "default url should include PG port 5432");
        assert_eq!(d.pool_size, 5);
    }

    #[test]
    fn test_load_defaults_when_no_toml_file() {
        let dir = tempfile::tempdir().unwrap();
        // No Chopin.toml in this directory
        let config = ChopinConfig::load(dir.path()).unwrap();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
    }

    #[test]
    fn test_load_from_toml_file() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = "[server]\nport = 4000\n[database]\npool_size = 20\n";
        std::fs::write(dir.path().join("Chopin.toml"), toml_content).unwrap();
        let config = ChopinConfig::load(dir.path()).unwrap();
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.database.pool_size, 20);
    }

    #[test]
    fn test_env_var_overrides_port() {
        unsafe { std::env::set_var("PORT", "7777") };
        let dir = tempfile::tempdir().unwrap();
        let config = ChopinConfig::load(dir.path()).unwrap();
        assert_eq!(config.server.port, 7777);
        unsafe { std::env::remove_var("PORT") };
    }
}
