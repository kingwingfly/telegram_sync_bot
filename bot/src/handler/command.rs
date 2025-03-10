use super::MyDialogue;
use crate::{context::Context, storage::MyStorage, utils::gen_key};
use anyhow::Result;
use log::info;
use teloxide::{
    Bot,
    dispatching::UpdateHandler,
    dptree::case,
    macros::BotCommands,
    prelude::Requester as _,
    types::{MediaKind, MediaText, Message, MessageCommon, MessageKind},
    utils::command::BotCommands as _,
};

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "This is a bot to sync files from chat.")]
    Start,
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "Dhow the current state.")]
    State,
    #[command(description = "Switch between paused and working state.")]
    Switch,
    #[command(description = "Toggle sync new files.")]
    TroggleSync,
    #[command(description = "Print current bypass key in the server side.")]
    BypassKey,
}

pub fn cmd_handler() -> UpdateHandler<anyhow::Error> {
    teloxide::filter_command::<Command, _>()
        .branch(
            case![Command::Start].endpoint(async |bot: Bot, msg: Message| {
                bot.send_message(
                    msg.chat.id,
                    "This is a bot to sync files from chat. Enter /help to see all commands.",
                )
                .await?;
                Ok(())
            }),
        )
        .branch(
            case![Command::Help].endpoint(async |bot: Bot, msg: Message| {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }),
        )
        .branch(case![Command::BypassKey].endpoint(async |ctx: Context| {
            info!("BypassKey: {}", ctx.bypasskey.read().unwrap());
            Ok(())
        }))
        .branch(
            case![Command::State].endpoint(async |bot: Bot, msg: Message, db: MyStorage| {
                let working = db.get_chat_state(msg.chat.id).await;
                let syncing = db.get_syncing_state(msg.chat.id).await;
                bot.send_message(
                    msg.chat.id,
                    format!("working: {}; syncing: {}", working, syncing),
                )
                .await?;
                Ok(())
            }),
        )
        .branch(case![Command::Switch].endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message, ctx: Context, db: MyStorage| {
                if !auth(&bot, &dialogue, &msg, &ctx).await? {
                    return Ok(());
                }
                let working = db.switch_chat_state(msg.chat.id).await?;
                bot.send_message(msg.chat.id, format!("Working: {}", working))
                    .await?;
                Ok(())
            },
        ))
        .branch(case![Command::TroggleSync].endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message, ctx: Context, db: MyStorage| {
                if !db.get_chat_state(msg.chat.id).await {
                    bot.send_message(msg.chat.id, "Paused").await?;
                    dialogue.exit().await?;
                    return Ok(());
                }
                if !auth(&bot, &dialogue, &msg, &ctx).await? {
                    return Ok(());
                }
                let syncing = db.troggle_syncing(msg.chat.id).await?;
                bot.send_message(msg.chat.id, format!("Syncing: {}", syncing))
                    .await?;
                Ok(())
            },
        ))
}

async fn auth(bot: &Bot, dialogue: &MyDialogue, msg: &Message, ctx: &Context) -> Result<bool> {
    if msg
        .from
        .as_ref()
        .map(|user| ctx.bypass_users.contains(&user.id))
        != Some(true)
    {
        // check bypass_pwd
        match msg {
            Message {
                kind:
                    MessageKind::Common(MessageCommon {
                        media_kind: MediaKind::Text(MediaText { text, .. }),
                        ..
                    }),
                ..
            } if matches!(text.split_once(" "), Some((_, key)) if key == *ctx.bypasskey.read().unwrap()) =>
            {
                // renew bypass_pwd
                let new = gen_key();
                info!("New bypasskey: {}", new);
                *ctx.bypasskey.write().unwrap() = new;
                Ok(true)
            }
            _ => {
                bot.send_message(
                    msg.chat.id,
                    "Permission denied: You are not in allow users list or invalid password",
                )
                .await?;
                dialogue.exit().await?;
                Ok(false)
            }
        }
    } else {
        Ok(true)
    }
}
