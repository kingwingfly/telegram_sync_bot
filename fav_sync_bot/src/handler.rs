use crate::{bot::State, context::Context, utils::cp_from_container};
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
    types::{InputFile, MediaKind, MessageKind, ReactionType, UpdateKind},
    utils::command::BotCommands as _,
};
use tokio::fs;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "show the current state.")]
    State,
    #[command(description = "pause the bot.")]
    Pause,
    #[command(description = "unpause the bot.")]
    Unpause,
}

type MyDialogue = Dialogue<State, InMemStorage<State>>;

pub fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(msg_handler())
        .branch(react_handler())
        .branch(callback_handler())
}

fn cmd_handler() -> UpdateHandler<anyhow::Error> {
    teloxide::filter_command::<Command, _>()
        .branch(
            case![Command::Help].endpoint(async |bot: Bot, msg: Message| {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }),
        )
        .branch(
            case![Command::State].endpoint(async |bot: Bot, msg: Message, state: State| {
                bot.send_message(msg.chat.id, format!("{:?}", state))
                    .await?;
                Ok(())
            }),
        )
        .branch(
            case![State::Paused].branch(case![Command::Unpause].endpoint(
                async |bot: Bot, dialogue: MyDialogue, msg: Message, owner: UserId| {
                    if msg.from.map(|user| user.id) != Some(owner) {
                        bot.send_message(msg.chat.id, "Permission denied: You are not owner")
                            .await?;
                        dialogue.exit().await?;
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
                                handle_file(
                                    bot,
                                    ctx,
                                    &document.document.file.id,
                                    document
                                        .document
                                        .file_name
                                        .unwrap_or(document.document.file.id.clone()),
                                    msg.chat.id,
                                )
                                .await?;
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        MediaKind::Video(video) => {
                            tokio::spawn(async move {
                                handle_file(
                                    bot,
                                    ctx,
                                    &video.video.file.id,
                                    format!("{}.mp4", video.video.file.id),
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
                                    handle_file(
                                        bot,
                                        ctx,
                                        &photo.file.id,
                                        format!("{}.jpg", photo.file.id),
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
        .branch(dptree::endpoint(async || Ok(())))
}

fn react_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_message_reaction_updated().branch(case![State::Working].endpoint(
        async |bot: Bot, update: Update, ctx: Context| {
            if let UpdateKind::MessageReaction(reaction) = update.kind {
                let msg_id = reaction.message_id.0.to_ne_bytes();
                let msgs = ctx
                    .db
                    .open_tree(reaction.chat.id.0.to_ne_bytes())
                    .context("Failed to open msg tree")?;
                if let Ok(Some(file_path)) = msgs.get(msg_id) {
                    let file_path = std::path::Path::new(std::str::from_utf8(&file_path)?);
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
                                msgs.insert(msg_id, target_path.as_bytes())
                                    .context("Failed to update msg and file path")?;
                                info!("Fav: {}", target_path);
                            }
                            "👎" => {
                                msgs.remove(msg_id).context("Failed to remove msg_id")?;
                                bot.delete_message(reaction.chat.id, reaction.message_id)
                                    .await?;
                                fs::remove_file(&file_path).await?;
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
                            msgs.insert(msg_id, target_path.as_bytes())
                                .context("Failed to update msg and file path")?;
                            info!("Unfav: {}", target_path);
                        }
                    }
                } else {
                    debug!("Won't react to user message");
                }
            }
            Ok(())
        },
    ))
}

fn callback_handler() -> UpdateHandler<anyhow::Error> {
    Update::filter_callback_query()
}

async fn handle_file(
    bot: Bot,
    ctx: Context,
    file_id: impl AsRef<str>,
    file_name: impl AsRef<str>,
    chat_id: ChatId,
) -> Result<()> {
    info!("Saving: {}", file_id.as_ref());
    let server_path = bot.get_file(file_id.as_ref()).send().await?.path;
    let save_path = format!("{}/{}", ctx.output_dir.display(), file_name.as_ref());
    match ctx.local_server {
        false => {
            let mut file = fs::File::create(&save_path).await?;
            info!("Saving: {}", save_path);
            bot.download_file(&server_path, &mut file).await?;
        }
        true => match ctx.container_manager {
            Some(container_manager) => {
                cp_from_container(
                    container_manager,
                    ctx.container_id.unwrap(),
                    server_path,
                    &save_path,
                )
                .await?
            }
            None => {
                fs::copy(server_path, &save_path).await?;
            }
        },
    }
    let reply_id = bot
        .send_photo(chat_id, InputFile::file_id(file_id.as_ref().to_owned()))
        .await?
        .id
        .0
        .to_ne_bytes();
    let msgs = ctx
        .db
        .open_tree(chat_id.0.to_ne_bytes())
        .context("Failed to open msg tree")?;
    msgs.insert(reply_id, save_path.as_bytes())
        .context("Failed to save msg_id")?;
    info!("Saved: {}", save_path);
    Ok(())
}
