use crate::{cli::Cli, handler::handler};
use anyhow::{Context as _, Result};
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

#[derive(Clone, Default, Debug)]
pub enum State {
    #[default]
    Paused,
    Working,
}

pub async fn run() -> Result<()> {
    let (bot, context) = Cli::init()?;
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            context,
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
