use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;

use crate::config::Config;

/// Initialize the database connection from config.
pub async fn connect(config: &Config) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let mut opts = ConnectOptions::new(&config.database_url);
    opts.max_connections(100)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8))
        .sqlx_logging(config.is_dev());

    SeaDatabase::connect(opts).await
}
