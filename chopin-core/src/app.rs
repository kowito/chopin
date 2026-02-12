use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use axum::http::header;
use axum::response::IntoResponse;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::cache::CacheService;
use crate::config::{Config, ServerMode};
use crate::controllers::AppState;
use crate::migrations::Migrator;
use crate::openapi::ApiDoc;
use crate::perf;
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

    /// Build the Axum router for API routes only.
    ///
    /// Note: `/json` and `/plaintext` benchmark endpoints are handled directly
    /// by the hyper server layer (see `server.rs`) and never touch this Router.
    pub fn router(&self) -> Router {
        let config = Arc::new(self.config.clone());
        let is_dev = self.config.is_dev();

        let state = AppState {
            db: self.db.clone(),
            config: config.clone(),
            cache: self.cache.clone(),
        };

        // Initialize cached Date header (updated every 500ms by background task)
        perf::init_date_cache();

        let mut router = Router::new()
            .route("/", get(welcome))
            .merge(routing::build_routes().with_state(state))
            .merge(Scalar::with_url("/api-docs", ApiDoc::openapi()))
            .route("/api-docs/openapi.json", get(openapi_json))
            .layer(axum::Extension(config))
            .layer(CorsLayer::permissive());

        // Only add expensive tracing/request-id middleware in development mode.
        if is_dev {
            let x_request_id = axum::http::HeaderName::from_static("x-request-id");
            router = router
                .layer(SetRequestIdLayer::new(x_request_id.clone(), MakeRequestUuid))
                .layer(PropagateRequestIdLayer::new(x_request_id))
                .layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Run the application server.
    ///
    /// Behaviour depends on [`ServerMode`]:
    ///
    /// - **Standard** — `axum::serve` with full middleware, graceful shutdown.
    ///   Easy to use, great for development and typical production.
    /// - **Performance** — Raw hyper HTTP/1.1 server with SO_REUSEPORT,
    ///   multi-core accept loops, and pre-baked static responses on `/json`
    ///   and `/plaintext`. All other routes still go through Axum.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.server_addr();
        let mode = self.config.server_mode;
        let router = self.router();

        println!("\n🎹 Chopin server is running!");
        println!("   → Mode:    {}", mode);
        println!("   → Server:  http://{}", addr);
        println!("   → API docs: http://{}/api-docs\n", addr);

        tracing::info!("Chopin server running on http://{} (mode: {})", addr, mode);

        match mode {
            ServerMode::Standard => {
                // ─── Easy mode: full Axum pipeline ───
                let listener = tokio::net::TcpListener::bind(&addr).await?;
                axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown_signal())
                    .await?;
            }
            ServerMode::Performance => {
                // ─── Performance mode: raw hyper + SO_REUSEPORT ───
                let socket_addr: std::net::SocketAddr = addr.parse()?;
                crate::server::run_reuseport(socket_addr, router, shutdown_signal()).await?;
            }
        }

        Ok(())
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutting down Chopin server...");
}

// ═══ Application endpoints (served through Axum) ═══

#[derive(Serialize)]
struct WelcomeMessage {
    message: &'static str,
    docs: &'static str,
    status: &'static str,
}

/// Welcome page at `/`.
async fn welcome() -> impl IntoResponse {
    let msg = WelcomeMessage {
        message: "Welcome to Chopin! 🎹",
        docs: "/api-docs",
        status: "running",
    };
    let bytes = sonic_rs::to_vec(&msg).unwrap_or_default();
    (
        [(header::CONTENT_TYPE, "application/json")],
        bytes,
    )
}

/// Serve the raw OpenAPI JSON spec.
async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
