use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    pretty_env_logger::env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    cli::run_cli(migration::Migrator).await;
}
