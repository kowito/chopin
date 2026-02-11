use clap::{Parser, Subcommand};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Parser)]
#[command(name = "chopin")]
#[command(about = "The high-level Rust Web Framework for perfectionists with deadlines")]
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
    /// Generate scaffolding
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
    /// Generate a new model with SeaORM entity, migration, and controller
    Model {
        /// Model name (e.g., Post)
        name: String,
        /// Fields in format name:type (e.g., title:string body:text)
        fields: Vec<String>,
    },
    /// Generate a new controller with CRUD endpoints
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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => {
            println!("🎹 Creating new Chopin project: {}", name);
            create_project(&name);
        }
        Commands::Generate { kind } => match kind {
            GenerateCommands::Model { name, fields } => {
                generate_model(&name, &fields);
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
        },
        Commands::Docs { action } => match action {
            DocsCommands::Export { format, output } => {
                println!("Exporting OpenAPI spec as {} to {}", format, output);
                export_openapi(&format, &output);
            }
        },
        Commands::Run => {
            run_dev_server();
        }
        Commands::CreateSuperuser => {
            create_superuser();
        }
        Commands::Info => {
            show_project_info();
        }
    }
}

// ── Helper: map field type shorthand to Rust/SeaORM types ──

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
    s.chars()
        .next()
        .map(|c| c.to_uppercase().to_string() + &s[1..])
        .unwrap_or_default()
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

// ── Generate Model ──

fn generate_model(name: &str, fields: &[String]) {
    let model_name = to_pascal_case(name);
    let snake_name = to_snake_case(name);
    let table_name = format!("{}s", snake_name);

    println!("🎹 Generating model: {}", model_name);

    // Parse fields
    let parsed_fields: Vec<(&str, &str)> = fields
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
        .collect();

    // 1. Generate the model file
    generate_model_file(&model_name, &snake_name, &table_name, &parsed_fields);

    // 2. Generate the migration file
    generate_migration_file(&model_name, &snake_name, &table_name, &parsed_fields);

    // 3. Generate the controller file
    generate_controller_for_model(&model_name, &snake_name, &parsed_fields);

    println!("  ✓ Model, migration, and controller generated.");
    println!();
    println!("  Next: Register the new module in your `src/models/mod.rs` and `src/controllers/mod.rs`");
}

fn generate_model_file(
    model_name: &str,
    snake_name: &str,
    table_name: &str,
    fields: &[(&str, &str)],
) {
    let models_dir = Path::new("src/models");
    if !models_dir.exists() {
        fs::create_dir_all(models_dir).expect("Failed to create src/models");
    }

    let mut model_fields = String::new();
    let mut response_fields = String::new();
    let mut response_from_fields = String::new();

    for (field_name, field_type) in fields {
        let rust_type = field_to_rust_type(field_type);
        model_fields.push_str(&format!("\n    pub {}: {},\n", field_name, rust_type));
        response_fields.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
        response_from_fields.push_str(&format!(
            "            {}: model.{}.clone(),\n",
            field_name, field_name
        ));
    }

    let needs_chrono = fields.iter().any(|(_, t)| *t == "datetime" || *t == "timestamp");
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
{model_fields}
    pub created_at: chrono::NaiveDateTime,
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

    let path = models_dir.join(format!("{}.rs", snake_name));
    fs::write(&path, content).expect("Failed to write model file");
    println!("  ✓ Created {}", path.display());
}

fn generate_migration_file(
    _model_name: &str,
    _snake_name: &str,
    table_name: &str,
    fields: &[(&str, &str)],
) {
    let migrations_dir = Path::new("src/migrations");
    if !migrations_dir.exists() {
        fs::create_dir_all(migrations_dir).expect("Failed to create src/migrations");
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
}

fn generate_controller_for_model(
    model_name: &str,
    snake_name: &str,
    fields: &[(&str, &str)],
) {
    let controllers_dir = Path::new("src/controllers");
    if !controllers_dir.exists() {
        fs::create_dir_all(controllers_dir).expect("Failed to create src/controllers");
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
        r#"use axum::{{extract::{{Path, State}}, routing::{{get, post}}, Router}};
use chrono::Utc;
use sea_orm::{{ActiveModelTrait, EntityTrait, Set}};
use serde::{{Deserialize, Serialize}};
use utoipa::ToSchema;

use crate::error::ChopinError;
use crate::extractors::Json;
use crate::models::{snake_name}::{{self, Entity as {model_name}, {model_name}Response}};
use crate::response::ApiResponse;

use super::AppState;

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
#[utoipa::path(
    get,
    path = "/api/{plural_name}",
    responses(
        (status = 200, description = "List of {plural_name}", body = ApiResponse<Vec<{model_name}Response>>),
    ),
    tag = "{plural_name}"
)]
async fn list(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<{model_name}Response>>, ChopinError> {{
    let items = {model_name}::find()
        .all(&state.db)
        .await?;

    let response: Vec<{model_name}Response> = items.into_iter().map(|m| m.into()).collect();
    Ok(ApiResponse::success(response))
}}

/// Create a new {snake_name}.
#[utoipa::path(
    post,
    path = "/api/{plural_name}",
    request_body = Create{model_name}Request,
    responses(
        (status = 201, description = "{model_name} created", body = ApiResponse<{model_name}Response>),
        (status = 400, description = "Invalid input")
    ),
    tag = "{plural_name}"
)]
async fn create(
    State(state): State<AppState>,
    Json(payload): Json<Create{model_name}Request>,
) -> Result<ApiResponse<{model_name}Response>, ChopinError> {{
    let now = Utc::now().naive_utc();

    let new_item = {snake_name}::ActiveModel {{
{create_fields_set}        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }};

    let model = new_item.insert(&state.db).await?;
    Ok(ApiResponse::success({model_name}Response::from(model)))
}}

/// Get a single {snake_name} by ID.
#[utoipa::path(
    get,
    path = "/api/{plural_name}/{{id}}",
    params(
        ("id" = i32, Path, description = "{model_name} ID")
    ),
    responses(
        (status = 200, description = "{model_name} found", body = ApiResponse<{model_name}Response>),
        (status = 404, description = "{model_name} not found")
    ),
    tag = "{plural_name}"
)]
async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<{model_name}Response>, ChopinError> {{
    let item = {model_name}::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound(format!("{model_name} with id {{}} not found", id)))?;

    Ok(ApiResponse::success({model_name}Response::from(item)))
}}
"#
    );

    let path = controllers_dir.join(format!("{}.rs", snake_name));
    fs::write(&path, content).expect("Failed to write controller file");
    println!("  ✓ Created {}", path.display());
}

// ── Generate Controller (standalone) ──

fn generate_controller(name: &str) {
    let snake_name = to_snake_case(name);
    let model_name = to_pascal_case(name);

    println!("🎹 Generating controller: {}", snake_name);

    let controllers_dir = Path::new("src/controllers");
    if !controllers_dir.exists() {
        fs::create_dir_all(controllers_dir).expect("Failed to create src/controllers");
    }

    let plural_name = format!("{}s", snake_name);

    let content = format!(
        r#"use axum::{{extract::{{Path, State}}, routing::get, Router}};
use serde::{{Deserialize, Serialize}};
use utoipa::ToSchema;

use crate::error::ChopinError;
use crate::extractors::Json;
use crate::response::ApiResponse;

use super::AppState;

// ── Routes ──

pub fn routes() -> Router<AppState> {{
    Router::new()
        .route("/", get(list))
        .route("/{{id}}", get(get_by_id))
}}

// ── Handlers ──

/// List all {plural_name}.
#[utoipa::path(
    get,
    path = "/api/{plural_name}",
    responses(
        (status = 200, description = "List of {plural_name}"),
    ),
    tag = "{plural_name}"
)]
async fn list(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<serde_json::Value>>, ChopinError> {{
    // TODO: Replace with actual model query
    Ok(ApiResponse::success(vec![]))
}}

/// Get a single {snake_name} by ID.
#[utoipa::path(
    get,
    path = "/api/{plural_name}/{{id}}",
    params(
        ("id" = i32, Path, description = "{model_name} ID")
    ),
    responses(
        (status = 200, description = "{model_name} found"),
        (status = 404, description = "{model_name} not found")
    ),
    tag = "{plural_name}"
)]
async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<serde_json::Value>, ChopinError> {{
    // TODO: Replace with actual model query
    Err(ChopinError::NotFound(format!("{model_name} with id {{}} not found", id)))
}}
"#
    );

    let path = controllers_dir.join(format!("{}.rs", snake_name));
    fs::write(&path, content).expect("Failed to write controller file");
    println!("  ✓ Created {}", path.display());
    println!();
    println!("  Next: Register the controller in `src/controllers/mod.rs` and `src/routing.rs`");
}

// ── DB Migrate ──

fn run_migrations() {
    println!("🎹 Running pending database migrations...");

    // Use cargo run to invoke a small migration runner
    let status = Command::new("cargo")
        .args(["run", "--quiet", "--", "--migrate"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("  ✓ Migrations applied successfully.");
        }
        Ok(s) => {
            eprintln!("  ✗ Migration failed with exit code: {}", s);
            eprintln!("  Hint: Migrations run automatically on server startup.");
            eprintln!("  Try `cargo run` to start the server and apply migrations.");
        }
        Err(_) => {
            println!("  Note: Chopin runs migrations automatically on startup.");
            println!("  Start your server with `cargo run` to apply pending migrations.");
        }
    }
}

// ── DB Rollback ──

fn rollback_migrations(steps: u32) {
    println!("🎹 Rolling back {} migration(s)...", steps);

    let status = Command::new("cargo")
        .args(["run", "--quiet", "--", "--rollback", &steps.to_string()])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("  ✓ Rolled back {} migration(s) successfully.", steps);
        }
        Ok(s) => {
            eprintln!("  ✗ Rollback failed with exit code: {}", s);
            eprintln!();
            eprintln!("  Hint: Make sure your migration files have `down()` implemented.");
            eprintln!("  You can also manually rollback by editing the migration table.");
        }
        Err(_) => {
            println!("  Note: To rollback, ensure your app binary supports the --rollback flag.");
            println!();
            println!("  Add this to your main.rs:");
            println!("    let args: Vec<String> = std::env::args().collect();");
            println!("    if args.contains(&\"--rollback\".to_string()) {{");
            println!("        use sea_orm_migration::MigratorTrait;");
            println!("        Migrator::down(&db, Some(1)).await?;");
            println!("    }}");
        }
    }
}

// ── DB Status ──

fn migration_status() {
    println!("🎹 Migration status:");
    println!();

    // Check for migration files
    let migrations_dir = Path::new("src/migrations");
    if !migrations_dir.exists() {
        println!("  No migrations directory found.");
        return;
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

    if migration_files.is_empty() {
        println!("  No migration files found.");
    } else {
        println!("  Found {} migration(s):", migration_files.len());
        for file in &migration_files {
            println!("    📄 {}", file);
        }
    }

    println!();
    println!("  Hint: Run `chopin db migrate` to apply pending migrations.");
    println!("  Hint: Run `chopin db rollback` to rollback the last migration.");
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

fn create_superuser() {
    use std::io::{self, Write};

    println!("🎹 Creating superuser account...");
    println!();

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

    // Run a cargo command that creates the superuser
    let status = Command::new("cargo")
        .args([
            "run", "--quiet", "--",
            "--create-superuser",
            &email,
            &username,
            &password,
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!();
            println!("  ✓ Superuser '{}' created successfully!", username);
        }
        Ok(_) => {
            eprintln!("  ✗ Failed to create superuser.");
            eprintln!();
            eprintln!("  Hint: Add superuser creation support to your main.rs:");
            eprintln!("    if args.contains(&\"--create-superuser\") {{");
            eprintln!("        // Hash password + insert user with role=\"superuser\"");
            eprintln!("    }}");
            eprintln!();
            eprintln!("  Or use the built-in helper:");
            create_superuser_inline(&email, &username, &password);
        }
        Err(_) => {
            eprintln!("  ✗ Could not run cargo. Creating superuser directly...");
            create_superuser_inline(&email, &username, &password);
        }
    }
}

fn create_superuser_inline(email: &str, username: &str, password: &str) {
    // Generate a helper script that creates the superuser
    let script = format!(
        r#"
// Add this to your main.rs to support `chopin createsuperuser`:
//
// use chopin_core::auth::hash_password;
// use sea_orm::{{ActiveModelTrait, Set}};
//
// async fn create_superuser(db: &DatabaseConnection) {{
//     let password_hash = hash_password("{}").unwrap();
//     let now = chrono::Utc::now().naive_utc();
//     let user = user::ActiveModel {{
//         email: Set("{}".to_string()),
//         username: Set("{}".to_string()),
//         password_hash: Set(password_hash),
//         role: Set("superuser".to_string()),
//         is_active: Set(true),
//         created_at: Set(now),
//         updated_at: Set(now),
//         ..Default::default()
//     }};
//     user.insert(db).await.unwrap();
// }}
"#,
        password, email, username
    );
    println!("{}", script);
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
                println!("  Project: {}", line.split('=').nth(1).unwrap_or("unknown").trim().trim_matches('"'));
            }
            if line.starts_with("version") && !line.contains("workspace") {
                println!("  Version: {}", line.split('=').nth(1).unwrap_or("unknown").trim().trim_matches('"'));
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

    // Check for models
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
        println!("  Models:  {} model file(s)", model_count);
    }

    // Check for controllers
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
        println!("  Ctrls:   {} controller file(s)", controller_count);
    }

    // Check for migrations
    let migrations_dir = Path::new("src/migrations");
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
        println!("  Migrs:   {} migration(s)", migration_count);
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

    let status = Command::new("cargo")
        .args(["run"])
        .status();

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

// ── Create Project ──

fn create_project(name: &str) {
    let project_dir = Path::new(name);

    if project_dir.exists() {
        eprintln!("Error: Directory '{}' already exists", name);
        std::process::exit(1);
    }

    // Create directory structure
    let dirs = [
        "",
        "src",
        "src/models",
        "src/controllers",
        "src/migrations",
    ];

    for dir in &dirs {
        fs::create_dir_all(project_dir.join(dir)).expect("Failed to create directory");
    }

    // Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
chopin-core = {{ version = "0.1.0" }}
tokio = {{ version = "1", features = ["rt-multi-thread", "macros"] }}
serde = {{ version = "1", features = ["derive"] }}
tracing = "0.1"
tracing-subscriber = "0.3"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
"#
    );

    // main.rs
    let main_rs = r#"use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = chopin_core::App::new().await?;
    app.run().await?;

    Ok(())
}
"#;

    // .env.example
    let env_example = r#"# Database
DATABASE_URL=sqlite://app.db?mode=rwc

# JWT
JWT_SECRET=your-secret-key-here
JWT_EXPIRY_HOURS=24

# Server
SERVER_PORT=3000
SERVER_HOST=127.0.0.1

# Environment
ENVIRONMENT=development

# Logging (see: https://docs.rs/tracing-subscriber)
# Options: trace, debug, info, warn, error
RUST_LOG=debug

# Cache (optional - uses in-memory by default)
# REDIS_URL=redis://127.0.0.1:6379

# File Uploads
UPLOAD_DIR=./uploads
MAX_UPLOAD_SIZE=10485760
"#;

    // .gitignore
    let gitignore = r#"/target/
*.rs.bk
Cargo.lock
.env
.DS_Store
*.db
"#;

    // .cargo/config.toml for M4 optimization
    let cargo_dir = project_dir.join(".cargo");
    fs::create_dir_all(&cargo_dir).expect("Failed to create .cargo");

    let cargo_config = r#"[target.'cfg(target_arch = "aarch64")']
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+aes,+neon"]
"#;

    // README.md
    let readme = format!(
        r#"# {name}

Built with [Chopin](https://github.com/yourusername/chopin) — the high-level Rust Web Framework.

## Quick Start

```bash
cargo run
```

Server starts at `http://127.0.0.1:3000`

API docs at `http://127.0.0.1:3000/api-docs`

## Generate Models

```bash
chopin generate model Post title:string body:text
```
"#
    );

    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");
    fs::write(project_dir.join("src/main.rs"), main_rs).expect("Failed to write main.rs");
    fs::write(project_dir.join(".env.example"), env_example).expect("Failed to write .env.example");
    fs::write(project_dir.join(".env"), env_example).expect("Failed to write .env");
    fs::write(project_dir.join(".gitignore"), gitignore).expect("Failed to write .gitignore");
    fs::write(cargo_dir.join("config.toml"), cargo_config).expect("Failed to write cargo config");
    fs::write(project_dir.join("README.md"), readme).expect("Failed to write README.md");

    println!();
    println!("  ✓ Project '{}' created successfully!", name);
    println!();
    println!("  Next steps:");
    println!("    cd {}", name);
    println!("    cargo run");
    println!();
    println!("  API docs: http://127.0.0.1:3000/api-docs");
}

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
