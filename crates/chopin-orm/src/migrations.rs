use crate::{Executor, OrmResult};

/// Defines a single database migration with forward and reverse operations.
pub trait Migration {
    /// A unique name identifying this migration (e.g., "001_create_users").
    fn name(&self) -> &'static str;
    /// Apply this migration.
    fn up(&self, executor: &mut dyn Executor) -> OrmResult<()>;
    /// Revert this migration.
    fn down(&self, executor: &mut dyn Executor) -> OrmResult<()>;
}

/// Represents the status of a single migration.
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub name: String,
    pub applied: bool,
}

/// Coordinates the execution, rollback, and status reporting of database migrations.
pub struct MigrationManager;

impl MigrationManager {
    /// Creates the internal `__chopin_migrations` ledger table if it does not exist.
    pub fn ensure_migrations_table(executor: &mut dyn Executor) -> OrmResult<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS __chopin_migrations (
                name TEXT PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        "#;
        executor.execute(sql, &[])?;
        Ok(())
    }

    /// Returns the status of each migration (applied or pending).
    pub fn status(
        executor: &mut dyn Executor,
        migrations: &[&dyn Migration],
    ) -> OrmResult<Vec<MigrationStatus>> {
        Self::ensure_migrations_table(executor)?;

        let mut statuses = Vec::with_capacity(migrations.len());
        for m in migrations {
            let name = m.name();
            let check_sql = "SELECT 1 FROM __chopin_migrations WHERE name = $1";
            let rows = executor.query(check_sql, &[&name])?;
            statuses.push(MigrationStatus {
                name: name.to_string(),
                applied: !rows.is_empty(),
            });
        }
        Ok(statuses)
    }

    /// Applies all pending migrations in order.
    pub fn up(executor: &mut dyn Executor, migrations: &[&dyn Migration]) -> OrmResult<()> {
        Self::ensure_migrations_table(executor)?;

        for m in migrations {
            let name = m.name();
            let check_sql = "SELECT 1 FROM __chopin_migrations WHERE name = $1";
            let rows = executor.query(check_sql, &[&name])?;

            if rows.is_empty() {
                #[cfg(feature = "log")]
                log::info!("Applying migration: {}", name);
                m.up(executor)?;
                let insert_sql = "INSERT INTO __chopin_migrations (name) VALUES ($1)";
                executor.execute(insert_sql, &[&name])?;
                #[cfg(feature = "log")]
                log::info!("Successfully applied: {}", name);
            }
        }
        Ok(())
    }

    /// Reverts all applied migrations in reverse order.
    pub fn down(executor: &mut dyn Executor, migrations: &[&dyn Migration]) -> OrmResult<()> {
        Self::ensure_migrations_table(executor)?;

        for m in migrations.iter().rev() {
            let name = m.name();
            let check_sql = "SELECT 1 FROM __chopin_migrations WHERE name = $1";
            let rows = executor.query(check_sql, &[&name])?;

            if !rows.is_empty() {
                #[cfg(feature = "log")]
                log::info!("Reverting migration: {}", name);
                m.down(executor)?;
                let delete_sql = "DELETE FROM __chopin_migrations WHERE name = $1";
                executor.execute(delete_sql, &[&name])?;
                #[cfg(feature = "log")]
                log::info!("Successfully reverted: {}", name);
            }
        }
        Ok(())
    }
}

/// Declares a database index to be created during schema sync or migrations.
pub struct Index {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub unique: bool,
}
