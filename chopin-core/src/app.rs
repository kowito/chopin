use axum::Router;
use axum::routing::get;
use sea_orm::DatabaseConnection;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::config::Config;
use crate::controllers::AppState;
use crate::openapi::ApiDoc;
use crate::routing;

/// The main Chopin application.
pub struct App {
    pub config: Config,
    pub db: DatabaseConnection,
}

impl App {
    /// Create a new Chopin application.
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::from_env()?;
        let db = crate::db::connect(&config).await?;

        Ok(App { config, db })
    }

    /// Create a new Chopin application with a given config.
    pub async fn with_config(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db = crate::db::connect(&config).await?;
        Ok(App { config, db })
    }

    /// Build the Axum router with all middleware and routes.
    pub fn router(&self) -> Router {
        let state = AppState {
            db: self.db.clone(),
            config: self.config.clone(),
        };

        let api_routes = routing::build_routes().with_state(state);

        Router::new()
            .merge(api_routes)
            .merge(Scalar::with_url("/api-docs", ApiDoc::openapi()))
            .route("/api-docs/openapi.json", get(openapi_json))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
    }

    /// Run the application server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.server_addr();
        let router = self.router();

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

/// Serve the raw OpenAPI JSON spec.
async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
