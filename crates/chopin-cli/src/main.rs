use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;

mod check;
mod config;
mod deploy;
mod generate;
mod openapi;
mod migrations;

#[derive(Parser)]
#[command(name = "chopin")]
#[command(about = "🎹 Chopin: Ultra-low-latency HTTP Framework CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Chopin project
    New {
        /// Name of the project
        name: String,
    },
    /// Start development server
    Dev,
    /// Build for production
    Build,
    /// Database migrations
    Migrate {
        #[command(subcommand)]
        command: MigrateCommands,
    },
    /// Run benchmarks
    Bench,
    /// Database utilities
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// Generate scaffolding
    Generate {
        #[command(subcommand)]
        command: GenerateCommands,
    },
    /// Run architectural linter
    Check,
    /// Generate an optimized Dockerfile for deployment
    Deploy {
        /// Type of deployment to generate (e.g. docker)
        target: String,
    },
    /// Scrape the routes to generate an OpenAPI spec
    Openapi,
}

#[derive(Subcommand)]
enum GenerateCommands {
    /// Generate a new app module (models, services, errors, handlers)
    App {
        /// Name of the app (e.g., "todos", "auth")
        name: String,
    },
    /// Generate a new handler function
    Handler {
        /// Name of the app to add this handler to
        app: String,
        /// Name of the handler function
        name: String,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Open an interactive Postgres shell
    Shell,
    /// Dump database data to a file
    Dump {
        /// Output file path
        #[arg(short, long, default_value = "dump.sql")]
        file: String,
    },
    /// Restore database from a dump file
    Restore {
        /// Input file path
        #[arg(short, long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum MigrateCommands {
    /// Show migration status
    Status,
    /// Run pending migrations
    Up,
    /// Rollback migrations
    Down {
        #[arg(default_value_t = 1)]
        steps: u32,
    },
    /// Generate a new migration
    Generate { name: String },
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => {
            println!("{} Creating new project: {}", "🎹".bold(), name.green());
            create_project(&name)?;
            println!(
                "{} Project {} created successfully!",
                "✓".green().bold(),
                name
            );
            println!("\nNext steps:");
            println!("  cd {}", name);
            println!("  cargo run\n");
        }
        Commands::Dev => {
            println!(
                "{} Starting {} development server...",
                "🚀".bold(),
                "Chopin".cyan()
            );
            let mut child = std::process::Command::new("cargo").arg("run").spawn()?;
            child.wait()?;
        }
        Commands::Build => {
            println!(
                "{} Building for production (release profile)...",
                "🏗️".bold()
            );
            let mut child = std::process::Command::new("cargo")
                .arg("build")
                .arg("--release")
                .spawn()?;
            child.wait()?;
        }
        Commands::Migrate { command } => {
            let project_dir = std::env::current_dir()?;
            migrations::run_migration_command(&project_dir, command)?;
        }
        Commands::Db { command } => {
            let project_dir = std::env::current_dir()?;
            let cfg = config::ChopinConfig::load(&project_dir)?;
            let db_url = &cfg.database.url;
            match command {
                DbCommands::Shell => {
                    println!("{} Opening database shell...", "🐚".bold());
                    let mut cmd = std::process::Command::new("psql");
                    cmd.arg(db_url);
                    cmd.spawn()?.wait()?;
                }
                DbCommands::Dump { file } => {
                    println!("{} Dumping data to {}...", "💾".bold(), file.yellow());
                    let mut cmd = std::process::Command::new("pg_dump");
                    cmd.arg(db_url);
                    cmd.arg("-f").arg(&file);
                    cmd.spawn()?.wait()?;
                }
                DbCommands::Restore { file } => {
                    println!("{} Restoring data from {}...", "📥".bold(), file.yellow());
                    let mut cmd = std::process::Command::new("psql");
                    cmd.arg(db_url);
                    cmd.arg("-f").arg(&file);
                    cmd.spawn()?.wait()?;
                }
            }
        }
        Commands::Generate { command } => match command {
            GenerateCommands::App { name } => {
                let project_dir = std::env::current_dir()?;
                generate::generate_app(&project_dir, &name)?;
            }
            GenerateCommands::Handler { app, name } => {
                let project_dir = std::env::current_dir()?;
                generate::generate_handler(&project_dir, &app, &name)?;
            }
        },
        Commands::Check => {
            let project_dir = std::env::current_dir()?;
            check::run_checks(&project_dir)?;
        }
        Commands::Deploy { target } => {
            if target == "docker" {
                let project_dir = std::env::current_dir()?;
                deploy::generate_dockerfile(&project_dir)?;
            } else {
                println!(
                    "{} Unknown deploy target '{}'. Try: 'docker'",
                    "⚠".yellow(),
                    target
                );
            }
        }
        Commands::Openapi => {
            let project_dir = std::env::current_dir()?;
            openapi::generate_openapi(&project_dir)?;
        }
        Commands::Bench => {
            println!("{} Running benchmarks...", "🔥".bold());
        }
    }

    Ok(())
}

fn create_project(name: &str) -> Result<()> {
    let path = std::env::current_dir()?.join(name);
    if path.exists() {
        anyhow::bail!("Directory already exists: {}", name);
    }

    std::fs::create_dir_all(path.join("src/apps"))?;

    // Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"

[dependencies]
chopin-core = "0.5"
chopin-macros = "0.1"
serde = {{ version = "1.0", features = ["derive"] }}
"#,
        name
    );
    std::fs::write(path.join("Cargo.toml"), cargo_toml)?;

    // src/main.rs — uses macro-based route discovery
    let main_rs = r#"use chopin_core::Chopin;
mod apps;

fn main() {
    println!("🎹 Starting {} server on 0.0.0.0:8080", env!("CARGO_PKG_NAME"));

    Chopin::new()
        .mount_all_routes()
        .serve("0.0.0.0:8080")
        .unwrap();
}
"#;
    std::fs::write(path.join("src/main.rs"), main_rs)?;

    // src/apps/mod.rs placeholder
    std::fs::create_dir_all(path.join("src/apps"))?;
    std::fs::write(
        path.join("src/apps/mod.rs"),
        "// Add your app modules here, e.g.:\n// pub mod todos;\n",
    )?;

    Ok(())
}
