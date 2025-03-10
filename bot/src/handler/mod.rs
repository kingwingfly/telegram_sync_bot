mod channel;
mod command;
mod message;
mod reaction;

use channel::channel_post_handler;
use message::msg_handler;
use reaction::{reaction_count_handler, reaction_handler};
use teloxide::{
    dispatching::{
        UpdateHandler,
        dialogue::{self, InMemStorage},
    },
    prelude::Dialogue,
    types::Update,
};

pub(super) type MyDialogue = Dialogue<(), InMemStorage<()>>;

pub fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<()>, (), _>()
        .branch(msg_handler())
        .branch(channel_post_handler())
        .branch(reaction_handler())
        .branch(reaction_count_handler())
}
