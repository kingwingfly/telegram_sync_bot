use anyhow::{Context, Result};
use log::{debug, error, info};
use sled::Db;
use std::sync::OnceLock;
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = run().await {
        error!("Error: {:?}", e);
    }
}

fn init() -> Result<()> {
    pretty_env_logger::env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("sled", log::LevelFilter::Info)
        .init();
    info!("Initializing..");
    dotenv::dotenv().ok();
    info!(
        "TELOXIDE_TOKEN: {}",
        std::env::var("TELOXIDE_TOKEN").context("TELOXIDE_TOKEN unset")?
    );
    info!(
        "OWNER_ID: {}",
        std::env::var("OWNER_ID").context("OWNER_ID unset")?
    );
    info!("Finished Initializing..");
    Ok(())
}

fn output_dir() -> &'static str {
    static OUTPUT_DIR: OnceLock<String> = OnceLock::new();
    OUTPUT_DIR.get_or_init(|| {
        let output_dir = std::env::args().nth(1).unwrap_or(".".to_string());
        info!("Output dir: {}", output_dir);
        std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");
        output_dir
    })
}

fn fav_dir() -> &'static str {
    static OUTPUT_DIR: OnceLock<String> = OnceLock::new();
    OUTPUT_DIR.get_or_init(|| {
        let output_dir = std::env::args().nth(1).unwrap_or(".".to_string());
        let fav_dir = format!("{}/favorite", output_dir);
        info!("Favorite dir: {}", fav_dir);
        std::fs::create_dir_all(&fav_dir).expect("Failed to create favorite dir");
        fav_dir
    })
}

async fn run() -> Result<()> {
    init()?;

    let bot = Bot::from_env();
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            sled::open(format!("{}/data/db.sled", output_dir())).context("Failed to open db")?,
            UserId(
                std::env::var("OWNER_ID")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .context("INVALID OWNER_ID")?
            )
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

#[derive(Clone, Default, Debug)]
pub enum State {
    #[default]
    Paused,
    Working,
}

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

fn handler() -> UpdateHandler<anyhow::Error> {
    use dptree::case;

    let command_handler = teloxide::filter_command::<Command, _>()
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
        )));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(
            case![State::Working].endpoint(async |bot: Bot, msg: Message, db: Db| {
                if let MessageKind::Common(common_msg) = msg.kind {
                    match common_msg.media_kind {
                        MediaKind::Text(text) => {
                            debug!("Text: {:#?}", text);
                        }
                        MediaKind::Document(document) => {
                            tokio::spawn(async move {
                                let path =
                                    bot.get_file(&document.document.file.id).send().await?.path;
                                let file_path = format!(
                                    "{}/{}",
                                    output_dir(),
                                    document
                                        .document
                                        .file_name
                                        .unwrap_or(document.document.file.id.clone())
                                );
                                let mut file = fs::File::create(&file_path).await?;
                                info!("Saving: {}", file_path);
                                bot.download_file(&path, &mut file).await?;
                                let msg_id = bot
                                    .send_video(
                                        msg.chat.id,
                                        InputFile::file_id(document.document.file.id),
                                    )
                                    .await?
                                    .id
                                    .0
                                    .to_ne_bytes();
                                let msgs =
                                    db.open_tree("msgs").context("Failed to open msg tree")?;
                                msgs.insert(msg_id, file_path.as_bytes())
                                    .context("Failed to save msg_id")?;
                                info!("Saved: {}", file_path);
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        MediaKind::Video(video) => {
                            tokio::spawn(async move {
                                let path = bot.get_file(&video.video.file.id).send().await?.path;
                                let file_path =
                                    format!("{}/{}.mp4", output_dir(), video.video.file.id);
                                let mut file = fs::File::create(&file_path).await?;
                                info!("Saving: {}", file_path);
                                bot.download_file(&path, &mut file).await?;
                                let msg_id = bot
                                    .send_video(
                                        msg.chat.id,
                                        InputFile::file_id(video.video.file.id),
                                    )
                                    .await?
                                    .id
                                    .0
                                    .to_ne_bytes();
                                let msgs =
                                    db.open_tree("msgs").context("Failed to open msg tree")?;
                                msgs.insert(msg_id, file_path.as_bytes())
                                    .context("Failed to save msg_id")?;
                                info!("Saved: {}", file_path);
                                Result::<_, anyhow::Error>::Ok(())
                            });
                        }
                        MediaKind::Photo(photo) => {
                            tokio::spawn(async move {
                                if let Some(photo) =
                                    photo.photo.into_iter().max_by_key(|p| p.height)
                                {
                                    let path = bot.get_file(&photo.file.id).send().await?.path;
                                    let file_path =
                                        format!("{}/{}.jpg", output_dir(), photo.file.id);
                                    let mut file = fs::File::create(&file_path).await?;
                                    info!("Saving: {}", file_path);
                                    bot.download_file(&path, &mut file).await?;
                                    let msg_id = bot
                                        .send_photo(msg.chat.id, InputFile::file_id(photo.file.id))
                                        .await?
                                        .id
                                        .0
                                        .to_ne_bytes();
                                    let msgs =
                                        db.open_tree("msgs").context("Failed to open msg tree")?;
                                    msgs.insert(msg_id, file_path.as_bytes())
                                        .context("Failed to save msg_id")?;
                                    info!("Saved: {}", file_path);
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
        .branch(dptree::endpoint(async || Ok(())));

    let react_handler = Update::filter_message_reaction_updated().branch(
        case![State::Working].endpoint(async |bot: Bot, update: Update, db: Db| {
            if let UpdateKind::MessageReaction(reaction) = update.kind {
                let msg_id = reaction.message_id.0.to_ne_bytes();
                let msgs = db.open_tree("msgs").context("Failed to open msg tree")?;
                if let Ok(Some(file_path)) = msgs.get(msg_id) {
                    let file_path = std::path::Path::new(std::str::from_utf8(&file_path)?);
                    let file_name = file_path
                        .file_name()
                        .and_then(|file| file.to_str())
                        .context("Failed to read filename from db")?;

                    if let Some(ReactionType::Emoji { emoji }) = reaction.new_reaction.first() {
                        match emoji.as_str() {
                            "👍" | "❤" => {
                                let target_path = format!("{}/{}", fav_dir(), file_name);
                                fs::rename(file_path, &target_path).await?;
                                msgs.insert(msg_id, target_path.as_bytes())
                                    .context("Failed to update msg and file path")?;
                                info!("Fav: {}", target_path);
                            }
                            "👎" => {
                                fs::remove_file(&file_path).await?;
                                msgs.remove(msg_id).context("Failed to remove msg_id")?;
                                bot.delete_message(reaction.chat.id, reaction.message_id)
                                    .await?;
                                info!("Delete: {}", file_path.display());
                            }
                            _ => {}
                        }
                    } else if let Some(ReactionType::Emoji { emoji }) =
                        reaction.old_reaction.first()
                    {
                        if matches!(emoji.as_str(), "👍" | "❤") {
                            let target_path = format!("{}/{}", output_dir(), file_name);
                            fs::rename(file_path, &target_path).await?;
                            msgs.insert(msg_id, target_path.as_bytes())
                                .context("Failed to update msg and file path")?;
                            info!("Unfav: {}", target_path);
                        }
                    }
                }
            }
            Ok(())
        }),
    );

    let callback_query_handler = Update::filter_callback_query();

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(react_handler)
        .branch(callback_query_handler)
}
