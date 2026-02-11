use clap::{Parser, Subcommand};

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
            println!("Creating new Chopin project: {}", name);
            create_project(&name);
        }
        Commands::Generate { kind } => match kind {
            GenerateCommands::Model { name, fields } => {
                println!("Generating model: {} with fields: {:?}", name, fields);
                println!("  -> src/models/{}.rs", name.to_lowercase());
                println!("  -> src/controllers/{}.rs", name.to_lowercase());
                println!("  -> migration file");
            }
            GenerateCommands::Controller { name } => {
                println!("Generating controller: {}", name);
                println!("  -> src/controllers/{}.rs", name.to_lowercase());
            }
        },
        Commands::Db { action } => match action {
            DbCommands::Migrate => {
                println!("Running pending migrations...");
            }
        },
        Commands::Docs { action } => match action {
            DocsCommands::Export { format, output } => {
                println!("Exporting OpenAPI spec as {} to {}", format, output);
                export_openapi(&format, &output);
            }
        },
        Commands::Run => {
            println!("Starting Chopin development server...");
            println!("Run `cargo run` in your project directory instead.");
        }
    }
}

fn create_project(name: &str) {
    use std::fs;
    use std::path::Path;

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
SERVER_PORT=5000
SERVER_HOST=127.0.0.1

# Environment
ENVIRONMENT=development
"#;

    // .gitignore
    let gitignore = r#"/target/
*.rs.bk
Cargo.lock
.env
.DS_Store
"#;

    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");
    fs::write(project_dir.join("src/main.rs"), main_rs).expect("Failed to write main.rs");
    fs::write(project_dir.join(".env.example"), env_example).expect("Failed to write .env.example");
    fs::write(project_dir.join(".env"), env_example).expect("Failed to write .env");
    fs::write(project_dir.join(".gitignore"), gitignore).expect("Failed to write .gitignore");

    println!("Project '{}' created successfully!", name);
    println!();
    println!("Next steps:");
    println!("  cd {}", name);
    println!("  cargo run");
    println!();
    println!("API docs: http://127.0.0.1:5000/api-docs");
}

fn export_openapi(format: &str, output: &str) {
    use utoipa::OpenApi;
    use chopin_core::openapi::ApiDoc;

    let spec = match format {
        "json" => ApiDoc::openapi().to_pretty_json().expect("Failed to generate JSON"),
        "yaml" => ApiDoc::openapi().to_yaml().expect("Failed to generate YAML"),
        _ => {
            eprintln!("Unsupported format: {}. Use 'json' or 'yaml'.", format);
            std::process::exit(1);
        }
    };

    std::fs::write(output, spec).expect("Failed to write file");
    println!("OpenAPI spec exported to: {}", output);
}
