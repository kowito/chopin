//! AWS RDS TLS connection example for `chopin-pg`.
//!
//! Demonstrates how to connect to an Amazon RDS PostgreSQL instance using
//! `sslmode=verify-full` and the official AWS CA bundle.
//!
//! # Quick start
//!
//! 1. Download the global AWS CA bundle **once**:
//!
//!    ```bash
//!    curl -o /tmp/aws-rds-global-bundle.pem \
//!        https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
//!    ```
//!
//! 2. Export connection parameters:
//!
//!    ```bash
//!    export PG_HOST=mydb.cluster-xxxxxx.us-east-1.rds.amazonaws.com
//!    export PG_PORT=5432            # optional, default 5432
//!    export PG_USER=myuser
//!    export PG_PASSWORD=secret
//!    export PG_DATABASE=mydb
//!    export PG_SSL_ROOT_CERT=/tmp/aws-rds-global-bundle.pem
//!    ```
//!
//! 3. Run:
//!
//!    ```bash
//!    cargo run -p chopin-pg --example aws_rds_tls --features tls
//!    ```
//!
//! # Security notes
//!
//! - `sslmode=verify-full` is used (the strictest mode): the server certificate
//!   is validated against the CA bundle **and** the hostname is verified against
//!   the certificate's CN/SAN.  Do not downgrade to `require` in production.
//! - The AWS CA bundle replaces the Mozilla WebPKI roots; no custom CA
//!   installation is required on the host.
//! - Credentials are read from environment variables — do not hard-code them.

fn main() {
    // ── Read configuration from environment variables ─────────────────────────
    let host = env_var("PG_HOST");
    let port: u16 = std::env::var("PG_PORT")
        .unwrap_or_else(|_| "5432".to_string())
        .parse()
        .expect("PG_PORT must be a valid port number (0–65535)");
    let user = env_var("PG_USER");
    let password = env_var("PG_PASSWORD");
    let database = env_var("PG_DATABASE");
    let ssl_root_cert = env_var("PG_SSL_ROOT_CERT");

    println!("Connecting to {host}:{port}/{database} as {user}");
    println!("TLS mode  : verify-full");
    println!("CA bundle : {ssl_root_cert}");
    println!();

    // ── Build PgConfig ────────────────────────────────────────────────────────
    use chopin_pg::{PgConfig, PgConnection, SslMode};

    let config = PgConfig::new(&host, port, &user, &password, &database)
        // verify-full = TLS + cert verification + hostname verification.
        // This is the mode recommended for production AWS RDS connections.
        .with_ssl_mode(SslMode::VerifyFull)
        // The AWS CA bundle replaces the default Mozilla WebPKI root store.
        // Download from:
        //   https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
        .with_ssl_root_cert(&ssl_root_cert)
        // Identify this connection in pg_stat_activity for observability.
        .with_application_name("chopin-aws-rds-example");

    // ── Connect ───────────────────────────────────────────────────────────────
    let mut conn = PgConnection::connect(&config).unwrap_or_else(|e| {
        eprintln!("Connection failed: {e}");
        eprintln!();
        eprintln!("Common causes:");
        eprintln!("  • PG_HOST is not reachable from this machine");
        eprintln!("  • The RDS security group does not allow inbound port {port}");
        eprintln!("  • PG_SSL_ROOT_CERT path is wrong or the file is empty");
        eprintln!("  • The hostname in PG_HOST does not match the TLS certificate");
        std::process::exit(1);
    });

    println!("Connected successfully via TLS.");
    println!();

    // ── Verify the connection with a lightweight round-trip ───────────────────
    let rows = conn
        .query(
            "SELECT version(), current_database(), current_user, inet_server_addr()::text",
            &[],
        )
        .expect("query failed");

    let version: String = rows[0].get_typed(0).unwrap();
    let db: String = rows[0].get_typed(1).unwrap();
    let user_name: String = rows[0].get_typed(2).unwrap();
    let server_addr: String = rows[0].get_typed(3).unwrap_or_else(|_| "N/A".to_string());

    println!("Server version : {version}");
    println!("Database       : {db}");
    println!("Connected as   : {user_name}");
    println!("Server address : {server_addr}");
    println!();

    // ── Confirm ssl_is_used() == true ─────────────────────────────────────────
    let rows = conn
        .query("SELECT ssl_is_used(), ssl_version()", &[])
        .expect("ssl_is_used() query failed");

    let ssl_used: bool = rows[0].get_typed(0).unwrap_or(false);
    let ssl_version: String = rows[0].get_typed(1).unwrap_or_else(|_| "N/A".to_string());

    if ssl_used {
        println!("TLS confirmed  : YES ({ssl_version})");
    } else {
        eprintln!("WARNING: ssl_is_used() returned false — the connection is NOT encrypted.");
        eprintln!("         Check your sslmode and CA bundle configuration.");
        std::process::exit(2);
    }

    println!();
    println!("All checks passed. AWS RDS TLS connection is working correctly.");
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        eprintln!("Required environment variable {name} is not set.");
        eprintln!("See the example file header for setup instructions.");
        std::process::exit(1);
    })
}
