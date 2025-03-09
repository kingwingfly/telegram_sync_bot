use crate::{cli::Cli, handler::handler};
use anyhow::Result;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

pub async fn run() -> Result<()> {
    let (bot, context, storage) = Cli::init().await?;
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![InMemStorage::<()>::new(), context, storage])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
