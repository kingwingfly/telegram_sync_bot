use super::{
    MyDialogue,
    command::cmd_handler,
    utils::{TryMultipleTimes, set_emoji},
};
use crate::storage::{ChatState, FileState, MyStorage, TransportState};
use anyhow::Result;
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    prelude::Requester as _,
    types::{InputFile, MediaKind, Message, MessageKind, Update},
};
use tracing::{info, instrument};

pub fn msg_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message()
        .branch(cmd_handler())
        .endpoint(handle)
}

pub fn channel_post_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_channel_post()
        .branch(cmd_handler())
        .endpoint(handle)
}

#[instrument(level = "debug", skip_all, fields(chat_id=%msg.chat.id, msg_id=%msg.id))]
async fn handle(bot: Bot, dialogue: MyDialogue, msg: Message, storage: MyStorage) -> Result<()> {
    let (chat_id, mut msg_id) = (msg.chat.id, msg.id);
    let chat_state = storage.get_chat_state(chat_id).await?;
    if chat_state == ChatState::Paused {
        bot.send_message(chat_id, "Sync paused").await?;
        dialogue.exit().await?;
        return Ok(());
    }
    if let MessageKind::Common(common_msg) = msg.kind {
        if let Some((file_id, file_name)) = match common_msg.media_kind {
            MediaKind::Document(document) => {
                // gif will be handled here too
                let file_id = document.document.file.id;
                let file_name = document.document.file_name.unwrap_or(file_id.clone());
                if document.media_group_id.is_some() {
                    // break up the group
                    let old = msg_id;
                    msg_id = (|| bot.send_document(chat_id, InputFile::file_id(&file_id)))
                        .try_multiple_times(3)
                        .await?
                        .id;
                    (|| bot.delete_message(chat_id, old))
                        .try_multiple_times(3)
                        .await?;
                }
                Some((file_id, file_name))
            }
            MediaKind::Video(video) => {
                let file_id = video.video.file.id;
                let file_name = format!("{}.mp4", file_id);
                if video.media_group_id.is_some() {
                    // break up the group
                    let old = msg_id;
                    msg_id = (|| bot.send_video(chat_id, InputFile::file_id(&file_id)))
                        .try_multiple_times(3)
                        .await?
                        .id;
                    (|| bot.delete_message(chat_id, old))
                        .try_multiple_times(3)
                        .await?;
                }
                Some((file_id, file_name))
            }
            MediaKind::Photo(photo) => {
                let file_id = photo
                    .photo
                    .into_iter()
                    .max_by_key(|p| p.height)
                    .unwrap()
                    .file
                    .id;
                let file_name = format!("{}.jpg", file_id);
                if photo.media_group_id.is_some() {
                    // break up the group
                    let old = msg_id;
                    msg_id = (|| bot.send_photo(chat_id, InputFile::file_id(&file_id)))
                        .try_multiple_times(3)
                        .await?
                        .id;
                    (|| bot.delete_message(chat_id, old))
                        .try_multiple_times(3)
                        .await?;
                }
                Some((file_id, file_name))
            }
            _ => None,
        } {
            if let Some((old_chat_id, old_msg_id)) = storage
                .set_file_handle(chat_id, msg_id, file_id.clone())
                .await?
            {
                debug_assert_eq!(old_chat_id, chat_id, "chat_id mismatch");
                if (|| bot.delete_message(chat_id, old_msg_id))
                    .try_multiple_times(3)
                    .await
                    .is_ok()
                {
                    info!(">> BOT: deleted message: {}", old_msg_id);
                }
            }
            if chat_state == ChatState::PartiallyActive {
                return Ok(());
            }
            tokio::spawn(async move {
                (|| set_emoji(&bot, chat_id, msg_id, "🫡"))
                    .try_multiple_times(3)
                    .await?;
                let emoji = match storage.add_task(file_id, file_name).await? {
                    Some(handle) => match handle.result().await {
                        TransportState::Completed => "👌",
                        TransportState::Cancelled => "😨",
                        TransportState::Failed => "😭",
                        _ => "👾",
                    },
                    None => "👌",
                };
                (|| set_emoji(&bot, chat_id, msg_id, emoji))
                    .try_multiple_times(3)
                    .await?;
                storage
                    .set_file_state_by_handle_and_link((chat_id, msg_id), FileState::Normal)
                    .await
                    .ok();
                Result::<_, anyhow::Error>::Ok(())
            });
        }
    }
    Ok(())
}
