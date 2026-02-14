//! # Chopin Basic API Example
//!
//! A complete CRUD API with authentication, pagination, and OpenAPI docs.

use std::sync::Arc;

use chopin::{config::Config, db, serve, Extension, Router};
use sea_orm_migration::MigratorTrait;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use chopin_basic_api::{controllers, migrations, models, AppState};

/// OpenAPI documentation for the example API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chopin Basic API",
        version = "0.2.0",
        description = "A complete CRUD example built with Chopin — auth, pagination, OpenAPI"
    ),
    paths(
        controllers::posts::list_posts,
        controllers::posts::create_post,
        controllers::posts::get_post,
        controllers::posts::update_post,
        controllers::posts::delete_post,
    ),
    components(
        schemas(
            models::post::PostResponse,
            controllers::posts::CreatePostRequest,
            controllers::posts::UpdatePostRequest,
            chopin::response::ApiResponse<models::post::PostResponse>,
            chopin::response::ApiResponse<Vec<models::post::PostResponse>>,
            chopin::extractors::Pagination,
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

    let config = Config::from_env()?;
    let database_conn = db::connect(&config).await?;

    // Run migrations
    tracing::info!("Running migrations...");
    migrations::Migrator::up(&database_conn, None).await?;
    chopin::migrations::Migrator::up(&database_conn, None).await?;

    let state = AppState {
        db: database_conn,
        config: config.clone(),
    };

    let app = Router::new()
        .merge(controllers::posts::routes())
        .merge(Scalar::with_url("/api-docs", ApiDoc::openapi()))
        .route(
            "/api-docs/openapi.json",
            chopin::routing::get(|| async { chopin::extractors::Json(ApiDoc::openapi()) }),
        )
        .with_state(state)
        .layer(Extension(Arc::new(config.clone())))
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = config.server_addr();
    println!("\n🎹 Chopin Basic API Example");
    println!("   → Server:   http://{}", addr);
    println!("   → API docs: http://{}/api-docs\n", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
}
