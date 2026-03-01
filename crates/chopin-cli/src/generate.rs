use anyhow::Result;
use colored::*;
use std::path::Path;

/// Scaffold a new Chopin "App" module.
pub fn generate_app(project_dir: &Path, name: &str) -> Result<()> {
    let app_dir = project_dir.join("src/apps").join(name);

    if app_dir.exists() {
        anyhow::bail!("App '{}' already exists at {}", name, app_dir.display());
    }

    std::fs::create_dir_all(&app_dir)?;

    // mod.rs — Public API
    let mod_rs = r#"pub mod errors;
pub mod models;
pub mod services;
pub mod handlers;
"#;
    std::fs::write(app_dir.join("mod.rs"), mod_rs)?;

    // models.rs
    let models = format!(
        r#"use serde::{{Deserialize, Serialize}};

/// {name} data model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {type_name} {{
    pub id: u64,
    // Add your fields here
}}
"#,
        name = name,
        type_name = to_pascal_case(name)
    );
    std::fs::write(app_dir.join("models.rs"), models)?;

    // services.rs
    let services = format!(
        r#"use super::errors::{type_name}Error;
use super::models::{type_name};

/// List all {name}s.
pub async fn list() -> Result<Vec<{type_name}>, {type_name}Error> {{
    // TODO: implement database query
    Ok(vec![])
}}

/// Get a single {name} by ID.
pub async fn get_by_id(id: u64) -> Result<{type_name}, {type_name}Error> {{
    // TODO: implement database query
    Err({type_name}Error::NotFound(id))
}}
"#,
        name = name,
        type_name = to_pascal_case(name)
    );
    std::fs::write(app_dir.join("services.rs"), services)?;

    // errors.rs
    let errors = format!(
        r#"use thiserror::Error;

#[derive(Error, Debug)]
pub enum {type_name}Error {{
    #[error("{type_name} not found: {{0}}")]
    NotFound(u64),
    #[error("Database error")]
    Db(#[from] chopin_pg::PgError),
}}
"#,
        type_name = to_pascal_case(name)
    );
    std::fs::write(app_dir.join("errors.rs"), errors)?;

    // handlers.rs
    let handlers = format!(
        r#"use chopin_core::{{Context, Response}};
use chopin_macros::{{get, post}};
use super::services;

#[get("/{name}")]
pub fn list(_ctx: Context) -> Response {{
    // TODO: call services::list() and return json
    Response::text("list {name}")
}}

#[get("/{name}/:id")]
pub fn get_by_id(ctx: Context) -> Response {{
    let _id = ctx.param("id").unwrap_or("0");
    // TODO: call services::get_by_id(_id)
    Response::text("get {name}")
}}

#[post("/{name}")]
pub fn create(_ctx: Context) -> Response {{
    // TODO: parse body with ctx.extract::<Json<...>>(), call services::create()
    Response::text("create {name}")
}}
"#,
        name = name
    );
    std::fs::write(app_dir.join("handlers.rs"), handlers)?;

    // tests.rs
    let tests = format!(
        r#"#[cfg(test)]
mod tests {{
    use super::services;

    #[tokio::test]
    async fn test_{name}_not_found() {{
        let result = services::get_by_id(999).await;
        assert!(result.is_err());
    }}
}}
"#,
        name = name
    );
    std::fs::write(app_dir.join("tests.rs"), tests)?;

    println!("{} Generated app: {}", "✓".green().bold(), name.cyan());
    println!("  Created: src/apps/{}/", name);
    println!("    ├── mod.rs       (public API + router)");
    println!("    ├── models.rs    (data structs)");
    println!("    ├── services.rs  (business logic)");
    println!("    ├── errors.rs    (domain errors)");
    println!("    ├── handlers.rs  (HTTP handlers)");
    println!("    └── tests.rs     (unit tests)");
    println!();
    println!(
        "  Next: Routes are automatically mounted via {}.",
        "Chopin::new().mount_all_routes()".yellow()
    );

    Ok(())
}

/// Scaffold a new handler function.
pub fn generate_handler(project_dir: &Path, app: &str, name: &str) -> Result<()> {
    let handlers_path = project_dir.join("src/apps").join(app).join("handlers.rs");

    if !handlers_path.exists() {
        anyhow::bail!("App '{}' does not exist or missing handlers.rs", app);
    }

    let handler_content = format!(
        r#"
#[get("/{app}/{name}")]
pub fn {name}(_ctx: Context) -> Response {{
    Response::text("Hello from {name}")
}}
"#
    );

    let mut content = std::fs::read_to_string(&handlers_path)?;
    content.push_str(&handler_content);
    std::fs::write(&handlers_path, content)?;

    println!(
        "{} Appended handler {} to {}",
        "✓".green().bold(),
        name.cyan(),
        handlers_path.display().to_string().cyan()
    );

    Ok(())
}

/// Convert snake_case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("todo"), "Todo");
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
        assert_eq!(to_pascal_case("billing"), "Billing");
    }
}
