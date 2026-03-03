use crate::{Executor, OrmResult};

/// Defines a single database migration mapping schema delta logic.
pub trait Migration {
    fn name(&self) -> &'static str;
    fn up(&self, executor: &mut dyn Executor) -> OrmResult<()>;
    fn down(&self, executor: &mut dyn Executor) -> OrmResult<()>;
}

/// Coordinates the execution and rollback of multiple `Migration` definitions sequentially.
pub struct MigrationManager;

impl MigrationManager {
    /// Creates the internal `__chopin_migrations` ledger if absent.
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

    /// Applies any un-executed definitions sequentially matching ledger states.
    pub fn up(executor: &mut dyn Executor, migrations: &[&dyn Migration]) -> OrmResult<()> {
        Self::ensure_migrations_table(executor)?;

        for m in migrations {
            let name = m.name();
            let check_sql = "SELECT 1 FROM __chopin_migrations WHERE name = $1";
            let rows = executor.query(check_sql, &[&name])?;

            if rows.is_empty() {
                println!("Applying migration: {}", name);
                m.up(executor)?;
                let insert_sql = "INSERT INTO __chopin_migrations (name) VALUES ($1)";
                executor.execute(insert_sql, &[&name])?;
                println!("Successfully applied: {}", name);
            }
        }
        Ok(())
    }

    /// Downgrades all supplied migrations sequentially matching ledger states (reverse order).
    pub fn down(executor: &mut dyn Executor, migrations: &[&dyn Migration]) -> OrmResult<()> {
        Self::ensure_migrations_table(executor)?;

        // Revert in reverse order
        for m in migrations.iter().rev() {
            let name = m.name();
            let check_sql = "SELECT 1 FROM __chopin_migrations WHERE name = $1";
            let rows = executor.query(check_sql, &[&name])?;

            if !rows.is_empty() {
                println!("Reverting migration: {}", name);
                m.down(executor)?;
                let delete_sql = "DELETE FROM __chopin_migrations WHERE name = $1";
                executor.execute(delete_sql, &[&name])?;
                println!("Successfully reverted: {}", name);
            }
        }
        Ok(())
    }
}

/// Declares an explicit database index attached to a given model struct.
pub struct Index {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub unique: bool,
}
