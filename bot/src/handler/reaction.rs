use super::{
    MyDialogue,
    utils::{TryMultipleTimes, pin_msg, unpin_msg},
};
use crate::{
    context::Context,
    storage::{ChatState, FileState, MyStorage},
};
use anyhow::Result;
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    prelude::Requester as _,
    types::{ReactionCount, ReactionType, Update, UpdateKind},
};
use tracing::{debug, info, instrument};

pub fn reaction_count_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_count_updated().endpoint(reaction_count_handle)
}

pub fn reaction_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().endpoint(reaction_handle)
}

#[instrument(
    level = "debug",
    skip_all,
    fields(chat_id, msg_id, new_reaction, old_reaction)
)]
async fn reaction_handle(
    bot: Bot,
    dialogue: MyDialogue,
    update: Update,
    storage: MyStorage,
) -> Result<()> {
    let chat_id = dialogue.chat_id();
    tracing::Span::current().record("chat_id", chat_id.0);
    if matches!(storage.get_chat_state(chat_id).await?, ChatState::Paused) {
        bot.send_message(chat_id, "React paused").await?;
        dialogue.exit().await?;
        return Ok(());
    }
    if let UpdateKind::MessageReaction(reaction) = update.kind {
        let msg_id = reaction.message_id;
        tracing::Span::current().record("msg_id", msg_id.0);
        let handle = (chat_id, msg_id);
        if (storage.get_file_state_by_handle(handle).await).is_ok() {
            if let Some(ReactionType::Emoji { emoji }) = reaction.new_reaction.first() {
                tracing::Span::current().record("new_reaction", emoji);
                match emoji.as_str() {
                    "👍" | "❤" => {
                        storage
                            .set_file_state_by_handle_and_link(handle, FileState::Fav)
                            .await?;
                        (|| pin_msg(&bot, chat_id, msg_id))
                            .try_multiple_times(3)
                            .await?;
                        info!(">> BOT: fav file-handle ({}, {})", chat_id, msg_id);
                    }
                    "👎" => {
                        storage.cancel_task_by_handle(chat_id, msg_id).await?;
                        (|| bot.delete_message(chat_id, msg_id))
                            .try_multiple_times(3)
                            .await?;
                        // do not need unpin deleted message
                        storage
                            .set_file_state_by_handle_and_link(handle, FileState::Trash)
                            .await?;
                        storage.delete_handle(handle).await?;
                        info!(">> BOT: deleted disliked message ({}, {})", chat_id, msg_id);
                    }
                    _ => {}
                }
            } else if let Some(ReactionType::Emoji { emoji }) = reaction.old_reaction.first() {
                tracing::Span::current().record("new_reaction", emoji);
                if matches!(emoji.as_str(), "👍" | "❤") {
                    storage
                        .set_file_state_by_handle_and_link(handle, FileState::Normal)
                        .await?;
                    (|| unpin_msg(&bot, chat_id, msg_id))
                        .try_multiple_times(3)
                        .await?;
                    info!(">> BOT: unfav file-handle ({}, {})", chat_id, msg_id);
                }
            }
        } else {
            (|| bot.delete_message(chat_id, msg_id))
                .try_multiple_times(3)
                .await?;
            info!(">> BOT: deleted out of control message");
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip_all, fields(chat_id, msg_id, score))]
async fn reaction_count_handle(
    bot: Bot,
    dialogue: MyDialogue,
    update: Update,
    ctx: Context,
    storage: MyStorage,
) -> Result<()> {
    let chat_id = dialogue.chat_id();
    tracing::Span::current().record("chat_id", chat_id.0);
    if matches!(storage.get_chat_state(chat_id).await?, ChatState::Paused) {
        bot.send_message(chat_id, "React paused").await?;
        dialogue.exit().await?;
        return Ok(());
    }
    if let UpdateKind::MessageReactionCount(reaction) = update.kind {
        let msg_id = reaction.message_id;
        tracing::Span::current().record("msg_id", msg_id.0);
        if (storage.get_file_state_by_handle((chat_id, msg_id)).await).is_ok() {
            let score: i32 = reaction
                .reactions
                .iter()
                .filter_map(|r| match r {
                    ReactionCount {
                        r#type: ReactionType::Emoji { emoji },
                        total_count,
                    } => match emoji.as_ref() {
                        "👍" | "😁" | "🙏" | "😇" | "🤗" => Some(*total_count as i32),
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

            tracing::Span::current().record("score", score);
            if score >= ctx.fav_score_limit {
                storage
                    .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Fav)
                    .await?;
                (|| pin_msg(&bot, chat_id, msg_id))
                    .try_multiple_times(3)
                    .await?;
                info!(">> BOT: fav file-handle ({} {})", chat_id, msg_id);
            } else if score < ctx.delete_score_limit {
                storage.cancel_task_by_handle(chat_id, msg_id).await?;
                (|| bot.delete_message(chat_id, msg_id))
                    .try_multiple_times(3)
                    .await?;
                // do not need unpin deleted message
                storage
                    .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Trash)
                    .await?;
                storage.delete_handle((chat_id, msg_id)).await?;
                info!(">> BOT: deleted disliked message");
            } else {
                storage
                    .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Normal)
                    .await?;
                (|| unpin_msg(&bot, chat_id, msg_id))
                    .try_multiple_times(3)
                    .await?;
                info!(">> BOT: unfav file-handle ({} {})", chat_id, msg_id);
            }
        } else {
            debug!("Unknown file state");
            (|| bot.delete_message(chat_id, msg_id))
                .try_multiple_times(3)
                .await?;
            info!(">> BOT: deleted out of control message");
        }
    }
    Ok(())
}
