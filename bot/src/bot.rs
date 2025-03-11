use crate::{cli::Cli, handler::handler};
use anyhow::Result;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};
use tokio::task::JoinHandle;
use tracing::info;

pub async fn run() -> Result<()> {
    let (bot, context, storage) = Cli::init().await?;
    let mut dispatcher = Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![InMemStorage::<()>::new(), context, storage])
        .build();
    let shutdown = dispatcher.shutdown_token();
    let listener: JoinHandle<Result<()>> = tokio::spawn(async move {
        tokio::signal::ctrl_c().await?;
        shutdown.shutdown()?.await;
        info!(">> BOT: shutdown");
        Ok(())
    });
    dispatcher.dispatch().await;
    listener.await??;
    Ok(())
}
