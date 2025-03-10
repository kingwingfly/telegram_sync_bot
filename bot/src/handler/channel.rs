use super::{MyDialogue, command::cmd_handler};
use crate::{context::Context, storage::MyStorage, utils::cp_from_container};
use anyhow::Result;
use log::{debug, info};
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    net::Download as _,
    prelude::{Request as _, Requester as _},
    requests::HasPayload as _,
    types::{ChatId, MediaKind, Message, MessageId, MessageKind, ReactionType, Update},
};
use tokio::fs;

pub fn channel_post_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_channel_post()
        .branch(cmd_handler())
        .endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message, ctx: Context, db: MyStorage| {
                if !db.get_chat_state(dialogue.chat_id()).await {
                    bot.send_message(dialogue.chat_id(), "Paused").await?;
                    dialogue.exit().await?;
                    return Ok(());
                }
                if let MessageKind::Common(common_msg) = msg.kind {
                    match common_msg.media_kind {
                        MediaKind::Text(text) => {
                            debug!("Text: {:#?}", text);
                        }
                        MediaKind::Document(document) => {
                            tokio::spawn(async move {
                                handle_channel_file(
                                    bot,
                                    ctx,
                                    db,
                                    &document.document.file.id,
                                    document
                                        .document
                                        .file_name
                                        .unwrap_or(document.document.file.id.clone()),
                                    msg.id,
                                    msg.chat.id,
                                )
                                .await?;
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        MediaKind::Video(video) => {
                            tokio::spawn(async move {
                                handle_channel_file(
                                    bot,
                                    ctx,
                                    db,
                                    &video.video.file.id,
                                    format!("{}.mp4", video.video.file.id),
                                    msg.id,
                                    msg.chat.id,
                                )
                                .await?;
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        MediaKind::Photo(photo) => {
                            tokio::spawn(async move {
                                if let Some(photo) =
                                    photo.photo.into_iter().max_by_key(|p| p.height)
                                {
                                    handle_channel_file(
                                        bot,
                                        ctx,
                                        db,
                                        &photo.file.id,
                                        format!("{}.jpg", photo.file.id),
                                        msg.id,
                                        msg.chat.id,
                                    )
                                    .await?;
                                }
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        _ => {}
                    }
                }
                Ok(())
            },
        )
}

async fn handle_channel_file(
    bot: Bot,
    ctx: Context,
    db: MyStorage,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    msg_id: MessageId,
    chat_id: ChatId,
) -> Result<()> {
    info!("Handling file: {}", file_id.as_ref());
    let syncing = db.get_syncing_state(chat_id).await;
    match db.get_path_by_file_id(file_id.as_ref()).await? {
        Some(old_path) => {
            set_emoji(&bot, chat_id, msg_id, "🫡").await?;
            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
            fs::rename(&old_path, &target_path).await?;
            if let Some(old_id) = db.get_msg_id(chat_id, file_id.as_ref()).await? {
                bot.delete_message(chat_id, old_id).await.ok(); // old_id may have been deleted
                info!("Deleted old message");
            }
            db.insert_or_replace_files(chat_id, file_id.as_ref(), msg_id)
                .await?;
            db.insert_or_replace_path(file_id.as_ref(), &target_path)
                .await?;
            set_emoji(&bot, chat_id, msg_id, "👌").await?;
            info!("Moved: {} -> {}", old_path, target_path);
        }
        _ if syncing => {
            set_emoji(&bot, chat_id, msg_id, "🫡").await?;
            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
            info!("Downloading: {}", file_id.as_ref());
            let save_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
            let server_path = loop {
                if let Ok(f) = bot.get_file(file_id.as_ref()).send().await {
                    break f.path;
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            };
            match ctx.local_server {
                false => {
                    let mut file = fs::File::create(&save_path).await?;
                    bot.download_file(&server_path, &mut file).await?;
                }
                true => match &ctx.container_manager {
                    Some(container_manager) => {
                        cp_from_container(
                            container_manager,
                            ctx.container_id.as_ref().unwrap(),
                            server_path,
                            &save_path,
                        )
                        .await?;
                    }
                    None => {
                        fs::copy(server_path, &save_path).await?;
                    }
                },
            }
            db.insert_or_replace_files(chat_id, file_id.as_ref(), msg_id)
                .await?;
            db.insert_or_replace_path(file_id.as_ref(), &target_path)
                .await?;
            set_emoji(&bot, chat_id, msg_id, "👌").await?;
            info!("Saved: {}", save_path);
        }
        _ => {}
    }
    Ok(())
}

async fn set_emoji(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    emoji: impl AsRef<str>,
) -> Result<()> {
    let mut req = bot.set_message_reaction(chat_id, msg_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: emoji.as_ref().to_string(),
    }]);
    req.await?;
    Ok(())
}
