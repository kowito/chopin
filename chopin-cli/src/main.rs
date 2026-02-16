use clap::{Parser, Subcommand};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Parser)]
#[command(name = "chopin")]
#[command(about = "🎹 Chopin — High-fidelity engineering for the modern virtuoso.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Chopin project
    New {
        /// Project name
        name: String,
    },
    /// Generate scaffolding (module, model, controller)
    #[command(alias = "g")]
    Generate {
        #[command(subcommand)]
        kind: GenerateCommands,
    },
    /// Database operations
    Db {
        #[command(subcommand)]
        action: DbCommands,
    },
    /// OpenAPI documentation operations
    Docs {
        #[command(subcommand)]
        action: DocsCommands,
    },
    /// Create a new database migration (Django-style: chopin makemigrations create_posts title:string body:text)
    #[command(name = "makemigrations", alias = "mm")]
    Makemigrations {
        /// Migration name (e.g., create_posts, add_slug_to_posts, custom_fix)
        name: String,
        /// Fields in format name:type (e.g., title:string body:text published:bool)
        fields: Vec<String>,
        /// Create an empty migration template
        #[arg(long)]
        empty: bool,
    },
    /// Run pending database migrations (Django-style: chopin migrate)
    Migrate,
    /// Create a new app module (Django-style: chopin startapp blog)
    #[command(alias = "startapp")]
    Startapp {
        /// App/module name (e.g., blog, billing, inventory)
        name: String,
    },
    /// Start the development server
    Run,
    /// Create a superuser account
    #[command(name = "createsuperuser")]
    CreateSuperuser,
    /// Show project info and status
    Info,
}

#[derive(Subcommand)]
enum GenerateCommands {
    /// Generate a new feature module (MVSR: model, handlers, services, routes)
    #[command(alias = "mod")]
    Module {
        /// Module name (e.g., blog, billing, inventory)
        name: String,
    },
    /// Generate a new model with SeaORM entity, migration, and handler
    Model {
        /// Model name (e.g., Post, Product)
        name: String,
        /// Fields in format name:type (e.g., title:string body:text price:f64)
        fields: Vec<String>,
        /// Target module to generate into (default: generates in src/)
        #[arg(short, long)]
        module: Option<String>,
    },
    /// Generate a standalone controller with endpoints
    Controller {
        /// Controller name (e.g., posts)
        name: String,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Run pending migrations
    Migrate,
    /// Rollback the last migration (or N migrations)
    Rollback {
        /// Number of migrations to rollback (default: 1)
        #[arg(short, long, default_value = "1")]
        steps: u32,
    },
    /// Show migration status
    Status,
    /// Reset database (rollback all + re-migrate)
    Reset,
    /// Seed the database with sample data
    Seed,
    /// Create a new migration (alias: chopin makemigrations)
    #[command(name = "makemigrations")]
    Makemigrations {
        /// Migration name (e.g., create_posts, add_slug_to_posts)
        name: String,
        /// Fields in format name:type (e.g., title:string body:text)
        fields: Vec<String>,
        /// Create an empty migration template
        #[arg(long)]
        empty: bool,
    },
}

#[derive(Subcommand)]
enum DocsCommands {
    /// Export OpenAPI spec to file
    Export {
        /// Output format: json or yaml
        #[arg(long, default_value = "json")]
        format: String,
        /// Output file path
        #[arg(long, default_value = "openapi.json")]
        output: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => {
            create_project(&name);
        }
        Commands::Generate { kind } => match kind {
            GenerateCommands::Module { name } => {
                generate_module(&name);
            }
            GenerateCommands::Model {
                name,
                fields,
                module,
            } => {
                generate_model(&name, &fields, module.as_deref());
            }
            GenerateCommands::Controller { name } => {
                generate_controller(&name);
            }
        },
        Commands::Db { action } => match action {
            DbCommands::Migrate => {
                run_migrations();
            }
            DbCommands::Rollback { steps } => {
                rollback_migrations(steps);
            }
            DbCommands::Status => {
                migration_status();
            }
            DbCommands::Reset => {
                reset_database();
            }
            DbCommands::Seed => {
                seed_database();
            }
            DbCommands::Makemigrations {
                name,
                fields,
                empty,
            } => {
                make_migrations(&name, &fields, empty);
            }
        },
        Commands::Docs { action } => match action {
            DocsCommands::Export { format, output } => {
                println!("Exporting OpenAPI spec as {} to {}", format, output);
                export_openapi(&format, &output);
            }
        },
        Commands::Startapp { name } => {
            generate_module(&name);
        }
        Commands::Makemigrations {
            name,
            fields,
            empty,
        } => {
            make_migrations(&name, &fields, empty);
        }
        Commands::Migrate => {
            run_migrations();
        }
        Commands::Run => {
            run_dev_server();
        }
        Commands::CreateSuperuser => {
            create_superuser().await;
        }
        Commands::Info => {
            show_project_info();
        }
    }
}

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

fn field_to_rust_type(field_type: &str) -> &str {
    match field_type {
        "string" | "str" => "String",
        "text" => "String",
        "i32" | "int" | "integer" => "i32",
        "i64" | "bigint" => "i64",
        "f32" | "float" => "f32",
        "f64" | "double" => "f64",
        "bool" | "boolean" => "bool",
        "datetime" | "timestamp" => "NaiveDateTime",
        "uuid" => "Uuid",
        _ => "String",
    }
}

fn field_to_sea_orm_col(field_type: &str) -> &str {
    match field_type {
        "string" | "str" => "string()",
        "text" => "text()",
        "i32" | "int" | "integer" => "integer()",
        "i64" | "bigint" => "big_integer()",
        "f32" | "float" => "float()",
        "f64" | "double" => "double()",
        "bool" | "boolean" => "boolean()",
        "datetime" | "timestamp" => "timestamp()",
        "uuid" => "uuid()",
        _ => "string()",
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
            }
        })
        .collect()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap());
    }
    result
}

fn parse_fields(fields: &[String]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .filter_map(|f| {
            let parts: Vec<&str> = f.split(':').collect();
            if parts.len() == 2 {
                Some((parts[0], parts[1]))
            } else {
                eprintln!("  ⚠ Skipping invalid field: {} (expected name:type)", f);
                None
            }
        })
        .collect()
}

// ══════════════════════════════════════════════════════════════
// chopin new <name>
// ══════════════════════════════════════════════════════════════

fn create_project(name: &str) {
    let project_dir = Path::new(name);

    if project_dir.exists() {
        eprintln!("  ✗ Directory '{}' already exists.", name);
        std::process::exit(1);
    }

    println!("🎹 Creating new Chopin project: {}", name);
    println!();

    // ── Directory tree ──
    let dirs = [
        "src",
        "src/apps",
        "src/shared",
        "migrations",
        "tests",
        ".cargo",
    ];
    for dir in &dirs {
        fs::create_dir_all(project_dir.join(dir)).expect("Failed to create directory");
    }

    // ── Cargo.toml ──
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
chopin-core = "0.2"
tokio = {{ version = "1", features = ["rt-multi-thread", "macros"] }}
serde = {{ version = "1", features = ["derive"] }}
sea-orm = {{ version = "1", features = ["runtime-tokio-rustls", "sqlx-sqlite", "sqlx-postgres"] }}
sea-orm-migration = "1"
chrono = "0.4"
async-trait = "0.1"
tracing = "0.1"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
"#
    );

    // ── src/main.rs — The "Composer" ──
    let main_rs = format!(
        r#"//! {name} — built with Chopin 🎹
//!
//! Module registration happens here. Each feature is a self-contained
//! `ChopinModule` that declares its own routes, migrations, and health checks.

use chopin_core::prelude::*;

// ── Import your feature modules here ──
// mod apps;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {{
    init_logging();

    let app = App::new().await?;
        // AuthModule is mounted by default.
        // Mount your own modules here:
        // .mount_module(apps::blog::BlogModule::new())
        // .mount_module(apps::billing::BillingModule::new())

    app.run().await?;

    Ok(())
}}
"#
    );

    // ── src/apps/mod.rs — Feature module registry ──
    let apps_mod = r#"//! Feature modules — each sub-folder is a self-contained ChopinModule.
//!
//! Register new modules here:
//!
//! ```rust
//! pub mod blog;
//! pub mod billing;
//! ```
//!
//! Then mount them in main.rs:
//!
//! ```rust
//! .mount_module(apps::blog::BlogModule::new())
//! ```
"#;

    // ── src/shared/mod.rs ──
    let shared_mod = r#"//! Shared types used across feature modules.
//!
//! Put cross-cutting concerns here (permissions, common DTOs, etc.).
//! Modules should depend on `chopin-core` and `shared/`, never on each other.
"#;

    // ── migrations/mod.rs ──
    let migrations_mod = r#"//! Application-level migrations.
//!
//! Note: Built-in auth migrations run automatically via AuthModule.
//! Add your own migrations here and register them in a Migrator.

pub use sea_orm_migration::prelude::*;
"#;

    // ── tests/health_test.rs ──
    let health_test = r#"//! Smoke test — verify the app boots and the welcome endpoint responds.

use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_app_boots() {
    let app = TestApp::new().await;
    let res = app.get("/").await;
    assert_eq!(res.status, 200);
}
"#;

    // ── .env.example ──
    let env_example = r#"# ── Database ──────────────────────────────────
DATABASE_URL=sqlite://app.db?mode=rwc

# ── JWT ───────────────────────────────────────
JWT_SECRET=change-me-in-production
JWT_EXPIRY_HOURS=24

# ── Server ────────────────────────────────────
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
ENVIRONMENT=development

# ── Logging ───────────────────────────────────
RUST_LOG=info

# ── Cache (optional, in-memory by default) ────
# REDIS_URL=redis://127.0.0.1:6379

# ── File uploads ──────────────────────────────
UPLOAD_DIR=./uploads
MAX_UPLOAD_SIZE=10485760
"#;

    // ── .gitignore ──
    let gitignore = r#"/target/
*.rs.bk
Cargo.lock
.env
.DS_Store
*.db
/uploads/
"#;

    // ── .cargo/config.toml ──
    let cargo_config = r#"# Optimise for Apple Silicon (harmless on other architectures)
[target.'cfg(target_arch = "aarch64")']
rustflags = ["-C", "target-cpu=native"]
"#;

    // ── README.md ──
    let readme = format!(
        r#"# {name}

Built with [Chopin](https://github.com/kowito/chopin) 🎹 — the high-level Rust Web Framework.

## Quick Start

```bash
cp .env.example .env
cargo run
```

- Server: `http://127.0.0.1:3000`
- API docs: `http://127.0.0.1:3000/api-docs`

## Project Structure

```
{name}/
├── src/
│   ├── main.rs          # The "Composer" — mounts all modules
│   ├── apps/            # Feature modules (MVSR pattern)
│   │   └── mod.rs
│   └── shared/          # Cross-cutting types & utilities
│       └── mod.rs
├── migrations/          # App-level database migrations
├── tests/               # Integration tests
├── .env.example
└── Cargo.toml
```

## Add a Feature Module

```bash
chopin generate module blog
```

This creates `src/apps/blog/` with:
- `mod.rs`       — `ChopinModule` implementation
- `handlers.rs`  — HTTP handler functions
- `services.rs`  — Pure business logic
- `models.rs`    — SeaORM entities
- `routes.rs`    — Route definitions
- `dto.rs`       — Request/response types

Then register it in `src/main.rs`:

```rust
.mount_module(apps::blog::BlogModule::new())
```

## Generate a Model

```bash
chopin generate model Post title:string body:text --module blog
```
"#
    );

    // ── Write all files ──
    let writes: Vec<(&str, &str)> = vec![
        ("Cargo.toml", &cargo_toml),
        ("src/main.rs", &main_rs),
        ("src/apps/mod.rs", apps_mod),
        ("src/shared/mod.rs", shared_mod),
        ("migrations/mod.rs", migrations_mod),
        ("tests/health_test.rs", health_test),
        (".env.example", env_example),
        (".env", env_example),
        (".gitignore", gitignore),
        (".cargo/config.toml", cargo_config),
        ("README.md", &readme),
    ];

    for (rel_path, content) in &writes {
        let path = project_dir.join(rel_path);
        fs::write(&path, content).unwrap_or_else(|_| panic!("Failed to write {}", path.display()));
    }

    // Print summary
    println!("  ✓ Created project structure:");
    println!("      {}/", name);
    println!("      ├── src/");
    println!("      │   ├── main.rs          # Module composer");
    println!("      │   ├── apps/            # Feature modules (MVSR)");
    println!("      │   └── shared/          # Shared types");
    println!("      ├── migrations/          # App-level migrations");
    println!("      ├── tests/");
    println!("      │   └── health_test.rs");
    println!("      ├── .env.example");
    println!("      ├── .cargo/config.toml");
    println!("      └── Cargo.toml");
    println!();
    println!("  Next steps:");
    println!("    cd {}", name);
    println!("    cargo run");
    println!();
    println!("  Generate your first module:");
    println!("    chopin generate module blog");
    println!();
    println!("  API docs: http://127.0.0.1:3000/api-docs");
}

// ══════════════════════════════════════════════════════════════
// chopin generate module <name>
// ══════════════════════════════════════════════════════════════

fn generate_module(name: &str) {
    let snake_name = to_snake_case(name);
    let pascal_name = to_pascal_case(name);

    let module_dir = Path::new("src/apps").join(&snake_name);

    if module_dir.exists() {
        eprintln!(
            "  ✗ Module '{}' already exists at {}",
            snake_name,
            module_dir.display()
        );
        std::process::exit(1);
    }

    println!("🎹 Generating module: {}", pascal_name);
    println!();

    fs::create_dir_all(&module_dir).expect("Failed to create module directory");

    // ── mod.rs — ChopinModule implementation ──
    let mod_rs = format!(
        r#"//! {pascal_name} module — implements `ChopinModule` for self-contained composition.

mod dto;
mod handlers;
mod models;
mod routes;
mod services;

pub use dto::*;
pub use models::*;

use async_trait::async_trait;
use axum::Router;
use chopin_core::controllers::AppState;
use chopin_core::error::ChopinError;
use chopin_core::module::ChopinModule;
use sea_orm::DatabaseConnection;

/// {pascal_name} feature module.
///
/// Mount in `main.rs`:
/// ```rust,ignore
/// .mount_module({pascal_name}Module::new())
/// ```
pub struct {pascal_name}Module;

impl {pascal_name}Module {{
    pub fn new() -> Self {{
        Self
    }}
}}

impl Default for {pascal_name}Module {{
    fn default() -> Self {{
        Self::new()
    }}
}}

#[async_trait]
impl ChopinModule for {pascal_name}Module {{
    fn name(&self) -> &str {{
        "{snake_name}"
    }}

    fn routes(&self) -> Router<AppState> {{
        routes::routes()
    }}

    async fn migrate(&self, _db: &DatabaseConnection) -> Result<(), ChopinError> {{
        // TODO: Add module-specific migrations
        Ok(())
    }}
}}
"#
    );

    // ── routes.rs ──
    let routes_rs = format!(
        r#"//! Route definitions for the {pascal_name} module.

use axum::Router;
use chopin_core::controllers::AppState;
use chopin_core::routing::{{get, post, put, delete}};

use super::handlers;

/// All routes for the {snake_name} module, nested under `/api/{snake_name}s`.
pub fn routes() -> Router<AppState> {{
    Router::new().nest(
        "/api/{snake_name}s",
        Router::new()
            .route("/", get(handlers::list).post(handlers::create))
            .route("/{{id}}", get(handlers::get_by_id).put(handlers::update).delete(handlers::remove)),
    )
}}
"#
    );

    // ── handlers.rs ──
    let handlers_rs = format!(
        r#"//! HTTP handlers for the {pascal_name} module.
//!
//! Handlers are thin adapters: extract request data, call a service, return a response.
//! Business logic lives in `services.rs`.

use axum::extract::{{Path, State}};
use chopin_core::controllers::AppState;
use chopin_core::error::ChopinError;
use chopin_core::extractors::Json;
use chopin_core::response::ApiResponse;

use super::dto::*;
use super::services;

/// List all {snake_name}s.
#[utoipa::path(get, path = "/api/{snake_name}s", tag = "{snake_name}s",
    responses((status = 200, description = "List of {snake_name}s"))
)]
pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ChopinError> {{
    let items = services::list_all(&state.db).await?;
    Ok(Json(ApiResponse::success(items)))
}}

/// Create a new {snake_name}.
#[utoipa::path(post, path = "/api/{snake_name}s", tag = "{snake_name}s",
    request_body = Create{pascal_name}Request,
    responses((status = 201, description = "{pascal_name} created"))
)]
pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<Create{pascal_name}Request>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ChopinError> {{
    let item = services::create_one(&state.db, payload).await?;
    Ok(Json(ApiResponse::success(item)))
}}

/// Get a {snake_name} by ID.
#[utoipa::path(get, path = "/api/{snake_name}s/{{id}}", tag = "{snake_name}s",
    params(("id" = i32, Path, description = "{pascal_name} ID")),
    responses(
        (status = 200, description = "{pascal_name} found"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ChopinError> {{
    let item = services::find_by_id(&state.db, id).await?;
    Ok(Json(ApiResponse::success(item)))
}}

/// Update a {snake_name}.
#[utoipa::path(put, path = "/api/{snake_name}s/{{id}}", tag = "{snake_name}s",
    params(("id" = i32, Path, description = "{pascal_name} ID")),
    responses((status = 200, description = "{pascal_name} updated"))
)]
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<Update{pascal_name}Request>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ChopinError> {{
    let item = services::update_one(&state.db, id, payload).await?;
    Ok(Json(ApiResponse::success(item)))
}}

/// Delete a {snake_name}.
#[utoipa::path(delete, path = "/api/{snake_name}s/{{id}}", tag = "{snake_name}s",
    params(("id" = i32, Path, description = "{pascal_name} ID")),
    responses((status = 200, description = "{pascal_name} deleted"))
)]
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<()>>, ChopinError> {{
    services::delete_one(&state.db, id).await?;
    Ok(Json(ApiResponse::success(())))
}}
"#
    );

    // ── services.rs ──
    let services_rs = format!(
        r#"//! Business logic for the {pascal_name} module.
//!
//! Services are pure Rust functions — no HTTP types.
//! They receive a `DatabaseConnection` and return domain types.
//! This makes them 100% unit-testable without a running server.

use chopin_core::error::ChopinError;
use sea_orm::DatabaseConnection;

use super::dto::*;

/// Fetch all {snake_name}s.
pub async fn list_all(
    _db: &DatabaseConnection,
) -> Result<Vec<serde_json::Value>, ChopinError> {{
    // TODO: Replace with actual SeaORM query
    //   let items = {pascal_name}::find().all(db).await?;
    Ok(vec![])
}}

/// Create a new {snake_name}.
pub async fn create_one(
    _db: &DatabaseConnection,
    _payload: Create{pascal_name}Request,
) -> Result<serde_json::Value, ChopinError> {{
    // TODO: Implement creation logic
    Err(ChopinError::Internal("Not implemented yet".into()))
}}

/// Find a {snake_name} by ID.
pub async fn find_by_id(
    _db: &DatabaseConnection,
    id: i32,
) -> Result<serde_json::Value, ChopinError> {{
    // TODO: Implement lookup
    Err(ChopinError::NotFound(format!("{pascal_name} with id {{}} not found", id)))
}}

/// Update a {snake_name}.
pub async fn update_one(
    _db: &DatabaseConnection,
    id: i32,
    _payload: Update{pascal_name}Request,
) -> Result<serde_json::Value, ChopinError> {{
    // TODO: Implement update
    Err(ChopinError::NotFound(format!("{pascal_name} with id {{}} not found", id)))
}}

/// Delete a {snake_name}.
pub async fn delete_one(
    _db: &DatabaseConnection,
    id: i32,
) -> Result<(), ChopinError> {{
    // TODO: Implement deletion
    Err(ChopinError::NotFound(format!("{pascal_name} with id {{}} not found", id)))
}}
"#
    );

    // ── dto.rs ──
    let dto_rs = format!(
        r#"//! Request and response DTOs for the {pascal_name} module.

use serde::{{Deserialize, Serialize}};
use utoipa::ToSchema;

/// Request body for creating a new {snake_name}.
#[derive(Debug, Deserialize, ToSchema)]
pub struct Create{pascal_name}Request {{
    // TODO: Add fields
    // pub title: String,
}}

/// Request body for updating a {snake_name}.
#[derive(Debug, Deserialize, ToSchema)]
pub struct Update{pascal_name}Request {{
    // TODO: Add fields
    // pub title: Option<String>,
}}

/// Public {pascal_name} response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct {pascal_name}Response {{
    pub id: i32,
    // TODO: Add fields
}}
"#
    );

    // ── models.rs ──
    let models_rs = format!(
        r#"//! SeaORM entities for the {pascal_name} module.
//!
//! Generate entities with:
//!   chopin generate model {pascal_name} title:string body:text --module {snake_name}

// TODO: Add SeaORM entity definitions here.
// Example:
//
// use sea_orm::entity::prelude::*;
// use serde::{{Deserialize, Serialize}};
//
// #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
// #[sea_orm(table_name = "{snake_name}s")]
// pub struct Model {{
//     #[sea_orm(primary_key)]
//     pub id: i32,
//     pub created_at: chrono::NaiveDateTime,
//     pub updated_at: chrono::NaiveDateTime,
// }}
//
// #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
// pub enum Relation {{}}
//
// impl ActiveModelBehavior for ActiveModel {{}}
"#
    );

    // ── Write all files ──
    let files: Vec<(&str, &str)> = vec![
        ("mod.rs", &mod_rs),
        ("routes.rs", &routes_rs),
        ("handlers.rs", &handlers_rs),
        ("services.rs", &services_rs),
        ("dto.rs", &dto_rs),
        ("models.rs", &models_rs),
    ];

    for (file, content) in &files {
        let path = module_dir.join(file);
        fs::write(&path, content).unwrap_or_else(|_| panic!("Failed to write {}", path.display()));
    }

    // ── Create test file ──
    let tests_dir = Path::new("tests");
    if !tests_dir.exists() {
        fs::create_dir_all(tests_dir).ok();
    }
    let test_file = tests_dir.join(format!("{}_tests.rs", snake_name));
    if !test_file.exists() {
        let test_content = format!(
            r#"//! Integration tests for the {pascal_name} module.

use chopin_core::testing::TestApp;

#[tokio::test]
async fn test_{snake_name}_list() {{
    let app = TestApp::new().await;
    let res = app.get("/api/{snake_name}s").await;
    assert_eq!(res.status, 200);
}}

#[tokio::test]
async fn test_{snake_name}_not_found() {{
    let app = TestApp::new().await;
    let res = app.get("/api/{snake_name}s/999").await;
    assert_eq!(res.status, 404);
}}
"#
        );
        fs::write(&test_file, test_content).ok();
    }

    // ── Auto-register in src/apps/mod.rs ──
    let apps_mod_path = Path::new("src/apps/mod.rs");
    let mut registered = false;
    if apps_mod_path.exists() {
        let content = fs::read_to_string(apps_mod_path).unwrap_or_default();
        let decl = format!("pub mod {};", snake_name);
        if !content.contains(&decl) {
            let mut new_content = content.clone();
            // Append the pub mod declaration
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str(&format!("pub mod {};\n", snake_name));
            fs::write(apps_mod_path, new_content).ok();
            registered = true;
        }
    }

    // ── Auto-mount in src/main.rs ──
    let main_rs_path = Path::new("src/main.rs");
    let mut mounted = false;
    if main_rs_path.exists() {
        let content = fs::read_to_string(main_rs_path).unwrap_or_default();
        let mount_call = format!("{}Module::new()", pascal_name);

        if !content.contains(&mount_call) {
            let mut new_content = content.clone();

            // Uncomment `mod apps;` if it's still commented out
            if new_content.contains("// mod apps;") && !new_content.contains("\nmod apps;") {
                new_content = new_content.replace("// mod apps;", "mod apps;");
            }

            // Insert .mount_module() after App::new().await?
            // Look for the pattern: `App::new().await?;` or `App::new().await?`
            if let Some(pos) = new_content.find("App::new().await?") {
                // Find the semicolon after App::new().await?
                if let Some(semi_offset) = new_content[pos..].find(';') {
                    let insert_pos = pos + semi_offset + 1;
                    let mount_line = format!(
                        "\n    let app = app.mount_module(apps::{}::{}Module::new());",
                        snake_name, pascal_name
                    );
                    // Only insert if not already present
                    if !new_content.contains(&mount_line.trim().to_string()) {
                        new_content.insert_str(insert_pos, &mount_line);
                        mounted = true;
                    }
                }
            }

            if new_content != content {
                fs::write(main_rs_path, new_content).ok();
            }
        }
    }

    // Print summary
    println!("  ✓ Created app at src/apps/{}/", snake_name);
    println!("      ├── mod.rs         # ChopinModule implementation");
    println!("      ├── routes.rs      # Route definitions");
    println!("      ├── handlers.rs    # HTTP handlers");
    println!("      ├── services.rs    # Business logic");
    println!("      ├── dto.rs         # Request/response types");
    println!("      └── models.rs      # SeaORM entities");
    println!();

    if registered {
        println!("  ✓ Registered in src/apps/mod.rs");
    }
    if mounted {
        println!("  ✓ Mounted in src/main.rs");
    }
    if test_file.exists() {
        println!("  ✓ Created tests/{}_tests.rs", snake_name);
    }
    println!();

    if !registered || !mounted {
        println!("  Manual steps (if auto-registration was skipped):");
        if !registered {
            println!("    1. Add to src/apps/mod.rs:  pub mod {};", snake_name);
        }
        if !mounted {
            println!(
                "    2. Add to src/main.rs:      .mount_module(apps::{}::{}Module::new())",
                snake_name, pascal_name
            );
        }
        println!();
    }

    println!("  Generate a model:");
    println!(
        "    chopin generate model {} title:string body:text --module {}",
        pascal_name, snake_name
    );
}

// ══════════════════════════════════════════════════════════════
// chopin generate model <name> [fields...] [--module <mod>]
// ══════════════════════════════════════════════════════════════

fn generate_model(name: &str, fields: &[String], module: Option<&str>) {
    let model_name = to_pascal_case(name);
    let snake_name = to_snake_case(name);
    let table_name = format!("{}s", snake_name);

    let parsed_fields = parse_fields(fields);

    println!("🎹 Generating model: {}", model_name);

    // Determine target directories based on whether --module is set
    let (models_dir, migrations_dir, controller_dir) = if let Some(mod_name) = module {
        let mod_snake = to_snake_case(mod_name);
        let base = Path::new("src/apps").join(&mod_snake);
        if !base.exists() {
            eprintln!(
                "  ✗ Module '{}' not found. Run `chopin generate module {}` first.",
                mod_snake, mod_snake
            );
            std::process::exit(1);
        }
        (base.clone(), Path::new("migrations").to_path_buf(), base)
    } else {
        (
            Path::new("src/models").to_path_buf(),
            Path::new("src/migrations").to_path_buf(),
            Path::new("src/controllers").to_path_buf(),
        )
    };

    // 1. Generate model entity
    generate_model_file(
        &model_name,
        &snake_name,
        &table_name,
        &parsed_fields,
        &models_dir,
        module.is_some(),
    );

    // 2. Generate migration
    generate_migration_file(&table_name, &parsed_fields, &migrations_dir);

    // 3. Generate controller/handler (only when not inside a module — modules already have handlers)
    if module.is_none() {
        generate_controller_for_model(&model_name, &snake_name, &parsed_fields, &controller_dir);
    }

    println!();
    if let Some(mod_name) = module {
        println!(
            "  Next: Update src/apps/{}/models.rs to include the entity.",
            to_snake_case(mod_name)
        );
        println!(
            "  Next: Update src/apps/{}/services.rs with query logic.",
            to_snake_case(mod_name)
        );
    } else {
        println!("  Next: Register in src/models/mod.rs and src/controllers/mod.rs");
    }
}

fn generate_model_file(
    model_name: &str,
    snake_name: &str,
    table_name: &str,
    fields: &[(&str, &str)],
    target_dir: &Path,
    inside_module: bool,
) {
    if !target_dir.exists() {
        fs::create_dir_all(target_dir).expect("Failed to create directory");
    }

    let mut model_fields = String::new();
    let mut response_fields = String::new();
    let mut response_from_fields = String::new();

    for (field_name, field_type) in fields {
        let rust_type = field_to_rust_type(field_type);
        model_fields.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
        response_fields.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
        response_from_fields.push_str(&format!(
            "            {}: model.{}.clone(),\n",
            field_name, field_name
        ));
    }

    let needs_chrono = fields
        .iter()
        .any(|(_, t)| *t == "datetime" || *t == "timestamp");
    let needs_uuid = fields.iter().any(|(_, t)| *t == "uuid");

    let mut extra_imports = String::new();
    if needs_chrono {
        extra_imports.push_str("use chrono::NaiveDateTime;\n");
    }
    if needs_uuid {
        extra_imports.push_str("use uuid::Uuid;\n");
    }

    let content = format!(
        r#"use sea_orm::entity::prelude::*;
use serde::{{Deserialize, Serialize}};
use utoipa::ToSchema;
{extra_imports}
/// {model_name} entity.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "{table_name}")]
pub struct Model {{
    #[sea_orm(primary_key)]
    pub id: i32,
{model_fields}    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {{}}

impl ActiveModelBehavior for ActiveModel {{}}

/// Public {model_name} response (safe to return in API responses).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct {model_name}Response {{
    pub id: i32,
{response_fields}    pub created_at: chrono::NaiveDateTime,
}}

impl From<Model> for {model_name}Response {{
    fn from(model: Model) -> Self {{
        {model_name}Response {{
            id: model.id,
{response_from_fields}            created_at: model.created_at,
        }}
    }}
}}
"#
    );

    let filename = if inside_module {
        format!("{}_entity.rs", snake_name)
    } else {
        format!("{}.rs", snake_name)
    };
    let path = target_dir.join(&filename);
    fs::write(&path, content).expect("Failed to write model file");
    println!("  ✓ Created {}", path.display());
}

fn generate_migration_file(table_name: &str, fields: &[(&str, &str)], migrations_dir: &Path) {
    if !migrations_dir.exists() {
        fs::create_dir_all(migrations_dir).expect("Failed to create migrations dir");
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let migration_name = format!("m{}_create_{}_table", timestamp, table_name);
    let iden_name = to_pascal_case(table_name);

    let mut columns = String::new();
    let mut iden_variants = String::new();

    for (field_name, field_type) in fields {
        let col_method = field_to_sea_orm_col(field_type);
        let variant = to_pascal_case(field_name);
        columns.push_str(&format!(
            r#"                    .col(
                        ColumnDef::new({iden_name}::{variant})
                            .{col_method}
                            .not_null(),
                    )
"#
        ));
        iden_variants.push_str(&format!("    {},\n", variant));
    }

    let content = format!(
        r#"use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {{
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
        manager
            .create_table(
                Table::create()
                    .table({iden_name}::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new({iden_name}::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
{columns}                    .col(
                        ColumnDef::new({iden_name}::CreatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new({iden_name}::UpdatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }}

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
        manager
            .drop_table(Table::drop().table({iden_name}::Table).to_owned())
            .await
    }}
}}

#[derive(Iden)]
enum {iden_name} {{
    Table,
    Id,
{iden_variants}    CreatedAt,
    UpdatedAt,
}}
"#
    );

    let path = migrations_dir.join(format!("{}.rs", migration_name));
    fs::write(&path, content).expect("Failed to write migration file");
    println!("  ✓ Created {}", path.display());

    // Auto-register in migrations/mod.rs
    update_migrations_mod_rs();
}

fn generate_controller_for_model(
    model_name: &str,
    snake_name: &str,
    fields: &[(&str, &str)],
    controllers_dir: &Path,
) {
    if !controllers_dir.exists() {
        fs::create_dir_all(controllers_dir).expect("Failed to create controllers dir");
    }

    let plural_name = format!("{}s", snake_name);

    let mut create_fields_struct = String::new();
    let mut create_fields_set = String::new();

    for (field_name, field_type) in fields {
        let rust_type = field_to_rust_type(field_type);
        create_fields_struct.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
        create_fields_set.push_str(&format!(
            "        {}: Set(payload.{}.clone()),\n",
            field_name, field_name
        ));
    }

    let content = format!(
        r#"use axum::extract::{{Path, State}};
use axum::Router;
use chopin_core::controllers::AppState;
use chopin_core::error::ChopinError;
use chopin_core::extractors::Json;
use chopin_core::response::ApiResponse;
use chopin_core::routing::{{get, post}};
use chrono::Utc;
use sea_orm::{{ActiveModelTrait, EntityTrait, Set}};
use serde::{{Deserialize, Serialize}};
use utoipa::ToSchema;

use crate::models::{snake_name}::{{self, Entity as {model_name}, {model_name}Response}};

// ── Request types ──

#[derive(Debug, Deserialize, ToSchema)]
pub struct Create{model_name}Request {{
{create_fields_struct}}}

// ── Routes ──

pub fn routes() -> Router<AppState> {{
    Router::new()
        .route("/", get(list).post(create))
        .route("/{{id}}", get(get_by_id))
}}

// ── Handlers ──

/// List all {plural_name}.
#[utoipa::path(get, path = "/api/{plural_name}", tag = "{plural_name}",
    responses((status = 200, description = "List of {plural_name}", body = ApiResponse<Vec<{model_name}Response>>))
)]
async fn list(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<{model_name}Response>>>, ChopinError> {{
    let items = {model_name}::find().all(&state.db).await?;
    let response: Vec<{model_name}Response> = items.into_iter().map(|m| m.into()).collect();
    Ok(Json(ApiResponse::success(response)))
}}

/// Create a new {snake_name}.
#[utoipa::path(post, path = "/api/{plural_name}", tag = "{plural_name}",
    request_body = Create{model_name}Request,
    responses(
        (status = 201, description = "{model_name} created", body = ApiResponse<{model_name}Response>),
        (status = 400, description = "Invalid input"),
    )
)]
async fn create(
    State(state): State<AppState>,
    Json(payload): Json<Create{model_name}Request>,
) -> Result<Json<ApiResponse<{model_name}Response>>, ChopinError> {{
    let now = Utc::now().naive_utc();
    let new_item = {snake_name}::ActiveModel {{
{create_fields_set}        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }};
    let model = new_item.insert(&state.db).await?;
    Ok(Json(ApiResponse::success({model_name}Response::from(model))))
}}

/// Get a {snake_name} by ID.
#[utoipa::path(get, path = "/api/{plural_name}/{{id}}", tag = "{plural_name}",
    params(("id" = i32, Path, description = "{model_name} ID")),
    responses(
        (status = 200, description = "{model_name} found", body = ApiResponse<{model_name}Response>),
        (status = 404, description = "Not found"),
    )
)]
async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<{model_name}Response>>, ChopinError> {{
    let item = {model_name}::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound(format!("{model_name} {{}} not found", id)))?;
    Ok(Json(ApiResponse::success({model_name}Response::from(item))))
}}
"#
    );

    let path = controllers_dir.join(format!("{}.rs", snake_name));
    fs::write(&path, content).expect("Failed to write controller file");
    println!("  ✓ Created {}", path.display());
}

// ══════════════════════════════════════════════════════════════
// chopin generate controller <name>
// ══════════════════════════════════════════════════════════════

fn generate_controller(name: &str) {
    let snake_name = to_snake_case(name);
    let model_name = to_pascal_case(name);
    let plural_name = format!("{}s", snake_name);

    println!("🎹 Generating controller: {}", snake_name);

    let controllers_dir = Path::new("src/controllers");
    if !controllers_dir.exists() {
        fs::create_dir_all(controllers_dir).expect("Failed to create src/controllers");
    }

    let content = format!(
        r#"use axum::extract::{{Path, State}};
use axum::Router;
use chopin_core::controllers::AppState;
use chopin_core::error::ChopinError;
use chopin_core::extractors::Json;
use chopin_core::response::ApiResponse;
use chopin_core::routing::get;

// ── Routes ──

pub fn routes() -> Router<AppState> {{
    Router::new()
        .route("/", get(list))
        .route("/{{id}}", get(get_by_id))
}}

// ── Handlers ──

/// List all {plural_name}.
#[utoipa::path(get, path = "/api/{plural_name}", tag = "{plural_name}",
    responses((status = 200, description = "List of {plural_name}"))
)]
async fn list(
    State(_state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ChopinError> {{
    // TODO: Replace with actual model query
    Ok(Json(ApiResponse::success(vec![])))
}}

/// Get a {snake_name} by ID.
#[utoipa::path(get, path = "/api/{plural_name}/{{id}}", tag = "{plural_name}",
    params(("id" = i32, Path, description = "{model_name} ID")),
    responses(
        (status = 200, description = "{model_name} found"),
        (status = 404, description = "Not found"),
    )
)]
async fn get_by_id(
    State(_state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ChopinError> {{
    // TODO: Replace with actual model query
    Err(ChopinError::NotFound(format!("{model_name} {{}} not found", id)))
}}
"#
    );

    let path = controllers_dir.join(format!("{}.rs", snake_name));
    fs::write(&path, content).expect("Failed to write controller file");
    println!("  ✓ Created {}", path.display());
    println!();
    println!(
        "  Hint: Consider using `chopin generate module {}` for a full MVSR module.",
        snake_name
    );
}

// ══════════════════════════════════════════════════════════════
// chopin makemigrations <name> [fields...] [--empty]
// ══════════════════════════════════════════════════════════════

fn make_migrations(name: &str, fields: &[String], empty: bool) {
    let snake_name = to_snake_case(name);

    println!("🎹 Creating migration: {}", snake_name);
    println!();

    // Generate timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let migration_name = format!("m{}_{}", timestamp, snake_name);

    // Ensure migrations/ directory exists
    let migrations_dir = Path::new("migrations");
    fs::create_dir_all(migrations_dir).expect("Failed to create migrations directory");

    let parsed_fields = parse_fields(fields);

    // Determine migration type from name convention:
    //   "create_posts"       → CREATE TABLE posts
    //   "add_slug_to_posts"  → ALTER TABLE posts ADD COLUMN slug
    //   (anything else)      → empty or create table from fields
    let content = if empty || (parsed_fields.is_empty() && fields.is_empty()) {
        generate_empty_migration_content()
    } else if let Some(table) = snake_name.strip_prefix("create_") {
        generate_create_table_content(table, &parsed_fields)
    } else if snake_name.starts_with("add_") {
        if let Some(pos) = snake_name.rfind("_to_") {
            let table = &snake_name[pos + 4..];
            generate_alter_table_content(table, &parsed_fields)
        } else {
            generate_create_table_content(&snake_name, &parsed_fields)
        }
    } else if !parsed_fields.is_empty() {
        // Has fields but no recognized prefix → create table
        generate_create_table_content(&format!("{}s", snake_name), &parsed_fields)
    } else {
        generate_empty_migration_content()
    };

    // Write migration file
    let file_path = migrations_dir.join(format!("{}.rs", migration_name));
    fs::write(&file_path, &content).expect("Failed to write migration file");
    println!("  ✓ Created {}", file_path.display());

    // Update migrations/mod.rs (auto-register)
    update_migrations_mod_rs();
    println!("  ✓ Registered in migrations/mod.rs");

    println!();
    println!("  Next steps:");
    println!("    1. Review: {}", file_path.display());
    println!("    2. Run:    chopin migrate");
    println!();
    println!("  Make sure your main.rs runs project migrations:");
    println!("    mod migrations;");
    println!("    use sea_orm_migration::MigratorTrait;");
    println!("    migrations::ProjectMigrator::up(&app.db, None).await?;");
}

/// Rebuild `migrations/mod.rs` by scanning all migration files.
fn update_migrations_mod_rs() {
    let migrations_dir = Path::new("migrations");
    let mod_path = migrations_dir.join("mod.rs");

    // Collect all migration module names (sorted)
    let mut migration_names: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(migrations_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('m') && name.ends_with(".rs") && name != "mod.rs" {
                let module_name = name.strip_suffix(".rs").unwrap().to_string();
                migration_names.push(module_name);
            }
        }
    }

    migration_names.sort();

    if migration_names.is_empty() {
        return;
    }

    // Generate mod.rs content
    let mod_declarations: String = migration_names
        .iter()
        .map(|n| format!("mod {};", n))
        .collect::<Vec<_>>()
        .join("\n");

    let box_entries: String = migration_names
        .iter()
        .map(|n| format!("            Box::new({}::Migration),", n))
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        r#"pub use sea_orm_migration::prelude::*;

{mod_declarations}

pub struct ProjectMigrator;

#[async_trait::async_trait]
impl MigratorTrait for ProjectMigrator {{
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {{
        vec![
{box_entries}
        ]
    }}
}}
"#
    );

    fs::write(&mod_path, content).expect("Failed to write migrations/mod.rs");
}

/// Generate an empty migration template.
fn generate_empty_migration_content() -> String {
    r#"use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Add your migration logic here
        //
        // Example — create a table:
        //   manager.create_table(
        //       Table::create()
        //           .table(Alias::new("my_table"))
        //           .col(ColumnDef::new(Alias::new("id")).integer().not_null().auto_increment().primary_key())
        //           .col(ColumnDef::new(Alias::new("name")).string().not_null())
        //           .to_owned(),
        //   ).await?;
        //
        // Example — add a column:
        //   manager.alter_table(
        //       Table::alter()
        //           .table(Alias::new("my_table"))
        //           .add_column(ColumnDef::new(Alias::new("email")).string().not_null())
        //           .to_owned(),
        //   ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Reverse the migration
        Ok(())
    }
}
"#
    .to_string()
}

/// Generate a CREATE TABLE migration.
fn generate_create_table_content(table_name: &str, fields: &[(&str, &str)]) -> String {
    let iden_name = to_pascal_case(table_name);

    let mut columns = String::new();
    let mut iden_variants = String::new();

    for (field_name, field_type) in fields {
        let col_method = field_to_sea_orm_col(field_type);
        let variant = to_pascal_case(field_name);
        columns.push_str(&format!(
            r#"                    .col(
                        ColumnDef::new({iden_name}::{variant})
                            .{col_method}
                            .not_null(),
                    )
"#
        ));
        iden_variants.push_str(&format!("    {},\n", variant));
    }

    format!(
        r#"use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {{
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
        manager
            .create_table(
                Table::create()
                    .table({iden_name}::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new({iden_name}::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
{columns}                    .col(
                        ColumnDef::new({iden_name}::CreatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new({iden_name}::UpdatedAt)
                            .timestamp()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }}

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
        manager
            .drop_table(Table::drop().table({iden_name}::Table).to_owned())
            .await
    }}
}}

#[derive(Iden)]
enum {iden_name} {{
    Table,
    Id,
{iden_variants}    CreatedAt,
    UpdatedAt,
}}
"#
    )
}

/// Generate an ALTER TABLE (add columns) migration.
fn generate_alter_table_content(table_name: &str, fields: &[(&str, &str)]) -> String {
    let iden_name = to_pascal_case(table_name);

    let mut alter_stmts = String::new();
    let mut drop_stmts = String::new();
    let mut iden_variants = String::new();

    for (field_name, field_type) in fields {
        let col_method = field_to_sea_orm_col(field_type);
        let variant = to_pascal_case(field_name);
        iden_variants.push_str(&format!("    {},\n", variant));

        alter_stmts.push_str(&format!(
            r#"        manager
            .alter_table(
                Table::alter()
                    .table({iden_name}::Table)
                    .add_column(
                        ColumnDef::new({iden_name}::{variant})
                            .{col_method}
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
"#
        ));

        drop_stmts.push_str(&format!(
            r#"        manager
            .alter_table(
                Table::alter()
                    .table({iden_name}::Table)
                    .drop_column({iden_name}::{variant})
                    .to_owned(),
            )
            .await?;
"#
        ));
    }

    format!(
        r#"use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {{
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
{alter_stmts}
        Ok(())
    }}

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {{
{drop_stmts}
        Ok(())
    }}
}}

#[derive(Iden)]
enum {iden_name} {{
    Table,
{iden_variants}}}
"#
    )
}

// ══════════════════════════════════════════════════════════════
// chopin migrate / chopin db migrate
// ══════════════════════════════════════════════════════════════

// ── DB Migrate ──

fn run_migrations() {
    println!("🎹 Running database migrations...");
    println!();

    // Compile and run the user's app with --migrate flag.
    // App::run() handles: core migrations + module migrations + exit.
    let status = Command::new("cargo")
        .args(["run", "--", "--migrate"])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("  ✗ Migration failed (exit code: {})", s);
            eprintln!();
            eprintln!("  Check:");
            eprintln!("    1. DATABASE_URL is set in .env");
            eprintln!("    2. Database server is running");
            eprintln!("    3. Migration files compile correctly");
            eprintln!();
            eprintln!("  If you have project-level migrations, make sure main.rs includes:");
            eprintln!("    mod migrations;");
            eprintln!("    use sea_orm_migration::MigratorTrait;");
            eprintln!("    migrations::ProjectMigrator::up(&app.db, None).await?;");
        }
        Err(e) => {
            eprintln!("  ✗ Failed to run: {}", e);
            eprintln!("  Make sure you're in a Chopin project directory with Cargo.toml.");
        }
    }
}

// ── DB Rollback ──

fn rollback_migrations(steps: u32) {
    println!("🎹 Rolling back {} migration(s)...", steps);

    let status = Command::new("cargo")
        .args(["run", "--", "--rollback", &steps.to_string()])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("  ✗ Rollback failed (exit code: {})", s);
            eprintln!();
            eprintln!("  Make sure your migration files have `down()` implemented.");
        }
        Err(e) => {
            eprintln!("  ✗ Failed to run: {}", e);
            eprintln!("  Make sure you're in a Chopin project directory.");
        }
    }
}

// ── DB Status ──

fn migration_status() {
    println!("🎹 Migration status:");
    println!();

    let mut total = 0;

    // Scan both migration directories
    for dir_path in &["migrations", "src/migrations"] {
        let migrations_dir = Path::new(dir_path);
        if !migrations_dir.exists() {
            continue;
        }

        let mut migration_files: Vec<String> = Vec::new();
        if let Ok(entries) = fs::read_dir(migrations_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('m') && name.ends_with(".rs") && name != "mod.rs" {
                    migration_files.push(name);
                }
            }
        }

        migration_files.sort();

        if !migration_files.is_empty() {
            println!("  {}/", dir_path);
            for file in &migration_files {
                println!("    ✓ {}", file);
            }
            total += migration_files.len();
        }
    }

    if total == 0 {
        println!("  No migration files found.");
        println!();
        println!("  Create one with:");
        println!("    chopin makemigrations create_posts title:string body:text");
    } else {
        println!();
        println!("  {} migration(s) total", total);
    }

    println!();
    println!("  Commands:");
    println!("    chopin makemigrations <name> [fields...]  — create a migration");
    println!("    chopin migrate                           — apply pending migrations");
    println!("    chopin db rollback                       — rollback last migration");
}

// ── DB Reset ──

fn reset_database() {
    println!("🎹 Resetting database...");
    println!("  ⚠  This will drop all tables and re-run all migrations!");
    println!();

    // Simple confirmation
    print!("  Are you sure? (yes/no): ");
    use std::io::{self, Write};
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    if input.trim().to_lowercase() != "yes" {
        println!("  Cancelled.");
        return;
    }

    // Delete the SQLite database file if it exists
    let db_files = ["chopin.db", "app.db"];
    for db_file in &db_files {
        if Path::new(db_file).exists() {
            fs::remove_file(db_file).ok();
            println!("  ✓ Removed {}", db_file);
        }
    }

    // Re-run migrations
    run_migrations();
}

// ── DB Seed ──

fn seed_database() {
    println!("🎹 Seeding database...");

    let seed_file = Path::new("src/seed.rs");
    if !seed_file.exists() {
        println!("  No seed file found at src/seed.rs");
        println!();
        println!("  Create a seed file with sample data:");
        println!("  ```");
        println!("  // src/seed.rs");
        println!("  pub async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {{");
        println!("      // Insert sample data here");
        println!("      Ok(())");
        println!("  }}");
        println!("  ```");
        return;
    }

    let status = Command::new("cargo")
        .args(["run", "--quiet", "--", "--seed"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("  ✓ Database seeded successfully.");
        }
        _ => {
            eprintln!("  ✗ Seeding failed. Check your seed.rs file.");
        }
    }
}

// ── Create Superuser ──

async fn create_superuser() {
    use sea_orm::{ActiveModelTrait, Database, Set};
    use std::io::{self, Write};

    println!("🎹 Creating superuser account...");
    println!();

    // Load .env file
    if let Err(e) = dotenvy::dotenv() {
        eprintln!("  ✗ Failed to load .env file: {}", e);
        eprintln!("  Make sure you have a .env file with DATABASE_URL set.");
        return;
    }

    // Get DATABASE_URL
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("  ✗ DATABASE_URL not found in environment.");
            eprintln!("  Add DATABASE_URL to your .env file.");
            return;
        }
    };

    // Prompt for email
    print!("  Email: ");
    io::stdout().flush().unwrap();
    let mut email = String::new();
    io::stdin().read_line(&mut email).unwrap();
    let email = email.trim().to_string();

    if email.is_empty() {
        eprintln!("  ✗ Email is required.");
        return;
    }

    // Prompt for username
    print!("  Username: ");
    io::stdout().flush().unwrap();
    let mut username = String::new();
    io::stdin().read_line(&mut username).unwrap();
    let username = username.trim().to_string();

    if username.is_empty() {
        eprintln!("  ✗ Username is required.");
        return;
    }

    // Prompt for password (simple, no hidden input for now)
    print!("  Password: ");
    io::stdout().flush().unwrap();
    let mut password = String::new();
    io::stdin().read_line(&mut password).unwrap();
    let password = password.trim().to_string();

    if password.len() < 8 {
        eprintln!("  ✗ Password must be at least 8 characters.");
        return;
    }

    // Confirm password
    print!("  Confirm password: ");
    io::stdout().flush().unwrap();
    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm).unwrap();
    let confirm = confirm.trim().to_string();

    if password != confirm {
        eprintln!("  ✗ Passwords do not match.");
        return;
    }

    // Connect to database
    println!();
    println!("  Connecting to database...");
    let db = match Database::connect(&database_url).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("  ✗ Failed to connect to database: {}", e);
            return;
        }
    };

    // Hash password
    let password_hash = match chopin_core::auth::hash_password(&password) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("  ✗ Failed to hash password: {}", e);
            return;
        }
    };

    // Create user
    let now = chrono::Utc::now().naive_utc();

    // Import the user entity from chopin_core
    use chopin_core::models::user;

    let user_model = user::ActiveModel {
        email: Set(email.clone()),
        username: Set(username.clone()),
        password_hash: Set(password_hash),
        role: Set("superuser".to_string()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    // Insert user
    match user_model.insert(&db).await {
        Ok(_) => {
            println!("  ✓ Superuser '{}' created successfully!", username);
            println!();
        }
        Err(e) => {
            eprintln!("  ✗ Failed to create superuser: {}", e);
            eprintln!();
            eprintln!("  This might be because:");
            eprintln!("    - A user with this email or username already exists");
            eprintln!("    - The users table doesn't exist (run migrations first)");
            eprintln!("    - Database connection issues");
        }
    }
}

// ── Project Info ──

fn show_project_info() {
    println!("🎹 Chopin Project Info");
    println!();

    // Check if we're in a Chopin project
    let cargo_toml = Path::new("Cargo.toml");
    if !cargo_toml.exists() {
        eprintln!("  Not in a Rust project directory (no Cargo.toml found).");
        return;
    }

    // Read Cargo.toml to extract project info
    if let Ok(content) = fs::read_to_string(cargo_toml) {
        for line in content.lines() {
            if line.starts_with("name") {
                println!(
                    "  Project: {}",
                    line.split('=')
                        .nth(1)
                        .unwrap_or("unknown")
                        .trim()
                        .trim_matches('"')
                );
            }
            if line.starts_with("version") && !line.contains("workspace") {
                println!(
                    "  Version: {}",
                    line.split('=')
                        .nth(1)
                        .unwrap_or("unknown")
                        .trim()
                        .trim_matches('"')
                );
            }
        }
    }

    // Check for .env
    if Path::new(".env").exists() {
        println!("  Config:  .env ✓");
    } else if Path::new(".env.example").exists() {
        println!("  Config:  .env.example found (copy to .env)");
    } else {
        println!("  Config:  No .env file");
    }

    // Check for feature modules (new MVSR structure)
    let apps_dir = Path::new("src/apps");
    if apps_dir.exists() {
        let module_count = fs::read_dir(apps_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .count()
            })
            .unwrap_or(0);
        println!("  Modules: {} feature module(s)", module_count);
    }

    // Check for models (flat legacy structure)
    let models_dir = Path::new("src/models");
    if models_dir.exists() {
        let model_count = fs::read_dir(models_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".rs") && name != "mod.rs"
                    })
                    .count()
            })
            .unwrap_or(0);
        if model_count > 0 {
            println!("  Models:  {} model file(s)", model_count);
        }
    }

    // Check for controllers (flat legacy structure)
    let controllers_dir = Path::new("src/controllers");
    if controllers_dir.exists() {
        let controller_count = fs::read_dir(controllers_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".rs") && name != "mod.rs"
                    })
                    .count()
            })
            .unwrap_or(0);
        if controller_count > 0 {
            println!("  Ctrls:   {} controller file(s)", controller_count);
        }
    }

    // Check for migrations (both locations)
    for migrations_path in &["migrations", "src/migrations"] {
        let migrations_dir = Path::new(migrations_path);
        if migrations_dir.exists() {
            let migration_count = fs::read_dir(migrations_dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            name.starts_with('m') && name.ends_with(".rs")
                        })
                        .count()
                })
                .unwrap_or(0);
            if migration_count > 0 {
                println!(
                    "  Migrs:   {} migration(s) ({})",
                    migration_count, migrations_path
                );
            }
        }
    }

    // Check for database file
    for db_file in &["chopin.db", "app.db"] {
        if Path::new(db_file).exists() {
            let metadata = fs::metadata(db_file);
            let size = metadata.map(|m| m.len()).unwrap_or(0);
            println!("  DB:      {} ({} KB)", db_file, size / 1024);
        }
    }

    println!();
}

// ── Dev Server ──

fn run_dev_server() {
    println!("🎹 Starting Chopin development server...");
    println!();

    let status = Command::new("cargo").args(["run"]).status();

    match status {
        Ok(s) if !s.success() => {
            eprintln!("Server exited with code: {}", s);
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            eprintln!("Make sure you're in a Chopin project directory.");
        }
        _ => {}
    }
}

// (create_project is defined above with the MVSR module structure)

fn export_openapi(format: &str, output: &str) {
    use chopin_core::openapi::ApiDoc;
    use utoipa::OpenApi;

    let spec = match format {
        "json" => ApiDoc::openapi()
            .to_pretty_json()
            .expect("Failed to generate JSON"),
        "yaml" => ApiDoc::openapi()
            .to_yaml()
            .expect("Failed to generate YAML"),
        _ => {
            eprintln!("Unsupported format: {}. Use 'json' or 'yaml'.", format);
            std::process::exit(1);
        }
    };

    fs::write(output, spec).expect("Failed to write file");
    println!("OpenAPI spec exported to: {}", output);
}
