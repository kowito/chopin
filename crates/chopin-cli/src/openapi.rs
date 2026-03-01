use anyhow::Result;
use colored::*;
use std::path::Path;
use walkdir::WalkDir;

/// Generates a lightweight OpenAPI 3.0 spec based on the file-system router.
/// This runs at compile-time/CLI-time and adds ZERO runtime overhead.
pub fn generate_openapi(project_dir: &Path) -> Result<()> {
    println!("{} Generating OpenAPI spec from routes...", "📜".bold());

    let apps_dir = project_dir.join("src/apps");
    if !apps_dir.exists() {
        anyhow::bail!("No src/apps directory found. Cannot find handlers.");
    }

    let mut paths = Vec::new();

    for entry in WalkDir::new(&apps_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "handlers.rs")
    {
        let content = std::fs::read_to_string(entry.path()).unwrap_or_default();

        for line in content.lines() {
            let line = line.trim();
            // Look for #[get("/path")] or #[post("/path")] etc.
            if line.starts_with("#[") && line.contains("(\"/") {
                // Extract method: get, post, put, delete, patch
                let method_start = 2; // after #[
                let method_end = line.find('(').unwrap_or(0);
                if method_end <= method_start {
                    continue;
                }
                let method = &line[method_start..method_end];

                // Extract path: "/path"
                let path_start = method_end + 2; // after ("
                let path_end = line.rfind('"').unwrap_or(0);
                if path_end <= path_start {
                    continue;
                }
                let http_path = line[path_start..path_end].to_string();

                // Convert /users/:id to /users/{id} for OpenAPI
                let mut openapi_path = String::new();
                let mut path_params = Vec::new();

                for segment in http_path.split('/') {
                    if segment.is_empty() {
                        continue;
                    }
                    if let Some(param) = segment.strip_prefix(':') {
                        openapi_path.push_str(&format!("/{{{}}}", param));
                        path_params.push(param.to_string());
                    } else {
                        openapi_path.push_str(&format!("/{}", segment));
                    }
                }

                if openapi_path.is_empty() {
                    openapi_path = "/".to_string();
                }

                paths.push((openapi_path, method.to_string(), path_params));
            }
        }
    }

    paths.sort();

    // Generate basic OpenAPI YAML
    let mut yaml = String::new();
    yaml.push_str("openapi: 3.0.0\n");
    yaml.push_str("info:\n");
    yaml.push_str("  title: Chopin API\n");
    yaml.push_str("  version: 1.0.0\n");
    yaml.push_str("  description: Auto-generated from file-based routing\n");
    yaml.push_str("paths:\n");

    // Group by path
    let mut grouped_paths: std::collections::BTreeMap<String, Vec<(String, Vec<String>)>> =
        std::collections::BTreeMap::new();
    for (path, method, params) in paths {
        grouped_paths
            .entry(path)
            .or_default()
            .push((method, params));
    }

    for (path, methods) in grouped_paths {
        yaml.push_str(&format!("  {}:\n", path));

        for (method, params) in methods {
            yaml.push_str(&format!("    {}:\n", method));
            yaml.push_str("      responses:\n");
            yaml.push_str("        '200':\n");
            yaml.push_str("          description: OK\n");

            if !params.is_empty() {
                yaml.push_str("      parameters:\n");
                for param in &params {
                    yaml.push_str(&format!("        - name: {}\n", param));
                    yaml.push_str("          in: path\n");
                    yaml.push_str("          required: true\n");
                    yaml.push_str("          schema:\n");
                    yaml.push_str("            type: string\n");
                }
            }
        }
    }

    let output_path = project_dir.join("openapi.yaml");
    std::fs::write(&output_path, yaml)?;

    println!("{} Saved to {}", "✓".green().bold(), "openapi.yaml".cyan());
    println!("  (Note: Scraped from routing macros in handlers.rs)");

    Ok(())
}
