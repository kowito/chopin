mod controllers;
mod models;
mod migrations;

use axum::Router;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use tracing_subscriber;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use chopin_core::{config::Config, db};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
}

/// OpenAPI documentation for the example API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chopin Example API",
        version = "0.1.0",
        description = "A complete example application built with Chopin"
    ),
    paths(
        controllers::posts::list_posts,
        controllers::posts::create_post,
        controllers::posts::get_post,
    ),
    components(
        schemas(
            models::post::PostResponse,
            controllers::posts::CreatePostRequest,
            chopin_core::response::ApiResponse<models::post::PostResponse>,
            chopin_core::response::ApiResponse<Vec<models::post::PostResponse>>,
            chopin_core::extractors::Pagination,
        )
    ),
    tags(
        (name = "posts", description = "Post management endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_env()?;
    let database_conn = db::connect(&config).await?;

    // Run example-specific migrations
    tracing::info!("Running example migrations...");
    migrations::Migrator::up(&database_conn, None).await?;

    // Also run core migrations for User table
    tracing::info!("Running core migrations...");
    chopin_core::migrations::Migrator::up(&database_conn, None).await?;

    let state = AppState {
        db: database_conn,
        config: config.clone(),
    };

    // Build router with example routes
    let app = Router::new()
        .merge(controllers::posts::routes())
        .merge(Scalar::with_url("/api-docs", ApiDoc::openapi()))
        .route(
            "/api-docs/openapi.json",
            axum::routing::get(|| async { axum::Json(ApiDoc::openapi()) }),
        )
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = config.server_addr();
    tracing::info!("Example API server running on http://{}", addr);
    tracing::info!("API docs available at http://{}/api-docs", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutting down example server...");
}
