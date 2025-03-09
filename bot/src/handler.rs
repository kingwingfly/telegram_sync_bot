use std::path::Path;

use crate::{
    bot::State,
    context::Context,
    utils::{cp_from_container, gen_pwd},
};
use anyhow::{Context as _, Result};
use dptree::case;
use log::{debug, info};
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
    #[command(description = "display this text.")]
    Help,
    #[command(description = "show the current state.")]
    State,
    #[command(description = "pause the bot.")]
    Pause,
    #[command(description = "unpause the bot.")]
    Unpause,
    #[command(description = "print current bypass password in the server side.")]
    BypassPwd,
}

type MyDialogue = Dialogue<State, InMemStorage<State>>;

pub fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<State>, State, _>()
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
            case![Command::State].endpoint(async |bot: Bot, msg: Message, state: State| {
                bot.send_message(msg.chat.id, format!("{:?}", state))
                    .await?;
                Ok(())
            }),
        )
        .branch(
            case![State::Paused].branch(case![Command::Unpause].endpoint(
                async |bot: Bot, dialogue: MyDialogue, msg: Message, ctx: Context| {
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
                            } if text == format!("/unpause {}", ctx.bypass_pwd.read().unwrap()) => {
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
                    dialogue.update(State::Working).await?;
                    bot.send_message(msg.chat.id, "Working").await?;
                    Ok(())
                },
            )),
        )
        .branch(case![State::Working].branch(case![Command::Pause].endpoint(
            async |bot: Bot, dialogue: MyDialogue, msg: Message| {
                dialogue.update(State::Paused).await?;
                bot.send_message(msg.chat.id, "Paused").await?;
                Ok(())
            },
        )))
}

fn msg_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message()
        .branch(cmd_handler())
        .branch(
            case![State::Working].endpoint(async |bot: Bot, msg: Message, ctx: Context| {
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
                                if let Some(photo) =
                                    photo.photo.into_iter().max_by_key(|p| p.height)
                                {
                                    handle_msg_file(
                                        bot,
                                        ctx,
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
            }),
        )
        .branch(dptree::endpoint(async || Ok(())))
}

fn reaction_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().branch(case![State::Working].endpoint(
        async |bot: Bot, update: Update, ctx: Context| {
            if let UpdateKind::MessageReaction(reaction) = update.kind {
                let msg_id = reaction.message_id.0.to_ne_bytes();
                let chat = ctx.db.open_tree(reaction.chat.id.0.to_ne_bytes())?;
                if let Ok(Some(file_path)) = chat.get(msg_id) {
                    let file_path = Path::new(std::str::from_utf8(&file_path)?);
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
                                chat.insert(msg_id, target_path.as_bytes())
                                    .context("Failed to update msg and file path")?;
                                info!("Fav: {}", target_path);
                            }
                            "👎" => {
                                let target_path =
                                    format!("{}/{}", ctx.trash_dir.display(), file_name);
                                chat.remove(msg_id).context("Failed to remove msg_id")?;
                                bot.delete_message(reaction.chat.id, reaction.message_id)
                                    .await?;
                                fs::rename(&file_path, target_path).await?;
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
                            chat.insert(msg_id, target_path.as_bytes())
                                .context("Failed to update msg and file path")?;
                            info!("Unfav: {}", target_path);
                        }
                    }
                } else {
                    debug!("Unable to handle react to user message");
                }
            }
            Ok(())
        },
    ))
}

fn reaction_count_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_count_updated().branch(case![State::Working].endpoint(
        async |bot: Bot, update: Update, ctx: Context| {
            if let UpdateKind::MessageReactionCount(reaction) = update.kind {
                let msg_id = reaction.message_id.0.to_ne_bytes();
                let chat = ctx.db.open_tree(reaction.chat.id.0.to_ne_bytes())?;
                if let Ok(Some(file_path)) = chat.get(msg_id) {
                    let file_path = Path::new(std::str::from_utf8(&file_path)?);
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
                        chat.insert(msg_id, target_path.as_bytes())
                            .context("Failed to update msg and file path")?;
                        info!("Fav: {}", target_path);
                    } else if score < ctx.delete_score_limit {
                        let target_path = format!("{}/{}", ctx.trash_dir.display(), file_name);
                        chat.remove(msg_id)
                            .context("Failed to update msg and file path")?;
                        bot.delete_message(reaction.chat.id, reaction.message_id)
                            .await?;
                        fs::rename(&file_path, target_path).await?;
                        info!("Delete: {}", file_path.display());
                    } else {
                        let target_path = format!("{}/{}", ctx.output_dir.display(), file_name);
                        if !Path::new(&target_path).exists() {
                            fs::rename(file_path, &target_path).await?;
                            chat.insert(msg_id, target_path.as_bytes())
                                .context("Failed to update msg and file path")?;
                            info!("Unfav: {}", target_path);
                        }
                    }
                }
            }
            Ok(())
        },
    ))
}

fn channel_post_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_channel_post()
        .branch(cmd_handler())
        .branch(
            case![State::Working].endpoint(async |bot: Bot, msg: Message, ctx: Context| {
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
            }),
        )
}

fn callback_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_callback_query()
}

// handler helper functions

async fn save_file(
    bot: &Bot,
    ctx: &Context,
    file_id: &impl AsRef<str>,
    file_name: &impl AsRef<str>,
) -> Result<String> {
    info!("Saving: {}", file_id.as_ref());
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
    info!("Saved: {}", save_path);
    Ok(save_path)
}

/// Handle file directly sent to the bot.
/// Due to the limitation of the bot API, the msg sent directly to the bot will be
/// prevented from being deleted or edited, so the bot forwards the file to the chat
/// again which can be operated by the bot.
async fn handle_msg_file<F, Fut>(
    bot: Bot,
    ctx: Context,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    chat_id: ChatId,
    reply: F,
) -> Result<()>
where
    F: Fn(&Bot, ChatId, InputFile) -> Fut,
    Fut: core::future::IntoFuture<Output = core::result::Result<Message, teloxide::RequestError>>,
{
    let save_path = save_file(&bot, &ctx, &file_id, &file_name).await?;
    let reply_id = reply(
        &bot,
        chat_id,
        InputFile::file_id(file_id.as_ref().to_owned()),
    )
    .await?
    .id
    .0
    .to_ne_bytes();
    let msgs = ctx.db.open_tree(chat_id.0.to_ne_bytes())?;
    msgs.insert(reply_id, save_path.as_bytes())?;
    Ok(())
}

async fn handle_channel_file(
    bot: Bot,
    ctx: Context,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    message_id: MessageId,
    chat_id: ChatId,
) -> Result<()> {
    let mut req = bot.set_message_reaction(chat_id, message_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: "🫡".to_string(),
    }]);
    req.await?;
    let save_path = save_file(&bot, &ctx, &file_id, &file_name).await?;
    let mut req = bot.set_message_reaction(chat_id, message_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: "👌".to_string(),
    }]);
    req.await?;
    let msgs = ctx.db.open_tree(chat_id.0.to_ne_bytes())?;
    msgs.insert(message_id.0.to_ne_bytes(), save_path.as_bytes())?;
    Ok(())
}
