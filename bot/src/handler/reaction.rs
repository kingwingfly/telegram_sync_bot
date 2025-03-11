use super::{
    MyDialogue,
    utils::{pin_msg, unpin_msg},
};
use crate::{
    context::Context,
    storage::{ChatState, FileState, MyStorage},
};
use log::{debug, info};
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    prelude::Requester as _,
    types::{ReactionCount, ReactionType, Update, UpdateKind},
};

pub fn reaction_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, storage: MyStorage| {
            let chat_id = dialogue.chat_id();
            if matches!(storage.get_chat_state(chat_id).await?, ChatState::Paused) {
                bot.send_message(chat_id, "React paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReaction(reaction) = update.kind {
                let msg_id = reaction.message_id;
                let handle = (chat_id, msg_id);
                if (storage.get_file_state_by_handle((chat_id, msg_id)).await).is_ok() {
                    if let Some(ReactionType::Emoji { emoji }) = reaction.new_reaction.first() {
                        match emoji.as_str() {
                            "👍" | "❤" => {
                                storage
                                    .set_file_state_by_handle_and_link(handle, FileState::Fav)
                                    .await?;
                                pin_msg(&bot, chat_id, msg_id).await?;
                            }
                            "👎" => {
                                storage
                                    .set_file_state_by_handle_and_link(handle, FileState::Trash)
                                    .await?;
                                bot.delete_message(chat_id, msg_id).await?;
                                storage.cancel_task_by_handle(chat_id, msg_id).await?;
                                info!(">> BOT: deleted disliked message");
                                // do not need unpin deleted message
                            }
                            _ => {}
                        }
                    } else if let Some(ReactionType::Emoji { emoji }) =
                        reaction.old_reaction.first()
                    {
                        if matches!(emoji.as_str(), "👍" | "❤") {
                            storage
                                .set_file_state_by_handle_and_link(handle, FileState::Normal)
                                .await?;
                            unpin_msg(&bot, chat_id, msg_id).await?;
                        }
                    }
                } else {
                    bot.delete_message(chat_id, msg_id).await?;
                    info!(">> BOT: deleted out of control message");
                }
            }
            Ok(())
        },
    )
}

pub fn reaction_count_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_count_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, ctx: Context, storage: MyStorage| {
            let chat_id = dialogue.chat_id();
            if matches!(storage.get_chat_state(chat_id).await?, ChatState::Paused) {
                bot.send_message(chat_id, "React paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReactionCount(reaction) = update.kind {
                let msg_id = reaction.message_id;
                if (storage.get_file_state_by_handle((chat_id, msg_id)).await).is_ok() {
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
                        storage
                            .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Fav)
                            .await?;
                        pin_msg(&bot, chat_id, msg_id).await?;
                        info!("Fav: file-handle ({} {})", chat_id, msg_id);
                    } else if score < ctx.delete_score_limit {
                        storage
                            .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Trash)
                            .await?;
                        bot.delete_message(chat_id, msg_id).await?;
                        storage.cancel_task_by_handle(chat_id, msg_id).await?;
                        info!("Deleted disliked message");
                        // do not need unpin deleted message
                    } else {
                        storage
                            .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Normal)
                            .await?;
                        unpin_msg(&bot, chat_id, msg_id).await?;
                        info!("Unfav: file-handle ({} {})", chat_id, msg_id);
                    }
                } else {
                    debug!("Unknown file state");
                    bot.delete_message(chat_id, msg_id).await?;
                    info!("Deleted out of control message");
                }
            }
            Ok(())
        },
    )
}
