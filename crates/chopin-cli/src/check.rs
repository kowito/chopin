use anyhow::Result;
use colored::*;
use std::path::Path;
use walkdir::WalkDir;

pub fn run_checks(project_dir: &Path) -> Result<()> {
    println!("{} Running architectural linter...", "🔍".bold());

    let mut has_errors = false;

    // Check 1: Handlers should never import models directly
    if let Err(e) = check_handlers_models_isolation(project_dir) {
        println!("{}", e);
        has_errors = true;
    }

    // Check 2: Apps should not have circular dependencies
    // (We'll do a simple check: apps shouldn't import each other's models directly,
    // they should go through services, or ideally not import other apps at all).
    if let Err(e) = check_apps_isolation(project_dir) {
        println!("{}", e);
        has_errors = true;
    }

    if has_errors {
        println!("\n{} Architectural violations found.", "❌".red().bold());
        std::process::exit(1);
    } else {
        println!("{} All architectural checks passed!", "✓".green().bold());
    }

    Ok(())
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
