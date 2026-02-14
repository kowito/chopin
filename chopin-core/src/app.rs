use std::sync::Arc;

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde::Serialize;
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
use crate::perf;
use crate::routing;
use crate::server::FastRoute;

/// The main Chopin application.
pub struct App {
    pub config: Config,
    pub db: DatabaseConnection,
    pub cache: CacheService,
    fast_routes: Vec<FastRoute>,
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

        Ok(App {
            config,
            db,
            cache,
            fast_routes: Vec::new(),
        })
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

        Ok(App {
            config,
            db,
            cache,
            fast_routes: Vec::new(),
        })
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
                    tracing::warn!(
                        "Redis connection failed, falling back to in-memory cache: {}",
                        e
                    );
                }
            }
        }
        let _ = config; // suppress unused warning when redis feature is off
        tracing::info!("Using in-memory cache");
        CacheService::in_memory()
    }

    /// Build the Axum router for API routes only.
    ///
    /// Fast routes registered via [`fast_route`](Self::fast_route) bypass this
    /// Router entirely — they are handled at the hyper `ChopinService` layer.
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
                .layer(SetRequestIdLayer::new(
                    x_request_id.clone(),
                    MakeRequestUuid,
                ))
                .layer(PropagateRequestIdLayer::new(x_request_id))
                .layer(TraceLayer::new_for_http());
        }

        router
    }

    // ═══ Fast Route Builder API ═══

    /// Register a [`FastRoute`] — a zero-allocation static response endpoint
    /// that bypasses Axum middleware.
    ///
    /// Use builder methods on the `FastRoute` to configure per-route behavior.
    /// All decorators are pre-computed at registration time (zero per-request cost).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use chopin::{App, FastRoute};
    ///
    /// let app = App::new().await?
    ///     // Bare: maximum performance
    ///     .fast_route(FastRoute::json("/json", br#"{"message":"Hello, World!"}"#))
    ///
    ///     // With CORS + method filter (still zero per-request cost)
    ///     .fast_route(
    ///         FastRoute::json("/api/status", br#"{"status":"ok"}"#)
    ///             .cors()
    ///             .get_only()
    ///     )
    ///
    ///     // With Cache-Control
    ///     .fast_route(
    ///         FastRoute::text("/health", b"OK")
    ///             .cache_control("public, max-age=60")
    ///     );
    ///
    /// app.run().await?;
    /// ```
    pub fn fast_route(mut self, route: FastRoute) -> Self {
        self.fast_routes.push(route);
        self
    }

    /// Convenience: register a JSON fast route (`Content-Type: application/json`).
    ///
    /// ```rust,ignore
    /// app.fast_json("/json", br#"{"message":"Hello, World!"}"#)
    /// ```
    pub fn fast_json(self, path: &str, body: &'static [u8]) -> Self {
        self.fast_route(FastRoute::json(path, body))
    }

    /// Convenience: register a plaintext fast route (`Content-Type: text/plain`).
    ///
    /// ```rust,ignore
    /// app.fast_text("/plaintext", b"Hello, World!")
    /// ```
    pub fn fast_text(self, path: &str, body: &'static [u8]) -> Self {
        self.fast_route(FastRoute::text(path, body))
    }

    /// Run the application server.
    ///
    /// All requests flow through `ChopinService`:
    /// - FastRoute match → zero-alloc pre-computed response
    /// - No match → Axum Router with full middleware
    ///
    /// When `reuseport` is enabled (via `REUSEPORT=true` env var),
    /// each CPU core gets its own SO_REUSEPORT listener and
    /// single-threaded tokio runtime for maximum throughput.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.server_addr();
        let reuseport = self.config.reuseport;
        let router = self.router();
        let fast_routes: Arc<[FastRoute]> = self.fast_routes.into();

        println!("\n🎹 Chopin server is running!");
        println!("   → Server:  http://{}", addr);
        println!("   → API docs: http://{}/api-docs", addr);
        if reuseport {
            println!("   → SO_REUSEPORT: enabled (multi-core)");
        }
        if !fast_routes.is_empty() {
            println!("   → Fast routes: {}", fast_routes.len());
            for r in fast_routes.iter() {
                println!("     • {}", r);
            }
        }
        println!();

        tracing::info!(
            "Chopin server running on http://{} (reuseport: {})",
            addr,
            reuseport
        );

        if reuseport {
            // SO_REUSEPORT: per-core single-threaded runtimes
            let socket_addr: std::net::SocketAddr = addr.parse()?;
            crate::server::run_reuseport(socket_addr, fast_routes, router, shutdown_signal())
                .await?;
        } else {
            // Single listener with ChopinService (FastRoute still works)
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            crate::server::run_until(listener, fast_routes, router, shutdown_signal()).await?;
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
    let mut buf = Vec::with_capacity(64);
    let _ = crate::json::to_writer(&mut buf, &msg);
    ([(header::CONTENT_TYPE, "application/json")], buf)
}

/// Serve the raw OpenAPI JSON spec.
async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
