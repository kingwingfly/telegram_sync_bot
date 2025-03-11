pub use sea_orm_migration::prelude::*;

mod m20250310_104146_create_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20250310_104146_create_tables::Migration)]
    }
}
