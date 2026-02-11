use axum::Router;
use axum::routing::get;
use axum::response::Json as AxumJson;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::cache::CacheService;
use crate::config::Config;
use crate::controllers::AppState;
use crate::migrations::Migrator;
use crate::openapi::ApiDoc;
use crate::routing;

/// The main Chopin application.
pub struct App {
    pub config: Config,
    pub db: DatabaseConnection,
    pub cache: CacheService,
}

impl App {
    /// Create a new Chopin application.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::from_env()?;
        let db = crate::db::connect(&config).await?;

        // Run pending migrations automatically on startup
        tracing::info!("Running pending database migrations...");
        Migrator::up(&db, None).await?;
        tracing::info!("Migrations complete.");

        // Initialize cache (in-memory by default, Redis if configured)
        let cache = Self::init_cache(&config).await;

        Ok(App { config, db, cache })
    }

    /// Create a new Chopin application with a given config.
    pub async fn with_config(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db = crate::db::connect(&config).await?;

        // Run pending migrations automatically on startup
        tracing::info!("Running pending database migrations...");
        Migrator::up(&db, None).await?;
        tracing::info!("Migrations complete.");

        // Initialize cache
        let cache = Self::init_cache(&config).await;

        Ok(App { config, db, cache })
    }

    /// Initialize the cache backend based on config.
    async fn init_cache(config: &Config) -> CacheService {
        #[cfg(feature = "redis")]
        if let Some(ref redis_url) = config.redis_url {
            match crate::cache::RedisCache::new(redis_url).await {
                Ok(redis_cache) => {
                    tracing::info!("Redis cache connected");
                    return CacheService::new(redis_cache);
                }
                Err(e) => {
                    tracing::warn!("Redis connection failed, falling back to in-memory cache: {}", e);
                }
            }
        }
        let _ = config; // suppress unused warning when redis feature is off
        tracing::info!("Using in-memory cache");
        CacheService::in_memory()
    }

    /// Build the Axum router with all middleware and routes.
    pub fn router(&self) -> Router {
        let state = AppState {
            db: self.db.clone(),
            config: self.config.clone(),
            cache: self.cache.clone(),
        };

        let config = self.config.clone();
        let api_routes = routing::build_routes().with_state(state);

        // Request ID header name
        let x_request_id = axum::http::HeaderName::from_static("x-request-id");

        Router::new()
            .route("/", get(welcome))
            .merge(api_routes)
            .merge(Scalar::with_url("/api-docs", ApiDoc::openapi()))
            .route("/api-docs/openapi.json", get(openapi_json))
            // Inject Config into request extensions so AuthUser extractor can access it
            .layer(axum::middleware::from_fn(move |mut req: axum::extract::Request, next: axum::middleware::Next| {
                let config = config.clone();
                async move {
                    req.extensions_mut().insert(config);
                    next.run(req).await
                }
            }))
            .layer(SetRequestIdLayer::new(x_request_id.clone(), MakeRequestUuid))
            .layer(PropagateRequestIdLayer::new(x_request_id))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
    }

    /// Run the application server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.server_addr();
        let router = self.router();

        println!("\n🎹 Chopin server is running!");
        println!("   → Server: http://{}", addr);
        println!("   → API docs: http://{}/api-docs\n", addr);
        
        tracing::info!("Chopin server running on http://{}", addr);
        tracing::info!("API docs available at http://{}/api-docs", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        Ok(())
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutting down Chopin server...");
}

/// Welcome page at `/`.
async fn welcome() -> AxumJson<serde_json::Value> {
    AxumJson(serde_json::json!({
        "message": "Welcome to Chopin! 🎹",
        "docs": "/api-docs",
        "status": "running"
    }))
}

/// Serve the raw OpenAPI JSON spec.
async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
