pub use sea_orm_migration::prelude::*;

mod m20250211_000002_create_posts_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250211_000002_create_posts_table::Migration),
        ]
    }
}
