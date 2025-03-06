use crate::{handler::handler, storage::output_dir};
use anyhow::{Context, Result};
use log::info;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

#[derive(Clone, Default, Debug)]
pub enum State {
    #[default]
    Paused,
    Working,
}

fn init() -> Result<()> {
    pretty_env_logger::env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("sled", log::LevelFilter::Info)
        .init();
    info!("Initializing..");
    dotenv::dotenv().ok();
    info!(
        "TELOXIDE_TOKEN: {}",
        std::env::var("TELOXIDE_TOKEN").context("TELOXIDE_TOKEN unset")?
    );
    info!(
        "OWNER_ID: {}",
        std::env::var("OWNER_ID").context("OWNER_ID unset")?
    );
    info!("Finished Initializing..");
    Ok(())
}

pub async fn run() -> Result<()> {
    init()?;

    let bot = Bot::from_env();
    #[cfg(feature = "local_server")]
    let bot = bot.set_api_url("http://localhost:8081".parse().unwrap());
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            sled::open(format!("{}/data/db.sled", output_dir())).context("Failed to open db")?,
            UserId(
                std::env::var("OWNER_ID")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .context("INVALID OWNER_ID")?
            )
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
