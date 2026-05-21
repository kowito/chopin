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

/// Scaffold a complete, wired-up CRUD resource — model, migration, services,
/// handlers, and errors — all with real ORM calls (no TODOs).
///
/// Usage: `chopin generate scaffold Post title:String body:String published:bool`
///
/// Generated layout:
/// ```text
/// src/apps/posts/
///   mod.rs        — public API re-exports
///   models.rs     — Post + CreatePost + UpdatePost structs
///   services.rs   — list / get / create / update / delete via chopin_pg::pool()
///   errors.rs     — PostError with #[derive(IntoResponse)]
///   handlers.rs   — 5 REST handlers wired to services
/// migrations/<ts>_create_posts/
///   up.sql / down.sql
/// ```
pub fn generate_scaffold(project_dir: &Path, name: &str, field_defs: &[String]) -> Result<()> {
    let struct_name = to_pascal_case(name);
    let snake_name = to_snake_case(name);
    let table_name = snake_name.clone() + "s";
    let route_base = format!("/{}", table_name);

    // ── Parse fields ────────────────────────────────────────────────────
    let mut fields: Vec<(String, &'static str, &'static str)> = Vec::new();
    for def in field_defs {
        let parts: Vec<&str> = def.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid field '{}'. Expected name:type", def);
        }
        let (rust_ty, sql_ty) = map_field_type(parts[1]);
        fields.push((parts[0].to_string(), rust_ty, sql_ty));
    }

    let app_dir = project_dir.join("src/apps").join(&snake_name);
    if app_dir.exists() {
        anyhow::bail!(
            "App '{}' already exists at {}",
            snake_name,
            app_dir.display()
        );
    }
    std::fs::create_dir_all(&app_dir)?;

    // ── mod.rs ──────────────────────────────────────────────────────────
    std::fs::write(
        app_dir.join("mod.rs"),
        "pub mod errors;\npub mod handlers;\npub mod models;\npub mod services;\n",
    )?;

    // ── models.rs ───────────────────────────────────────────────────────
    let field_decls: String = fields
        .iter()
        .map(|(n, t, _)| format!("    pub {n}: {t},\n"))
        .collect();

    let create_decls: String = fields
        .iter()
        .map(|(n, t, _)| format!("    pub {n}: {t},\n"))
        .collect();

    let update_decls: String = fields
        .iter()
        .map(|(n, t, _)| {
            // Wrap all fields in Option for partial updates
            let inner = if t.starts_with("Option<") {
                (*t).to_string()
            } else {
                format!("Option<{t}>")
            };
            format!("    pub {n}: {inner},\n")
        })
        .collect();

    let models_rs = format!(
        r#"use chopin_orm::Model;
use chopin_macros::IntoResponse;
use serde::{{Deserialize, Serialize}};

#[derive(Debug, Clone, Model, Serialize, Deserialize)]
#[model(table_name = "{table_name}")]
pub struct {struct_name} {{
    #[model(primary_key, generated)]
    pub id: i32,
{field_decls}}}

#[derive(Debug, Deserialize)]
pub struct Create{struct_name} {{
{create_decls}}}

#[derive(Debug, Deserialize)]
pub struct Update{struct_name} {{
{update_decls}}}
"#
    );
    std::fs::write(app_dir.join("models.rs"), &models_rs)?;

    // ── errors.rs ───────────────────────────────────────────────────────
    let errors_rs = format!(
        r#"use chopin_macros::IntoResponse;

#[derive(IntoResponse)]
pub enum {struct_name}Error {{
    #[status(404)]
    NotFound(i32),
    #[status(422)]
    Validation(String),
    #[status(500)]
    Db(chopin_orm::OrmError),
}}

impl From<chopin_orm::OrmError> for {struct_name}Error {{
    fn from(e: chopin_orm::OrmError) -> Self {{
        {struct_name}Error::Db(e)
    }}
}}
"#
    );
    std::fs::write(app_dir.join("errors.rs"), &errors_rs)?;

    // ── services.rs ─────────────────────────────────────────────────────
    // Build the set/field lines for create
    let create_fields: String = fields
        .iter()
        .map(|(n, _, _)| format!("    m.set(\"{n}\", body.{n}.into());\n"))
        .collect();

    // Build the set/field lines for update (only if Some)
    let update_fields: String = fields
        .iter()
        .map(|(n, _, _)| format!("    if let Some(v) = body.{n} {{ m.set(\"{n}\", v.into()); }}\n"))
        .collect();

    // Initial struct literal for create (id=0 for generated PK)
    let create_struct_fields: String = fields
        .iter()
        .map(|(n, t, _)| {
            let default = if t.starts_with("Option<") {
                "None".to_string()
            } else if *t == "bool" {
                "false".to_string()
            } else if *t == "String" {
                "body.".to_string() + n + ".clone()"
            } else {
                "body.".to_string() + n
            };
            format!("            {n}: {default},\n")
        })
        .collect();

    let services_rs = format!(
        r#"use chopin_orm::{{ActiveModel, Model}};
use super::errors::{struct_name}Error;
use super::models::{{{struct_name}, Create{struct_name}, Update{struct_name}}};

pub fn list() -> Result<Vec<{struct_name}>, {struct_name}Error> {{
    {struct_name}::find_all(chopin_pg::pool()).map_err({struct_name}Error::Db)
}}

pub fn get(id: i32) -> Result<{struct_name}, {struct_name}Error> {{
    {struct_name}::find_by_id(chopin_pg::pool(), id)?
        .ok_or({struct_name}Error::NotFound(id))
}}

pub fn create(body: Create{struct_name}) -> Result<{struct_name}, {struct_name}Error> {{
    let model = {struct_name} {{
        id: 0,
{create_struct_fields}    }};
    let mut m = ActiveModel::new_insert(model);
{create_fields}    m.save(chopin_pg::pool()).map_err({struct_name}Error::Db)
}}

pub fn update(id: i32, body: Update{struct_name}) -> Result<{struct_name}, {struct_name}Error> {{
    let existing = get(id)?;
    let mut m = ActiveModel::from_model(existing);
{update_fields}    m.save(chopin_pg::pool()).map_err({struct_name}Error::Db)
}}

pub fn delete(id: i32) -> Result<(), {struct_name}Error> {{
    {struct_name}::delete_by_id(chopin_pg::pool(), id)
        .map(|_| ())
        .map_err({struct_name}Error::Db)
}}
"#
    );
    std::fs::write(app_dir.join("services.rs"), &services_rs)?;

    // ── handlers.rs ─────────────────────────────────────────────────────
    let handlers_rs = format!(
        r#"use chopin_core::{{Context, Response}};
use chopin_core::extract::Json;
use chopin_macros::{{delete, get, post, put}};
use super::models::{{Create{struct_name}, Update{struct_name}}};
use super::services;

#[get("{route_base}")]
pub fn index(_ctx: Context) -> Response {{
    match services::list() {{
        Ok(items) => Response::json(&items),
        Err(e) => e.into(),
    }}
}}

#[get("{route_base}/:id")]
pub fn show(ctx: Context) -> Response {{
    let id: i32 = match ctx.param_parse("id") {{
        Ok(v) => v,
        Err(r) => return r,
    }};
    match services::get(id) {{
        Ok(item) => Response::json(&item),
        Err(e) => e.into(),
    }}
}}

#[post("{route_base}")]
pub fn create(ctx: Context) -> Response {{
    let Json(body) = match ctx.extract::<Json<Create{struct_name}>>() {{
        Ok(v) => v,
        Err(r) => return r,
    }};
    match services::create(body) {{
        Ok(item) => {{
            let mut r = Response::json(&item);
            r.status = 201;
            r
        }}
        Err(e) => e.into(),
    }}
}}

#[put("{route_base}/:id")]
pub fn update(ctx: Context) -> Response {{
    let id: i32 = match ctx.param_parse("id") {{
        Ok(v) => v,
        Err(r) => return r,
    }};
    let Json(body) = match ctx.extract::<Json<Update{struct_name}>>() {{
        Ok(v) => v,
        Err(r) => return r,
    }};
    match services::update(id, body) {{
        Ok(item) => Response::json(&item),
        Err(e) => e.into(),
    }}
}}

#[delete("{route_base}/:id")]
pub fn destroy(ctx: Context) -> Response {{
    let id: i32 = match ctx.param_parse("id") {{
        Ok(v) => v,
        Err(r) => return r,
    }};
    match services::delete(id) {{
        Ok(_) => Response::new(204),
        Err(e) => e.into(),
    }}
}}
"#
    );
    std::fs::write(app_dir.join("handlers.rs"), &handlers_rs)?;

    // ── migration ───────────────────────────────────────────────────────
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let migration_name = format!("{}_create_{}", timestamp, table_name);
    let migrations_dir = project_dir.join("migrations").join(&migration_name);
    std::fs::create_dir_all(&migrations_dir)?;

    let mut up_sql =
        format!("CREATE TABLE IF NOT EXISTS {table_name} (\n    id SERIAL PRIMARY KEY");
    for (fname, _, sql_ty) in &fields {
        up_sql.push_str(&format!(",\n    {fname} {sql_ty}"));
    }
    up_sql.push_str("\n);\n");
    std::fs::write(migrations_dir.join("up.sql"), &up_sql)?;
    std::fs::write(
        migrations_dir.join("down.sql"),
        format!("DROP TABLE IF EXISTS {table_name};\n"),
    )?;

    // ── summary ─────────────────────────────────────────────────────────
    println!(
        "{} Scaffold generated: {}",
        "✓".green().bold(),
        struct_name.cyan()
    );
    println!("  src/apps/{}/", snake_name);
    println!("    ├── mod.rs");
    println!("    ├── models.rs      ({struct_name}, Create{struct_name}, Update{struct_name})");
    println!("    ├── services.rs    (list / get / create / update / delete)");
    println!("    ├── errors.rs      ({struct_name}Error with #[derive(IntoResponse)])");
    println!("    └── handlers.rs    (GET/POST/PUT/DELETE — fully wired)");
    println!("  migrations/{migration_name}/up.sql");
    println!();
    println!("  Next steps:");
    println!(
        "    1. Add {} to your src/apps/mod.rs",
        format!("pub mod {snake_name};").yellow()
    );
    println!("    2. Run {}", "chopin migrate up".green());

    Ok(())
}

/// Scaffold a complete authentication module with User model, CRUD handlers,
/// business-logic services, domain errors, and a database migration.
///
/// Generated layout:
/// ```text
/// src/apps/auth/
///   mod.rs        — public API re-exports
///   models.rs     — User struct (id, email, password_hash, role, created_at)
///   handlers.rs   — POST /auth/register, POST /auth/login,
///                   POST /auth/logout,   POST /auth/refresh
///   services.rs   — register(), login(), issue_tokens()
///   errors.rs     — AuthError (InvalidCredentials, UserNotFound, …)
/// migrations/<ts>_create_users/
///   up.sql        — CREATE TABLE users …
///   down.sql      — DROP TABLE users
/// ```
pub fn generate_auth(project_dir: &Path) -> Result<()> {
    let auth_dir = project_dir.join("src/apps/auth");

    if auth_dir.exists() {
        anyhow::bail!("Auth module already exists at {}", auth_dir.display());
    }

    std::fs::create_dir_all(&auth_dir)?;

    // ─── mod.rs ───────────────────────────────────────────────────────────
    let mod_rs = r#"pub mod errors;
pub mod handlers;
pub mod models;
pub mod services;
"#;
    std::fs::write(auth_dir.join("mod.rs"), mod_rs)?;

    // ─── models.rs ────────────────────────────────────────────────────────
    let models_rs = r#"use serde::{Deserialize, Serialize};

/// Role enum for role-based access control.
///
/// Implement [`chopin_auth::Role`] so it works with
/// `StandardClaims<Role>` and `#[require_role]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

impl chopin_auth::Role for Role {}

/// Persisted user record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: Role,
    pub created_at: String,
}

/// Payload for `POST /auth/register`.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

/// Payload for `POST /auth/login`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Response returned after a successful login or token refresh.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
}
"#;
    std::fs::write(auth_dir.join("models.rs"), models_rs)?;

    // ─── errors.rs ────────────────────────────────────────────────────────
    let errors_rs = r#"use std::fmt;

/// Domain errors for the auth module.
#[derive(Debug)]
pub enum AuthError {
    /// Email is already registered.
    EmailTaken,
    /// No user found with that email.
    UserNotFound,
    /// Password does not match the stored hash.
    InvalidCredentials,
    /// JWT encoding / decoding failed.
    Token(String),
    /// Underlying database error.
    Database(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmailTaken => f.write_str("email already registered"),
            Self::UserNotFound => f.write_str("user not found"),
            Self::InvalidCredentials => f.write_str("invalid credentials"),
            Self::Token(e) => write!(f, "token error: {e}"),
            Self::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}
"#;
    std::fs::write(auth_dir.join("errors.rs"), errors_rs)?;

    // ─── services.rs ──────────────────────────────────────────────────────
    let services_rs = r#"//! Business logic for registration, login, and token issuance.
use chopin_auth::{PasswordHasher, StandardClaims};
use chopin_auth::extractor::GLOBAL_JWT_MANAGER;

use super::errors::AuthError;
use super::models::{LoginRequest, RegisterRequest, Role, TokenResponse, User};

/// Access token lifetime in seconds (1 hour).
pub const ACCESS_TTL: u64 = 3600;

/// Register a new user.
///
/// Hashes the password with Argon2 and inserts the record.
/// Returns the created [`User`] on success.
///
/// # Errors
/// - [`AuthError::EmailTaken`] if the email is already in use.
/// - [`AuthError::Database`] on persistence failure.
pub fn register(
    _db: &mut impl chopin_pg::connection::Execute,
    req: RegisterRequest,
) -> Result<User, AuthError> {
    let hash = PasswordHasher::interactive()
        .hash(req.password.as_bytes())
        .map_err(|e| AuthError::Database(e.to_string()))?;

    // TODO: INSERT INTO users (email, password_hash, role) VALUES ($1, $2, 'user')
    //       returning id, created_at — replace the placeholder below.
    let user = User {
        id: 1,
        email: req.email,
        password_hash: hash,
        role: Role::User,
        created_at: "now".into(),
    };

    Ok(user)
}

/// Verify credentials and return a signed access token.
///
/// # Errors
/// - [`AuthError::UserNotFound`] if no account exists for that email.
/// - [`AuthError::InvalidCredentials`] if the password is wrong.
/// - [`AuthError::Token`] if JWT signing fails.
pub fn login(
    _db: &mut impl chopin_pg::connection::Execute,
    req: LoginRequest,
) -> Result<TokenResponse, AuthError> {
    // TODO: SELECT * FROM users WHERE email = $1 — replace the placeholder.
    let placeholder_hash = "$argon2id$placeholder";
    let email_clone = req.email.clone();

    let valid = chopin_auth::verify_password(req.password.as_bytes(), placeholder_hash)
        .unwrap_or(false);
    if !valid {
        return Err(AuthError::InvalidCredentials);
    }

    issue_tokens(email_clone, Role::User)
}

/// Issue a signed `StandardClaims<Role>` access token for the given subject.
pub fn issue_tokens(sub: String, role: Role) -> Result<TokenResponse, AuthError> {
    let claims = StandardClaims::new(sub, ACCESS_TTL, Some(role), None);
    let manager = GLOBAL_JWT_MANAGER
        .get()
        .ok_or_else(|| AuthError::Token("JwtManager not initialised".into()))?;
    let token = manager
        .encode(&claims)
        .map_err(|e| AuthError::Token(e.to_string()))?;

    Ok(TokenResponse {
        access_token: token,
        token_type: "Bearer",
        expires_in: ACCESS_TTL,
    })
}
"#;
    std::fs::write(auth_dir.join("services.rs"), services_rs)?;

    // ─── handlers.rs ──────────────────────────────────────────────────────
    let handlers_rs = r##"//! HTTP handlers for the auth module.
//!
//! Mount these via `Chopin::new().mount_all_routes()` — they are registered
//! automatically through the `#[post]` inventory macros.
use chopin_core::{Context, Json, Response};
use chopin_macros::post;

use super::models::{LoginRequest, RegisterRequest};
use super::services;

/// Register a new user.
///
/// `POST /auth/register`
///
/// Body: `{"email": "...", "password": "..."}`
#[post("/auth/register")]
pub fn register(ctx: Context) -> Response {
    let Ok(Json(req)) = ctx.extract::<Json<RegisterRequest>>() else {
        return Response::new(400);
    };
    // TODO: pass a real db connection — e.g. from a thread-local pool.
    match services::register(&mut todo!("db"), req) {
        Ok(user) => ctx.json(&user),
        Err(e) => {
            let body = format!(r#"{{"error":"{}"}}"#, e);
            Response::json(409, body)
        }
    }
}

/// Authenticate and return a JWT access token.
///
/// `POST /auth/login`
///
/// Body: `{"email": "...", "password": "..."}`
#[post("/auth/login")]
pub fn login(ctx: Context) -> Response {
    let Ok(Json(req)) = ctx.extract::<Json<LoginRequest>>() else {
        return Response::new(400);
    };
    match services::login(&mut todo!("db"), req) {
        Ok(token_resp) => ctx.json(&token_resp),
        Err(_) => Response::new(401),
    }
}

/// Invalidate the current token (add its JTI to the blacklist).
///
/// `POST /auth/logout`
///
/// Requires: `Authorization: Bearer <token>`
#[post("/auth/logout")]
pub fn logout(ctx: Context) -> Response {
    use chopin_auth::extractor::GLOBAL_JWT_MANAGER;
    use chopin_auth::HasJti;
    use chopin_auth::StandardClaims;

    type Claims = StandardClaims<()>;

    let token = (0..ctx.req.header_count as usize).find_map(|i| {
        let (k, v) = ctx.req.headers[i];
        if k.eq_ignore_ascii_case("Authorization") {
            v.strip_prefix("Bearer ")
        } else {
            None
        }
    });

    let Some(token) = token else {
        return Response::new(401);
    };

    let Some(manager) = GLOBAL_JWT_MANAGER.get() else {
        return Response::server_error();
    };

    if let Ok(claims) = manager.decode::<Claims>(token) {
        if let Some(jti) = claims.jti() {
            if let Some(bl) = manager.blacklist() {
                bl.revoke(jti.to_string(), Some(claims.exp));
            }
        }
    }

    Response::new(204)
}

/// Refresh the access token.
///
/// `POST /auth/refresh`
///
/// Requires: `Authorization: Bearer <token>`
#[post("/auth/refresh")]
pub fn refresh(ctx: Context) -> Response {
    use chopin_auth::extractor::GLOBAL_JWT_MANAGER;
    use chopin_auth::StandardClaims;

    use super::models::Role;

    type Claims = StandardClaims<Role>;

    let token = (0..ctx.req.header_count as usize).find_map(|i| {
        let (k, v) = ctx.req.headers[i];
        if k.eq_ignore_ascii_case("Authorization") {
            v.strip_prefix("Bearer ")
        } else {
            None
        }
    });

    let Some(token) = token else {
        return Response::new(401);
    };

    let Some(manager) = GLOBAL_JWT_MANAGER.get() else {
        return Response::server_error();
    };

    match manager.decode::<Claims>(token) {
        Ok(old) => match services::issue_tokens(old.sub, old.role.unwrap_or(Role::User)) {
            Ok(resp) => ctx.json(&resp),
            Err(_) => Response::server_error(),
        },
        Err(_) => Response::new(401),
    }
}
"##;
    std::fs::write(auth_dir.join("handlers.rs"), handlers_rs)?;

    // ─── Migration ────────────────────────────────────────────────────────
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let migration_name = format!("{}_create_users", timestamp);
    let migrations_dir = project_dir.join("migrations").join(&migration_name);
    std::fs::create_dir_all(&migrations_dir)?;

    let up_sql = r#"CREATE TABLE IF NOT EXISTS users (
    id            BIGSERIAL    PRIMARY KEY,
    email         TEXT         NOT NULL UNIQUE,
    password_hash TEXT         NOT NULL,
    role          TEXT         NOT NULL DEFAULT 'user',
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS users_email_idx ON users (email);
"#;
    std::fs::write(migrations_dir.join("up.sql"), up_sql)?;

    let down_sql = "DROP TABLE IF EXISTS users;\n";
    std::fs::write(migrations_dir.join("down.sql"), down_sql)?;

    // ─── Output ───────────────────────────────────────────────────────────
    println!("{} Generated auth module", "✓".green().bold());
    println!("  Created: src/apps/auth/");
    println!("    ├── mod.rs       (public API)");
    println!("    ├── models.rs    (User, Role, request/response types)");
    println!("    ├── handlers.rs  (register, login, logout, refresh)");
    println!("    ├── services.rs  (register, login, issue_tokens)");
    println!("    └── errors.rs    (AuthError)");
    println!("  Created: migrations/{}/up.sql", migration_name);
    println!("  Created: migrations/{}/down.sql", migration_name);
    println!();
    println!("  {} Add to your main.rs:", "Next:".cyan());
    println!(
        "    {}",
        "use chopin_auth::{JwtManager, TokenBlacklist, init_jwt_manager};".yellow()
    );
    println!("    {}", "let bl = TokenBlacklist::new();".yellow());
    println!(
        "    {}",
        "init_jwt_manager(JwtManager::new(b\"your-secret\").with_blacklist(bl));".yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── existing tests ────────────────────────────────────────────────────────

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

    // ── generate_auth tests ───────────────────────────────────────────────

    #[test]
    fn test_generate_auth_creates_all_files() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();

        let auth_dir = dir.path().join("src/apps/auth");
        assert!(auth_dir.join("mod.rs").exists(), "mod.rs missing");
        assert!(auth_dir.join("models.rs").exists(), "models.rs missing");
        assert!(auth_dir.join("handlers.rs").exists(), "handlers.rs missing");
        assert!(auth_dir.join("services.rs").exists(), "services.rs missing");
        assert!(auth_dir.join("errors.rs").exists(), "errors.rs missing");
    }

    #[test]
    fn test_generate_auth_creates_migration() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();

        let migrations_dir = dir.path().join("migrations");
        let entries: Vec<_> = std::fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one migration directory");

        let mig_dir = entries[0].path();
        assert!(mig_dir.join("up.sql").exists(), "up.sql missing");
        assert!(mig_dir.join("down.sql").exists(), "down.sql missing");
    }

    #[test]
    fn test_generate_auth_migration_sql_content() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();

        let migrations_dir = dir.path().join("migrations");
        let mig_dir = std::fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap()
            .path();

        let up = std::fs::read_to_string(mig_dir.join("up.sql")).unwrap();
        assert!(up.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(up.contains("email"));
        assert!(up.contains("password_hash"));
        assert!(up.contains("role"));

        let down = std::fs::read_to_string(mig_dir.join("down.sql")).unwrap();
        assert!(down.contains("DROP TABLE IF EXISTS users"));
    }

    #[test]
    fn test_generate_auth_models_contains_role_enum() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();

        let models = std::fs::read_to_string(dir.path().join("src/apps/auth/models.rs")).unwrap();
        assert!(models.contains("enum Role"));
        assert!(models.contains("Admin"));
        assert!(models.contains("User"));
        assert!(models.contains("struct User"));
        assert!(models.contains("password_hash"));
    }

    #[test]
    fn test_generate_auth_handlers_contain_all_routes() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();

        let handlers =
            std::fs::read_to_string(dir.path().join("src/apps/auth/handlers.rs")).unwrap();
        assert!(handlers.contains("/auth/register"));
        assert!(handlers.contains("/auth/login"));
        assert!(handlers.contains("/auth/logout"));
        assert!(handlers.contains("/auth/refresh"));
    }

    #[test]
    fn test_generate_auth_duplicate_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        generate_auth(dir.path()).unwrap();
        let result = generate_auth(dir.path());
        assert!(result.is_err(), "duplicate auth scaffold should fail");
    }
}
