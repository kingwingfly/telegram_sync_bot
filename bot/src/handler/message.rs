use super::{MyDialogue, command::cmd_handler};
use crate::{context::Context, storage::MyStorage, utils::cp_from_container};
use anyhow::Result;
use log::{debug, info};
use teloxide::{
    Bot,
    dispatching::{UpdateFilterExt as _, UpdateHandler},
    net::Download as _,
    prelude::{Request as _, Requester as _},
    types::{ChatId, InputFile, MediaKind, Message, MessageKind, Update},
};
use tokio::fs;

pub fn msg_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message().branch(cmd_handler()).endpoint(
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
                            handle_msg_file(
                                bot,
                                ctx,
                                db,
                                &document.document.file.id,
                                document
                                    .document
                                    .file_name
                                    .unwrap_or(document.document.file.id.clone()),
                                msg.chat.id,
                                Bot::send_document,
                            )
                            .await?;
                            Result::<_, anyhow::Error>::Ok(())
                        });
                    }
                    MediaKind::Video(video) => {
                        tokio::spawn(async move {
                            handle_msg_file(
                                bot,
                                ctx,
                                db,
                                &video.video.file.id,
                                format!("{}.mp4", video.video.file.id),
                                msg.chat.id,
                                Bot::send_video,
                            )
                            .await?;
                            Result::<_, anyhow::Error>::Ok(())
                        });
                    }
                    MediaKind::Photo(photo) => {
                        tokio::spawn(async move {
                            if let Some(photo) = photo.photo.into_iter().max_by_key(|p| p.height) {
                                handle_msg_file(
                                    bot,
                                    ctx,
                                    db,
                                    &photo.file.id,
                                    format!("{}.jpg", photo.file.id),
                                    msg.chat.id,
                                    Bot::send_photo,
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

/// Handle file directly sent to the bot.
/// Due to the limitation of the bot API, the msg sent directly to the bot will be
/// prevented from being deleted or edited, so the bot forwards the file to the chat
/// again which can be operated by the bot.
async fn handle_msg_file<F, Fut>(
    bot: Bot,
    ctx: Context,
    db: MyStorage,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    chat_id: ChatId,
    reply: F,
) -> Result<()>
where
    F: Fn(&Bot, ChatId, InputFile) -> Fut,
    Fut: core::future::IntoFuture<Output = core::result::Result<Message, teloxide::RequestError>>,
{
    info!("Handling file: {}", file_id.as_ref());
    let syncing = db.get_syncing_state(chat_id).await;
    match db.get_path_by_file_id(file_id.as_ref()).await? {
        Some(old_path) => {
            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
            fs::rename(&old_path, &target_path).await?;
            if let Some(old_msg_id) = db.get_msg_id(chat_id, file_id.as_ref()).await? {
                bot.delete_message(chat_id, old_msg_id).await.ok(); // old_id may have been deleted
                info!("Deleted old message");
            }
            let reply_id = reply(
                &bot,
                chat_id,
                InputFile::file_id(file_id.as_ref().to_owned()),
            )
            .await?
            .id;
            db.insert_or_replace_files(chat_id, file_id.as_ref(), reply_id)
                .await?;
            db.insert_or_replace_path(file_id.as_ref(), &target_path)
                .await?;
            info!("Moved: {} -> {}", old_path, target_path);
        }
        _ if syncing => {
            info!("Downloading: {}", file_id.as_ref());
            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
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
            let reply_id = reply(
                &bot,
                chat_id,
                InputFile::file_id(file_id.as_ref().to_owned()),
            )
            .await?
            .id;
            db.insert_or_replace_files(chat_id, file_id.as_ref(), reply_id)
                .await?;
            db.insert_or_replace_path(file_id.as_ref(), &target_path)
                .await?;
            info!("Saved: {}", save_path);
        }
        _ => {}
    }
    Ok(())
}
