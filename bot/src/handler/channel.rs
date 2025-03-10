use super::{MyDialogue, command::cmd_handler, utils::set_emoji};
use crate::storage::{ChatState, MyStorage, TransportState};
use anyhow::Result;
use log::{debug, info};
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    prelude::Requester as _,
    types::{MediaKind, Message, MessageKind, Update},
};

pub fn channel_post_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_channel_post()
        .branch(cmd_handler())
        .endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message, storage: MyStorage| {
                let (chat_id, msg_id) = (msg.chat.id, msg.id);
                if storage.get_chat_state(chat_id).await? == ChatState::Paused {
                    bot.send_message(chat_id, "Paused").await?;
                    dialogue.exit().await?;
                    return Ok(());
                }
                set_emoji(&bot, chat_id, msg_id, "🫡").await?;
                if let MessageKind::Common(common_msg) = msg.kind {
                    if let Some((file_id, file_name)) = match common_msg.media_kind {
                        MediaKind::Document(document) => {
                            let file_id = document.document.file.id;
                            let file_name = document.document.file_name.unwrap_or(file_id.clone());
                            Some((file_id, file_name))
                        }
                        MediaKind::Video(video) => {
                            let file_id = video.video.file.id;
                            let file_name = format!("{}.mp4", file_id);
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
                            Some((file_id, file_name))
                        }
                        _ => None,
                    } {
                        let handle = storage.add(file_id, file_name).await?;
                        tokio::spawn(async move {
                            handle.cancelled().await;
                            let emoji = match handle.get_state() {
                                TransportState::Completed => "👌",
                                TransportState::Cancelled => "😨",
                                TransportState::Failed => "😭",
                                _ => "☃",
                            };
                            set_emoji(&bot, chat_id, msg_id, emoji).await?;
                            Result::<_, anyhow::Error>::Ok(())
                        });
                    }
                }
                Ok(())
            },
        )
}
