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

use crate::auth::rate_limit::RateLimiter;
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
    custom_openapi: Option<utoipa::openapi::OpenApi>,
    api_docs_path: String,
    custom_routes: Vec<Router<AppState>>,
}

impl App {
    /// Create a new Chopin application.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::from_env()?;
        let db = crate::db::connect(&config).await?;

        // Check for CLI database operations (--migrate, --rollback) and exit if present
        Self::handle_db_cli_args(&db).await?;

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
            custom_openapi: None,
            api_docs_path: "/api-docs".to_string(),
            custom_routes: Vec::new(),
        })
    }

    /// Create a new Chopin application with a given config.
    pub async fn with_config(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db = crate::db::connect(&config).await?;

        // Check for CLI database operations (--migrate, --rollback) and exit if present
        Self::handle_db_cli_args(&db).await?;

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
            custom_openapi: None,
            api_docs_path: "/api-docs".to_string(),
            custom_routes: Vec::new(),
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

    /// Handle CLI database operations passed as command-line arguments.
    /// If --migrate or --rollback is detected, perform the operation and exit the process.
    async fn handle_db_cli_args(db: &DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();

        // Check for --migrate flag
        if args.contains(&"--migrate".to_string()) {
            tracing::info!("Running pending database migrations...");
            Migrator::up(db, None).await?;
            tracing::info!("Migrations complete.");
            std::process::exit(0);
        }

        // Check for --rollback flag
        if let Some(pos) = args.iter().position(|arg| arg == "--rollback") {
            let steps = if pos + 1 < args.len() {
                args[pos + 1].parse::<u32>().unwrap_or(1)
            } else {
                1
            };
            tracing::info!("Rolling back {} migration(s)...", steps);
            Migrator::down(db, Some(steps)).await?;
            tracing::info!("Rollback complete.");
            std::process::exit(0);
        }

        Ok(())
    }

    /// Build the Axum router for API routes only.
    ///
    /// Fast routes registered via [`fast_route`](Self::fast_route) bypass this
    /// Router entirely — they are handled at the hyper `ChopinService` layer.
    pub fn router(&self) -> Router {
        let config = Arc::new(self.config.clone());
        let is_dev = self.config.is_dev();

        let rate_limiter = Arc::new(RateLimiter::new(
            self.config.security.rate_limit_max_attempts,
            self.config.security.rate_limit_window_secs,
        ));

        let state = AppState {
            db: self.db.clone(),
            config: config.clone(),
            cache: self.cache.clone(),
            rate_limiter,
        };

        // Initialize cached Date header (updated every 500ms by background task)
        perf::init_date_cache();

        // Resolve which OpenAPI spec to serve.
        // If user provided a custom spec, merge it with built-in auth docs.
        // Otherwise, use the built-in auth docs only.
        let openapi_spec = match &self.custom_openapi {
            Some(user_spec) => crate::openapi::merge_openapi(ApiDoc::openapi(), user_spec.clone()),
            None => ApiDoc::openapi(),
        };
        let openapi_spec_clone = openapi_spec.clone();
        let docs_path: &'static str = Box::leak(self.api_docs_path.clone().into_boxed_str());
        let json_path: &'static str =
            Box::leak(format!("{}/openapi.json", docs_path).into_boxed_str());

        // Only add the default welcome route if no FastRoute is registered for "/"
        let has_root_fast_route = self.fast_routes.iter().any(|fr| fr.path() == "/");
        let mut router = Router::new();
        if !has_root_fast_route {
            router = router.route("/", get(welcome));
        }
        router = router.merge(routing::build_routes().with_state(state.clone()));

        // Merge user-provided custom routes (with AppState applied).
        for custom in &self.custom_routes {
            router = router.merge(custom.clone().with_state(state.clone()));
        }

        router = router
            .merge(Scalar::with_url(docs_path, openapi_spec))
            .route(
                json_path,
                get(move || {
                    let spec = openapi_spec_clone.clone();
                    async move { axum::Json(spec) }
                }),
            )
            .layer(axum::Extension(config))
            .layer(CorsLayer::permissive());

        // Only add expensive tracing/request-id middleware in development mode.
        if is_dev {
            use tower_http::trace::DefaultMakeSpan;
            use tower_http::trace::DefaultOnRequest;
            use tower_http::trace::DefaultOnResponse;
            use tower_http::LatencyUnit;

            let x_request_id = axum::http::HeaderName::from_static("x-request-id");
            router = router
                .layer(SetRequestIdLayer::new(
                    x_request_id.clone(),
                    MakeRequestUuid,
                ))
                .layer(PropagateRequestIdLayer::new(x_request_id))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
                        .on_response(
                            DefaultOnResponse::new()
                                .level(tracing::Level::INFO)
                                .latency_unit(LatencyUnit::Millis),
                        ),
                );
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
    /// use chopin_core::{App, FastRoute};
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

    // ═══ Custom Routes Builder API ═══

    /// Merge a custom Axum [`Router`] into the application.
    ///
    /// Use this to add your own endpoints alongside Chopin's built-in
    /// auth routes and OpenAPI docs. You can call this multiple times to
    /// merge several routers.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use chopin_core::prelude::*;
    ///
    /// async fn list_items() -> Json<Vec<&'static str>> {
    ///     Json(vec!["Notebook", "Pen"])
    /// }
    ///
    /// let items_router = Router::new()
    ///     .route("/api/items", get(list_items));
    ///
    /// let app = App::new().await?
    ///     .routes(items_router)
    ///     .api_docs(MyApiDoc::openapi());
    /// app.run().await?;
    /// ```
    pub fn routes(mut self, router: Router<AppState>) -> Self {
        self.custom_routes.push(router);
        self
    }

    // ═══ API Documentation Builder API ═══

    /// Provide a custom OpenAPI spec for your API documentation.
    ///
    /// Your spec is **merged** with the built-in Chopin auth endpoints,
    /// so both your endpoints and the auth endpoints appear in `/api-docs`.
    ///
    /// This follows the same pattern as Axum + utoipa — annotate handlers
    /// with `#[utoipa::path(...)]`, define an `#[derive(OpenApi)]` struct,
    /// and pass it here.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use chopin_core::prelude::*;
    ///
    /// #[derive(OpenApi)]
    /// #[openapi(
    ///     info(title = "My API", version = "1.0.0"),
    ///     paths(list_posts, create_post),
    ///     components(schemas(PostResponse, CreatePostRequest)),
    ///     tags((name = "posts", description = "Post endpoints")),
    ///     security(("bearer_auth" = [])),
    ///     modifiers(&SecurityAddon)
    /// )]
    /// struct MyApiDoc;
    ///
    /// let app = App::new().await?
    ///     .api_docs(MyApiDoc::openapi());
    /// app.run().await?;
    /// ```
    pub fn api_docs(mut self, openapi: utoipa::openapi::OpenApi) -> Self {
        self.custom_openapi = Some(openapi);
        self
    }

    /// Customize the URL path where API docs are served.
    ///
    /// Default: `/api-docs` (Scalar UI) and `/api-docs/openapi.json` (raw spec).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let app = App::new().await?
    ///     .api_docs_url("/docs")              // Scalar UI at /docs
    ///     .api_docs(MyApiDoc::openapi());      // openapi.json at /docs/openapi.json
    /// ```
    pub fn api_docs_url(mut self, path: &str) -> Self {
        self.api_docs_path = path.to_string();
        self
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
        println!("   → API docs: http://{}{}", addr, self.api_docs_path);
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
