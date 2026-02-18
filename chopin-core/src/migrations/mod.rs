pub use sea_orm_migration::prelude::*;

mod m20250211_000001_create_users_table;
mod m20250215_000001_add_security_tables;
mod m20250218_000001_add_rbac_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250211_000001_create_users_table::Migration),
            Box::new(m20250215_000001_add_security_tables::Migration),
            Box::new(m20250218_000001_add_rbac_tables::Migration),
        ]
    }
}
