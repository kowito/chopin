pub use sea_orm_migration::prelude::*;

pub mod m20250218_000001_initial_schema;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20250218_000001_initial_schema::Migration)]
    }
}
