use crate::{
    context::Context,
    storage::MyStorage,
    utils::{cp_from_container, gen_pwd},
};
use anyhow::{Context as _, Result};
use dptree::case;
use log::{debug, info};
use std::path::Path;
use teloxide::{
    dispatching::{
        UpdateFilterExt, UpdateHandler,
        dialogue::{self, InMemStorage},
    },
    macros::BotCommands,
    net::Download,
    prelude::*,
    requests::HasPayload,
    types::{
        InputFile, MediaKind, MediaText, MessageCommon, MessageId, MessageKind, ReactionCount,
        ReactionType, UpdateKind,
    },
    utils::command::BotCommands as _,
};
use tokio::fs;

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
    #[command(description = "Print current bypass password in the server side.")]
    BypassPwd,
}

type MyDialogue = Dialogue<(), InMemStorage<()>>;

pub fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<()>, (), _>()
        .branch(msg_handler())
        .branch(channel_post_handler())
        .branch(reaction_handler())
        .branch(reaction_count_handler())
        .branch(callback_handler())
}

fn cmd_handler() -> UpdateHandler<anyhow::Error> {
    teloxide::filter_command::<Command, _>()
        .branch(
            case![Command::Start].endpoint(async |bot: Bot, msg: Message| {{
                bot.send_message(msg.chat.id, "This is a bot to sync files from chat. Enter /help to see all commands.")
                    .await?;
                Ok(())}})
        )
        .branch(
            case![Command::Help].endpoint(async |bot: Bot, msg: Message| {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }),
        ).branch(case![Command::BypassPwd].endpoint(async |ctx: Context| {
            info!("Bypass_pwd: {}", ctx.bypass_pwd.read().unwrap());
            Ok(())
        }))
        .branch(
            case![Command::State].endpoint(async |bot: Bot, msg: Message, db: MyStorage| {
                let working = db.get_chat_state(msg.chat.id).await;
                bot.send_message(msg.chat.id, format!("working: {}", working))
                    .await?;
                Ok(())
            }),
        )
        .branch(case![Command::Switch].endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message, ctx: Context, db: MyStorage| {
                if msg.from.map(|user| ctx.bypass_users.contains(&user.id)) != Some(true) {
                    // check bypass_pwd
                    match msg {
                        Message {
                            kind:
                                MessageKind::Common(MessageCommon {
                                    media_kind: MediaKind::Text(MediaText { text, .. }),
                                    ..
                                }),
                            ..
                        } if text == format!("/switch {}", ctx.bypass_pwd.read().unwrap()) => {
                            // renew bypass_pwd
                            let new = gen_pwd();
                            info!("New bypass_pwd: {}", new);
                            *ctx.bypass_pwd.write().unwrap() = new;
                        }
                        _ => {
                            bot.send_message(
                                msg.chat.id,
                                "Permission denied: You are not in allow users list or invalid password",
                            )
                            .await?;
                            dialogue.exit().await?;
                            return Ok(());
                        }
                    }
                }
                let working = db.switch_chat_state(msg.chat.id).await?;
                bot.send_message(msg.chat.id, format!("Working: {}", working)).await?;
                Ok(())
            },
        ))
}

fn msg_handler() -> UpdateHandler<anyhow::Error> {
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

fn reaction_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, ctx: Context, db: MyStorage| {
            if !db.get_chat_state(dialogue.chat_id()).await {
                bot.send_message(dialogue.chat_id(), "Paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReaction(reaction) = update.kind {
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
                                db.update_mp_pair(
                                    reaction.chat.id,
                                    reaction.message_id,
                                    &target_path,
                                )
                                .await?;
                                info!("Fav: {}", target_path);
                            }
                            "👎" => {
                                let target_path =
                                    format!("{}/{}", ctx.trash_dir.display(), file_name);
                                bot.delete_message(reaction.chat.id, reaction.message_id)
                                    .await?;
                                fs::rename(&file_path, &target_path).await?;
                                db.update_mp_pair(
                                    reaction.chat.id,
                                    reaction.message_id,
                                    &target_path,
                                )
                                .await?;
                                info!("Delete: {}", file_path.display());
                            }
                            _ => {}
                        }
                    } else if let Some(ReactionType::Emoji { emoji }) =
                        reaction.old_reaction.first()
                    {
                        if matches!(emoji.as_str(), "👍" | "❤") {
                            let target_path = format!("{}/{}", ctx.output_dir.display(), file_name);
                            fs::rename(file_path, &target_path).await?;
                            db.update_mp_pair(reaction.chat.id, reaction.message_id, &target_path)
                                .await?;
                            info!("Unfav: {}", target_path);
                        }
                    }
                } else {
                    debug!("Unable to handle react to user message");
                }
            }
            Ok(())
        },
    )
}

fn reaction_count_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_count_updated().endpoint(
        async |bot: Bot, dialogue: MyDialogue, update: Update, ctx: Context, db: MyStorage| {
            if !db.get_chat_state(dialogue.chat_id()).await {
                bot.send_message(dialogue.chat_id(), "Paused").await?;
                dialogue.exit().await?;
                return Ok(());
            }
            if let UpdateKind::MessageReactionCount(reaction) = update.kind {
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
                        db.update_mp_pair(reaction.chat.id, reaction.message_id, &target_path)
                            .await?;
                        info!("Fav: {}", target_path);
                    } else if score < ctx.delete_score_limit {
                        let target_path = format!("{}/{}", ctx.trash_dir.display(), file_name);
                        db.update_mp_pair(reaction.chat.id, reaction.message_id, &target_path)
                            .await?;
                        bot.delete_message(reaction.chat.id, reaction.message_id)
                            .await?;
                        fs::rename(&file_path, target_path).await?;
                        info!("Delete: {}", file_path.display());
                    } else {
                        let target_path = format!("{}/{}", ctx.output_dir.display(), file_name);
                        fs::rename(file_path, &target_path).await?;
                        db.update_mp_pair(reaction.chat.id, reaction.message_id, &target_path)
                            .await?;
                        info!("Unfav: {}", target_path);
                    }
                }
            }
            Ok(())
        },
    )
}

fn channel_post_handler() -> UpdateHandler<anyhow::Error> {
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

fn callback_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_callback_query()
}

// handler helper functions

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
    let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
    if let Some((old_id, old_path)) = db.get_mp_pair(chat_id, file_id.as_ref()).await? {
        fs::rename(&old_path, &target_path).await?;
        info!("Moved: {} -> {}", old_path, target_path);
        bot.delete_message(chat_id, old_id).await?;
        let reply_id = reply(
            &bot,
            chat_id,
            InputFile::file_id(file_id.as_ref().to_owned()),
        )
        .await?
        .id;
        db.update_mp_pair(chat_id, reply_id, &target_path).await?;
    } else {
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
        let reply_id = reply(
            &bot,
            chat_id,
            InputFile::file_id(file_id.as_ref().to_owned()),
        )
        .await?
        .id;
        db.insert_mp_pair(chat_id, file_id.as_ref(), reply_id, &target_path)
            .await?;
        info!("Saved: {}", save_path);
    }
    Ok(())
}

async fn handle_channel_file(
    bot: Bot,
    ctx: Context,
    db: MyStorage,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    message_id: MessageId,
    chat_id: ChatId,
) -> Result<()> {
    info!("Handling file: {}", file_id.as_ref());
    let mut req = bot.set_message_reaction(chat_id, message_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: "🫡".to_string(),
    }]);
    req.await?;
    let target_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
    if let Some((old_id, old_path)) = db.get_mp_pair(chat_id, file_id.as_ref()).await? {
        fs::rename(&old_path, &target_path).await?;
        info!("Moved: {} -> {}", old_path, target_path);
        bot.delete_message(chat_id, old_id).await?;
        db.update_mp_pair(chat_id, message_id, &target_path).await?;
    } else {
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
        db.insert_mp_pair(chat_id, file_id.as_ref(), message_id, &target_path)
            .await?;
        info!("Saved: {}", save_path);
    }
    let mut req = bot.set_message_reaction(chat_id, message_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: "👌".to_string(),
    }]);
    req.await?;
    Ok(())
}
