use anyhow::Result;
use chopin_pg::{PgConfig, PgPool};
use chrono::Local;
use colored::*;
use std::fs;
use std::path::Path;

pub fn run_migration_command(project_dir: &Path, command: crate::MigrateCommands) -> Result<()> {
    let cfg = crate::config::ChopinConfig::load(project_dir)?;
    let db_url = &cfg.database.url;
    let mut pool = PgPool::connect(PgConfig::from_url(db_url)?, 1)?;

    match command {
        crate::MigrateCommands::Status => show_status(project_dir, &mut pool),
        crate::MigrateCommands::Up => run_up(project_dir, &mut pool),
        crate::MigrateCommands::Down { steps } => run_down(project_dir, &mut pool, steps),
        crate::MigrateCommands::Generate { name } => generate_migration(project_dir, &name),
    }
}

fn ensure_migration_table(pool: &mut PgPool) -> Result<()> {
    let mut conn = pool.get()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chopin_orm_migrations (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        &[],
    )?;
    Ok(())
}

fn get_applied_migrations(pool: &mut PgPool) -> Result<Vec<String>> {
    let mut conn = pool.get()?;
    let rows = conn.query("SELECT name FROM chopin_orm_migrations ORDER BY id ASC", &[])?;
    let mut names = Vec::new();
    for row in rows {
        let val = row.get(0)?;
        let name = match val {
            chopin_pg::PgValue::Text(s) => s,
            _ => return Err(anyhow::anyhow!("Expected text for migration name")),
        };
        names.push(name);
    }
    Ok(names)
}

fn show_status(project_dir: &Path, pool: &mut PgPool) -> Result<()> {
    ensure_migration_table(pool)?;
    let applied = get_applied_migrations(pool)?;
    let migrations_dir = project_dir.join("migrations");
    
    if !migrations_dir.exists() {
        println!("{} No migrations directory found.", "ℹ".blue());
        return Ok(());
    }

    println!("{} Migration Status:", "📊".bold());
    let mut files: Vec<_> = fs::read_dir(migrations_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "sql") && p.file_name().unwrap().to_str().unwrap().contains(".up"))
        .collect();
    files.sort();

    for file in files {
        let full_name = file.file_stem().unwrap().to_str().unwrap().replace(".up", "");
        let status = if applied.contains(&full_name) {
            "Applied".green()
        } else {
            "Pending".yellow()
        };
        println!("  - {:<40} [{}]", full_name, status);
    }

    Ok(())
}

fn run_up(project_dir: &Path, pool: &mut PgPool) -> Result<()> {
    ensure_migration_table(pool)?;
    let applied = get_applied_migrations(pool)?;
    let migrations_dir = project_dir.join("migrations");

    if !migrations_dir.exists() {
        return Err(anyhow::anyhow!("Migrations directory not found."));
    }

    let mut files: Vec<_> = fs::read_dir(migrations_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "sql") && p.file_name().unwrap().to_str().unwrap().contains(".up"))
        .collect();
    files.sort();

    let mut count = 0;
    for file in files {
        let full_name = file.file_stem().unwrap().to_str().unwrap().replace(".up", "");
        if !applied.contains(&full_name) {
            println!("{} Applying migration: {}", "↑".green(), full_name);
            let sql = fs::read_to_string(&file)?;
            let mut conn = pool.get()?;
            
            // Execute in transaction
            conn.execute("BEGIN", &[])?;
            match conn.execute(&sql, &[]) {
                Ok(_) => {
                    conn.execute("INSERT INTO chopin_orm_migrations (name) VALUES ($1)", &[&full_name])?;
                    conn.execute("COMMIT", &[])?;
                    count += 1;
                }
                Err(e) => {
                    conn.execute("ROLLBACK", &[])?;
                    return Err(anyhow::anyhow!("Failed to apply migration {}: {}", full_name, e));
                }
            }
        }
    }

    if count == 0 {
        println!("{} No pending migrations.", "✓".green());
    } else {
        println!("{} Successfully applied {} migrations.", "✓".green(), count);
    }

    Ok(())
}

fn run_down(project_dir: &Path, pool: &mut PgPool, steps: u32) -> Result<()> {
    ensure_migration_table(pool)?;
    let applied = get_applied_migrations(pool)?;
    let migrations_dir = project_dir.join("migrations");

    if applied.is_empty() {
        println!("{} No migrations to rollback.", "ℹ".blue());
        return Ok(());
    }

    let to_rollback = applied.into_iter().rev().take(steps as usize);
    let mut count = 0;

    for name in to_rollback {
        let down_file = migrations_dir.join(format!("{}.down.sql", name));
        if !down_file.exists() {
            return Err(anyhow::anyhow!("Down migration file not found for {}", name));
        }

        println!("{} Rolling back migration: {}", "↓".red(), name);
        let sql = fs::read_to_string(&down_file)?;
        let mut conn = pool.get()?;

        conn.execute("BEGIN", &[])?;
        match conn.execute(&sql, &[]) {
            Ok(_) => {
                conn.execute("DELETE FROM chopin_orm_migrations WHERE name = $1", &[&name])?;
                conn.execute("COMMIT", &[])?;
                count += 1;
            }
            Err(e) => {
                conn.execute("ROLLBACK", &[])?;
                return Err(anyhow::anyhow!("Failed to rollback migration {}: {}", name, e));
            }
        }
    }

    println!("{} Successfully rolled back {} migrations.", "✓".green(), count);
    Ok(())
}

fn generate_migration(project_dir: &Path, name: &str) -> Result<()> {
    let migrations_dir = project_dir.join("migrations");
    if !migrations_dir.exists() {
        fs::create_dir_all(&migrations_dir)?;
    }

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let base_name = format!("{}_{}", timestamp, name);
    
    let up_file = migrations_dir.join(format!("{}.up.sql", base_name));
    let down_file = migrations_dir.join(format!("{}.down.sql", base_name));

    fs::write(&up_file, "-- Write your UP migration here\n")?;
    fs::write(&down_file, "-- Write your DOWN migration here\n")?;

    println!("{} Generated migration files:", "✨".bold());
    println!("  - {}", up_file.display());
    println!("  - {}", down_file.display());

    Ok(())
}
