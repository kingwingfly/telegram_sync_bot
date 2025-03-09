use crate::{cli::Cli, handler::handler};
use anyhow::Result;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

#[derive(Clone, Default, Debug)]
pub enum State {
    #[cfg_attr(not(debug_assertions), default)]
    Paused,
    #[cfg_attr(debug_assertions, default)]
    Working,
}

pub async fn run() -> Result<()> {
    let (bot, context) = Cli::init()?;
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![InMemStorage::<State>::new(), context])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
