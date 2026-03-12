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

/// Convert PascalCase to snake_case for table names.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Map a shorthand type name to (Rust type, SQL type).
fn map_field_type(t: &str) -> (&'static str, &'static str) {
    match t {
        "string" | "String" | "text" => ("String", "TEXT NOT NULL"),
        "i32" | "int" | "integer" => ("i32", "INTEGER NOT NULL"),
        "i64" | "bigint" => ("i64", "BIGINT NOT NULL"),
        "f32" | "float" => ("f32", "REAL NOT NULL"),
        "f64" | "double" => ("f64", "DOUBLE PRECISION NOT NULL"),
        "bool" | "boolean" => ("bool", "BOOLEAN NOT NULL DEFAULT false"),
        "string?" | "text?" => ("Option<String>", "TEXT"),
        "i32?" | "int?" => ("Option<i32>", "INTEGER"),
        "i64?" | "bigint?" => ("Option<i64>", "BIGINT"),
        "bool?" | "boolean?" => ("Option<bool>", "BOOLEAN"),
        _ => ("String", "TEXT NOT NULL"), // fallback
    }
}

/// Generate a model struct + up/down migrations from field definitions.
///
/// Usage: `chopin generate model User name:string email:string age:i32`
pub fn generate_model(project_dir: &Path, name: &str, field_defs: &[String]) -> Result<()> {
    let struct_name = to_pascal_case(name);
    let table_name = to_snake_case(name) + "s"; // simple pluralization

    // Parse field definitions.
    let mut fields: Vec<(&str, &'static str, &'static str)> = Vec::new(); // (name, rust_type, sql_type)
    for def in field_defs {
        let parts: Vec<&str> = def.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid field definition '{}'. Expected format: name:type",
                def
            );
        }
        let (rust_ty, sql_ty) = map_field_type(parts[1]);
        fields.push((parts[0], rust_ty, sql_ty));
    }

    // ─── Generate model struct ───────────────────────────────────────────
    let mut model_code = format!(
        r#"use chopin_orm::Model;
use serde::{{Deserialize, Serialize}};

#[derive(Debug, Clone, Model, Serialize, Deserialize)]
#[model(table_name = "{}")]
pub struct {} {{
    #[model(primary_key)]
    pub id: i32,
"#,
        table_name, struct_name
    );

    for (fname, rust_ty, _) in &fields {
        model_code.push_str(&format!("    pub {}: {},\n", fname, rust_ty));
    }
    model_code.push_str("}\n");

    let models_path = project_dir.join(format!("src/models/{}.rs", to_snake_case(name)));
    std::fs::create_dir_all(models_path.parent().unwrap())?;
    std::fs::write(&models_path, &model_code)?;

    // ─── Generate migration ──────────────────────────────────────────────
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let migration_name = format!("{}_{}", timestamp, to_snake_case(name));

    let migrations_dir = project_dir.join("migrations").join(&migration_name);
    std::fs::create_dir_all(&migrations_dir)?;

    // up.sql
    let mut up_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n    id SERIAL PRIMARY KEY",
        table_name
    );
    for (fname, _, sql_ty) in &fields {
        up_sql.push_str(&format!(",\n    {} {}", fname, sql_ty));
    }
    up_sql.push_str("\n);\n");
    std::fs::write(migrations_dir.join("up.sql"), &up_sql)?;

    // down.sql
    let down_sql = format!("DROP TABLE IF EXISTS {};\n", table_name);
    std::fs::write(migrations_dir.join("down.sql"), &down_sql)?;

    println!(
        "{} Generated model: {}",
        "✓".green().bold(),
        struct_name.cyan()
    );
    println!("  Created: {}", models_path.display());
    println!("  Created: migrations/{}/up.sql", migration_name);
    println!("  Created: migrations/{}/down.sql", migration_name);

    Ok(())
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

    #[test]
    fn test_to_pascal_case_empty_string() {
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn test_to_pascal_case_single_char() {
        assert_eq!(to_pascal_case("a"), "A");
        assert_eq!(to_pascal_case("z"), "Z");
    }

    #[test]
    fn test_to_pascal_case_multiple_underscores() {
        assert_eq!(to_pascal_case("order_line_item"), "OrderLineItem");
        assert_eq!(to_pascal_case("a_b_c_d"), "ABCD");
    }

    #[test]
    fn test_to_pascal_case_trailing_underscore() {
        // trailing underscore produces an empty last word (empty string in collect)
        let result = to_pascal_case("user_");
        // "user_" splits into ["user", ""] so result is "User" + "" = "User"
        assert_eq!(result, "User");
    }

    #[test]
    fn test_to_pascal_case_with_numbers() {
        assert_eq!(to_pascal_case("order_123"), "Order123");
        assert_eq!(to_pascal_case("v2_api"), "V2Api");
    }

    #[test]
    fn test_generate_app_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        generate_app(dir.path(), "widget").unwrap();
        let app_dir = dir.path().join("src/apps/widget");
        assert!(app_dir.join("mod.rs").exists(), "mod.rs missing");
        assert!(app_dir.join("models.rs").exists(), "models.rs missing");
        assert!(app_dir.join("services.rs").exists(), "services.rs missing");
        assert!(app_dir.join("errors.rs").exists(), "errors.rs missing");
        assert!(app_dir.join("handlers.rs").exists(), "handlers.rs missing");
        assert!(app_dir.join("tests.rs").exists(), "tests.rs missing");
    }

    #[test]
    fn test_generate_app_model_contains_pascal_name() {
        let dir = tempfile::tempdir().unwrap();
        generate_app(dir.path(), "order_item").unwrap();
        let models =
            std::fs::read_to_string(dir.path().join("src/apps/order_item/models.rs")).unwrap();
        assert!(
            models.contains("OrderItem"),
            "model struct should be PascalCase"
        );
    }

    #[test]
    fn test_generate_app_duplicate_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        generate_app(dir.path(), "product").unwrap();
        let result = generate_app(dir.path(), "product");
        assert!(result.is_err(), "duplicate app should fail");
    }
}
