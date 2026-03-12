use anyhow::Result;
use colored::*;
use std::path::Path;
use walkdir::WalkDir;

pub fn run_checks(project_dir: &Path) -> Result<()> {
    println!("{} Running Chopin project checks...\n", "🔍".bold());

    let mut pass = 0u32;
    let mut fail = 0u32;

    // ─── Check 1: Config / env vars ──────────────────────────────────────
    print!("  Config & env vars ... ");
    match check_config(project_dir) {
        Ok(msg) => {
            println!("{} {}", "✓".green().bold(), msg);
            pass += 1;
        }
        Err(e) => {
            println!("{} {}", "✗".red().bold(), e);
            fail += 1;
        }
    }

    // ─── Check 2: Database connectivity ──────────────────────────────────
    print!("  Database connection . ");
    match check_database(project_dir) {
        Ok(msg) => {
            println!("{} {}", "✓".green().bold(), msg);
            pass += 1;
        }
        Err(e) => {
            println!("{} {}", "✗".red().bold(), e);
            fail += 1;
        }
    }

    // ─── Check 3: Handler-model isolation ────────────────────────────────
    print!("  Handler isolation ... ");
    match check_handlers_models_isolation(project_dir) {
        Ok(()) => {
            println!("{}", "✓".green().bold());
            pass += 1;
        }
        Err(e) => {
            println!("{}\n{}", "✗".red().bold(), e);
            fail += 1;
        }
    }

    // ─── Check 4: App isolation ──────────────────────────────────────────
    print!("  App isolation ....... ");
    match check_apps_isolation(project_dir) {
        Ok(()) => {
            println!("{}", "✓".green().bold());
            pass += 1;
        }
        Err(e) => {
            println!("{}\n{}", "✗".red().bold(), e);
            fail += 1;
        }
    }

    // ─── Summary ─────────────────────────────────────────────────────────
    println!();
    if fail == 0 {
        println!(
            "{} All {} checks passed!",
            "✓".green().bold(),
            pass
        );
    } else {
        println!(
            "{} {} passed, {} failed.",
            "✗".red().bold(),
            pass,
            fail
        );
        std::process::exit(1);
    }

    Ok(())
}

/// Validate config and environment variables.
fn check_config(project_dir: &Path) -> Result<String, String> {
    // Try to load ChopinConfig.
    let config_path = project_dir.join("Chopin.toml");
    if !config_path.exists() {
        // Fall back to DATABASE_URL env var.
        return match std::env::var("DATABASE_URL") {
            Ok(_) => Ok("DATABASE_URL set (no Chopin.toml)".into()),
            Err(_) => Err("No Chopin.toml and DATABASE_URL not set".into()),
        };
    }
    // Config file exists — try to parse it.
    match crate::config::ChopinConfig::load(project_dir) {
        Ok(cfg) => {
            if cfg.database.url.is_empty() {
                Err("database.url is empty in Chopin.toml".into())
            } else {
                Ok(format!("Chopin.toml loaded (db: {})", mask_url(&cfg.database.url)))
            }
        }
        Err(e) => Err(format!("Failed to parse Chopin.toml: {}", e)),
    }
}

/// Try connecting to the database.
fn check_database(project_dir: &Path) -> Result<String, String> {
    let url = get_database_url(project_dir).map_err(|e| e.to_string())?;
    let config = chopin_pg::connection::PgConfig::from_url(&url)
        .map_err(|e| format!("invalid DATABASE_URL: {}", e))?;
    match chopin_pg::pool::PgPool::connect(config, 1) {
        Ok(mut pool) => {
            // Try a simple query.
            match pool.get() {
                Ok(mut conn) => {
                    match conn.execute("SELECT 1", &[]) {
                        Ok(_) => Ok(format!("connected to {}", mask_url(&url))),
                        Err(e) => Err(format!("query failed: {}", e)),
                    }
                }
                Err(e) => Err(format!("pool.get() failed: {}", e)),
            }
        }
        Err(e) => Err(format!("connection failed: {}", e)),
    }
}

/// Resolve the database URL from config or env.
fn get_database_url(project_dir: &Path) -> Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }
    let cfg = crate::config::ChopinConfig::load(project_dir)?;
    Ok(cfg.database.url)
}

/// Mask a database URL for safe display (hide password).
fn mask_url(url: &str) -> String {
    // postgres://user:password@host:port/db → postgres://user:***@host:port/db
    if let Some(at) = url.find('@') {
        if let Some(colon) = url[..at].rfind(':') {
            let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
            if colon > scheme_end {
                return format!("{}***{}", &url[..colon + 1], &url[at..]);
            }
        }
    }
    url.to_string()
}

fn check_handlers_models_isolation(project_dir: &Path) -> Result<(), String> {
    let apps_dir = project_dir.join("src/apps");
    if !apps_dir.exists() {
        return Ok(());
    }

    let mut violations = Vec::new();

    for entry in WalkDir::new(&apps_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "handlers.rs")
    {
        let content = std::fs::read_to_string(entry.path())
            .map_err(|e| format!("Failed to read {}: {}", entry.path().display(), e))?;

        // Simple grep for models imports
        for (i, line) in content.lines().enumerate() {
            if line.contains("::models::")
                || line.contains(" models::")
                || line.contains("crate::apps::") && line.contains("models")
            {
                let rel_path = entry
                    .path()
                    .strip_prefix(project_dir)
                    .unwrap_or(entry.path());
                violations.push(format!(
                    "  {} {}:{}\n    > {}",
                    "→".red(),
                    rel_path.display().to_string().yellow(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }

    if !violations.is_empty() {
        let mut err_msg = format!(
            "{} Handler layer bypassing Service layer!\n",
            "⚠ Error:".bold().red()
        );
        err_msg.push_str("  Handlers must NEVER touch models or database directly.\n  They must call a fn in `apps/*/services.rs`.\n\n");
        err_msg.push_str(&violations.join("\n"));
        return Err(err_msg);
    }

    Ok(())
}

fn check_apps_isolation(project_dir: &Path) -> Result<(), String> {
    let apps_dir = project_dir.join("src/apps");
    if !apps_dir.exists() {
        return Ok(());
    }

    let mut violations = Vec::new();

    // Collect all app names
    let mut apps = Vec::new();
    for entry in std::fs::read_dir(&apps_dir).map_err(|e| e.to_string())? {
        if let Ok(entry) = entry
            && entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
        {
            apps.push(entry.file_name().to_string_lossy().to_string());
        }
    }

    for app in &apps {
        let app_dir = apps_dir.join(app);

        for entry in WalkDir::new(&app_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "rs")
            })
        {
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();

            for (i, line) in content.lines().enumerate() {
                for other_app in &apps {
                    if other_app == app {
                        continue;
                    }

                    // e.g. use crate::apps::users::... inside src/apps/todos/...
                    let import_pattern = format!("crate::apps::{}", other_app);
                    if line.contains(&import_pattern) {
                        let rel_path = entry
                            .path()
                            .strip_prefix(project_dir)
                            .unwrap_or(entry.path());
                        violations.push(format!(
                            "  {} {} depends on cross-app {}\n    {}:{} > {}",
                            "→".red(),
                            app.cyan(),
                            other_app.cyan(),
                            rel_path.display(),
                            i + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }

    if !violations.is_empty() {
        let mut err_msg = format!("{} App Isolation Violation!\n", "⚠ Error:".bold().red());
        err_msg.push_str("  Apps should be completely independent modules.\n  They must not import other apps directly. Use Events or dependency injection.\n\n");
        err_msg.push_str(&violations.join("\n"));
        return Err(err_msg);
    }

    Ok(())
}
