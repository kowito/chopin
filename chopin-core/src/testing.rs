use axum::http::HeaderMap;
use sea_orm::DatabaseConnection;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::config::{Config, SecurityConfig};

/// A test application builder for integration testing.
///
/// Spins up a Chopin server with an in-memory SQLite database.
///
/// ```rust,ignore
/// #[tokio::test]
/// async fn test_signup() {
///     let app = TestApp::new().await;
///     let res = app.post("/api/auth/signup", r#"{"email":"a@b.com","username":"bob","password":"secret123"}"#).await;
///     assert_eq!(res.status, 201);
/// }
/// ```
pub struct TestApp {
    pub addr: SocketAddr,
    pub client: TestClient,
    pub db: DatabaseConnection,
    pub config: Config,
}

impl TestApp {
    /// Create a new test app with an in-memory SQLite database.
    /// All security features are **disabled** by default for simpler testing.
    /// Use `new_secure()` to test with security features enabled.
    pub async fn new() -> Self {
        let config = Config {
            reuseport: false,
            database_url: "sqlite::memory:".to_string(),
            jwt_secret: "test-secret-key-for-testing".to_string(),
            jwt_expiry_hours: 24,
            server_host: "127.0.0.1".to_string(),
            server_port: 0, // OS assigns a random port
            environment: "test".to_string(),
            redis_url: None,
            upload_dir: "/tmp/chopin-test-uploads".to_string(),
            max_upload_size: 10_485_760,
            s3_bucket: None,
            s3_region: None,
            s3_endpoint: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_public_url: None,
            s3_prefix: None,
            security: SecurityConfig {
                enable_2fa: false,
                enable_rate_limit: false,
                rate_limit_max_attempts: 5,
                rate_limit_window_secs: 300,
                enable_account_lockout: false,
                lockout_max_attempts: 5,
                lockout_duration_secs: 900,
                enable_refresh_tokens: false,
                refresh_token_expiry_days: 30,
                enable_session_management: false,
                enable_password_reset: false,
                password_reset_expiry_secs: 3600,
                enable_email_verification: false,
                email_verification_expiry_secs: 86400,
                enable_csrf: false,
                enable_device_tracking: false,
                min_password_length: 8,
            },
        };

        Self::with_config(config).await
    }

    /// Create a new test app with all security features enabled.
    pub async fn new_secure() -> Self {
        let config = Config {
            reuseport: false,
            database_url: "sqlite::memory:".to_string(),
            jwt_secret: "test-secret-key-for-testing".to_string(),
            jwt_expiry_hours: 24,
            server_host: "127.0.0.1".to_string(),
            server_port: 0,
            environment: "test".to_string(),
            redis_url: None,
            upload_dir: "/tmp/chopin-test-uploads".to_string(),
            max_upload_size: 10_485_760,
            s3_bucket: None,
            s3_region: None,
            s3_endpoint: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_public_url: None,
            s3_prefix: None,
            security: SecurityConfig::default(),
        };

        Self::with_config(config).await
    }

    /// Create a new test app with a custom config.
    pub async fn with_config(config: Config) -> Self {
        let app = crate::App::with_config(config.clone())
            .await
            .expect("Failed to create test app");

        // Run module migrations (including auth tables)
        app.run_migrations()
            .await
            .expect("Failed to run module migrations");

        let router = app.router();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test server");
        let addr = listener.local_addr().expect("Failed to get local addr");

        // Spawn the server in the background
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let client = TestClient::new(addr);

        TestApp {
            addr,
            client,
            db: app.db,
            config: app.config,
        }
    }

    /// Get the base URL for the test server.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// Create a user and return the auth token.
    pub async fn create_user(
        &self,
        email: &str,
        username: &str,
        password: &str,
    ) -> (String, serde_json::Value) {
        let body = serde_json::json!({
            "email": email,
            "username": username,
            "password": password,
        });

        let res = self
            .client
            .post(&self.url("/api/auth/signup"), &body.to_string())
            .await;

        assert!(
            res.status == 200 || res.status == 201,
            "Signup failed with status {}: {}",
            res.status,
            res.body
        );

        let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
        let token = json["data"]["access_token"].as_str().unwrap().to_string();
        let user = json["data"]["user"].clone();
        (token, user)
    }

    /// Login and return the auth token.
    pub async fn login(&self, email: &str, password: &str) -> String {
        let body = serde_json::json!({
            "email": email,
            "password": password,
        });

        let res = self
            .client
            .post(&self.url("/api/auth/login"), &body.to_string())
            .await;

        assert_eq!(res.status, 200, "Login failed: {}", res.body);

        let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
        json["data"]["access_token"].as_str().unwrap().to_string()
    }
}

/// A simple HTTP test client with helper methods.
#[derive(Clone)]
pub struct TestClient {
    inner: reqwest::Client,
    base_addr: SocketAddr,
}

impl TestClient {
    /// Create a new test client pointing at the given address.
    pub fn new(addr: SocketAddr) -> Self {
        TestClient {
            inner: reqwest::Client::new(),
            base_addr: addr,
        }
    }

    /// Send a GET request.
    pub async fn get(&self, url: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .get(url)
            .send()
            .await
            .expect("GET request failed");
        TestResponse::from_response(res).await
    }

    /// Send a GET request with an auth token.
    pub async fn get_with_auth(&self, url: &str, token: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .expect("GET request failed");
        TestResponse::from_response(res).await
    }

    /// Send a POST request with a JSON body.
    pub async fn post(&self, url: &str, body: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST request failed");
        TestResponse::from_response(res).await
    }

    /// Send a POST request with auth token and JSON body.
    pub async fn post_with_auth(&self, url: &str, token: &str, body: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(body.to_string())
            .send()
            .await
            .expect("POST request failed");
        TestResponse::from_response(res).await
    }

    /// Send a PATCH request with auth token and JSON body.
    pub async fn patch_with_auth(&self, url: &str, token: &str, body: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .patch(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(body.to_string())
            .send()
            .await
            .expect("PATCH request failed");
        TestResponse::from_response(res).await
    }

    /// Send a DELETE request with auth token.
    pub async fn delete_with_auth(&self, url: &str, token: &str) -> TestResponse {
        let res: reqwest::Response = self
            .inner
            .delete(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .expect("DELETE request failed");
        TestResponse::from_response(res).await
    }

    /// Get the base URL.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.base_addr)
    }
}

/// A simplified HTTP response for test assertions.
#[derive(Debug)]
pub struct TestResponse {
    pub status: u16,
    pub body: String,
    pub headers: HeaderMap,
}

impl TestResponse {
    async fn from_response(res: reqwest::Response) -> Self {
        let status = res.status().as_u16();
        let headers = HeaderMap::new();
        let body = res.text().await.unwrap_or_default();
        TestResponse {
            status,
            body,
            headers,
        }
    }

    /// Parse the body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("Failed to parse response as JSON")
    }

    /// Check if the response indicates success.
    pub fn is_success(&self) -> bool {
        let json = self.json();
        json["success"].as_bool().unwrap_or(false)
    }

    /// Get the data field from the response.
    pub fn data(&self) -> serde_json::Value {
        self.json()["data"].clone()
    }

    /// Get the error field from the response.
    pub fn error(&self) -> serde_json::Value {
        self.json()["error"].clone()
    }
}
