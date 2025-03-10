use super::MyDialogue;
use crate::{context::Context, storage::MyStorage};
use anyhow::{Context as _, Result};
use log::{debug, info};
use std::path::Path;
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    prelude::Requester as _,
    types::{ChatId, MessageId, ReactionCount, ReactionType, Update, UpdateKind},
};
use tokio::fs;

pub fn reaction_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, ctx: Context, db: MyStorage| {
            if !db.get_chat_state(dialogue.chat_id()).await {
                bot.send_message(dialogue.chat_id(), "Paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReaction(reaction) = update.kind {
                let (chat_id, msg_id) = (reaction.chat.id, reaction.message_id);
                if let Some(file_path) = db.get_path(reaction.chat.id, reaction.message_id).await? {
                    let file_path = Path::new(&file_path);
                    let file_name = file_path
                        .file_name()
                        .and_then(|file| file.to_str())
                        .context("Failed to read filename from db")?;

                    if let Some(ReactionType::Emoji { emoji }) = reaction.new_reaction.first() {
                        match emoji.as_str() {
                            "👍" | "❤" => {
                                let target_path =
                                    format!("{}/{}", ctx.fav_dir.display(), file_name);
                                fs::rename(file_path, &target_path).await?;
                                db.update_path(chat_id, msg_id, &target_path).await?;
                                info!("Fav: {}", target_path);
                                pin_msg(&bot, chat_id, msg_id).await?;
                            }
                            "👎" => {
                                let target_path =
                                    format!("{}/{}", ctx.trash_dir.display(), file_name);
                                bot.delete_message(chat_id, msg_id).await?;
                                info!("Deleted disliked message");
                                fs::rename(&file_path, &target_path).await?;
                                db.update_path(chat_id, msg_id, &target_path).await?;
                                info!("Delete: {}", file_path.display());
                                // do not need unpin deleted message
                            }
                            _ => {}
                        }
                    } else if let Some(ReactionType::Emoji { emoji }) =
                        reaction.old_reaction.first()
                    {
                        if matches!(emoji.as_str(), "👍" | "❤") {
                            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name);
                            fs::rename(file_path, &target_path).await?;
                            db.update_path(chat_id, msg_id, &target_path).await?;
                            info!("Unfav: {}", target_path);
                            unpin_msg(&bot, chat_id, msg_id).await?;
                        }
                    }
                } else {
                    debug!("Unable to handle react to user message maybe");
                    bot.delete_message(chat_id, msg_id).await?;
                    info!("Deleted out of control message");
                }
            }
            Ok(())
        },
    )
}

pub fn reaction_count_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_count_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, ctx: Context, db: MyStorage| {
            if !db.get_chat_state(dialogue.chat_id()).await {
                bot.send_message(dialogue.chat_id(), "Paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReactionCount(reaction) = update.kind {
                let (chat_id, msg_id) = (reaction.chat.id, reaction.message_id);
                if let Some(file_path) = db.get_path(reaction.chat.id, reaction.message_id).await? {
                    let file_path = Path::new(&file_path);
                    let file_name = file_path
                        .file_name()
                        .and_then(|file| file.to_str())
                        .context("Failed to read filename from db")?;

                    let score: i32 = reaction
                        .reactions
                        .iter()
                        .filter_map(|r| match r {
                            ReactionCount {
                                r#type: ReactionType::Emoji { emoji },
                                total_count,
                            } => match emoji.as_ref() {
                                "👍" | "😁" | "🙏" | "😇" | "🤗" => {
                                    Some(*total_count as i32)
                                }
                                "❤" | "🔥" | "🥰" | "🎉" | "🍌" | "💋" | "💘" | "😘" => {
                                    Some(2 * *total_count as i32)
                                }
                                "❤‍🔥" => Some(3 * *total_count as i32),
                                "👎" | "🤯" | "😱" | "😢" | "🥴" | "🌚" | "😐" | "🖕" | "😨" => {
                                    Some(-(*total_count as i32))
                                }
                                "🤬" | "🤮" | "💩" | "🤡" | "💔" | "😡" => {
                                    Some(-(2 * *total_count as i32))
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                        .sum();

                    if score >= ctx.fav_score_limit {
                        let target_path = format!("{}/{}", ctx.fav_dir.display(), file_name);
                        fs::rename(file_path, &target_path).await?;
                        db.update_path(chat_id, msg_id, &target_path).await?;
                        info!("Fav: {}", target_path);
                        pin_msg(&bot, chat_id, msg_id).await?;
                    } else if score < ctx.delete_score_limit {
                        let target_path = format!("{}/{}", ctx.trash_dir.display(), file_name);
                        bot.delete_message(chat_id, msg_id).await?;
                        info!("Deleted disliked message");
                        fs::rename(&file_path, &target_path).await?;
                        db.update_path(chat_id, msg_id, &target_path).await?;
                        info!("Delete: {}", file_path.display());
                        // do not need unpin deleted message
                    } else {
                        let target_path = format!("{}/{}", ctx.output_dir.display(), file_name);
                        fs::rename(file_path, &target_path).await?;
                        db.update_path(chat_id, msg_id, &target_path).await?;
                        info!("Unfav: {}", target_path);
                        unpin_msg(&bot, chat_id, msg_id).await?;
                    }
                } else {
                    bot.delete_message(chat_id, msg_id).await?;
                    info!("Deleted out of control message");
                }
            }
            Ok(())
        },
    )
}

async fn pin_msg(bot: &Bot, chat_id: ChatId, msg_id: MessageId) -> Result<()> {
    bot.pin_chat_message(chat_id, msg_id).await?;
    info!("Pinned message: {}", msg_id.0);
    Ok(())
}

async fn unpin_msg(bot: &Bot, chat_id: ChatId, msg_id: MessageId) -> Result<()> {
    let mut unpin = bot.unpin_chat_message(chat_id);
    unpin.message_id = Some(msg_id);
    unpin.await?;
    info!("Unpinned message: {}", msg_id.0);
    Ok(())
}
