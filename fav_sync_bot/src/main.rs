use anyhow::{Context, Result};
use log::{debug, error, info};
use reqwest::Client;
use std::sync::OnceLock;
use teloxide::{
    dispatching::{
        UpdateHandler,
        dialogue::{self, InMemStorage},
    },
    macros::BotCommands,
    prelude::*,
    types::{MediaKind, MessageKind},
    utils::command::BotCommands as _,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = run().await {
        error!("Error: {:?}", e);
    }
}

fn init() -> Result<()> {
    pretty_env_logger::env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
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

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(Client::new)
}

async fn run() -> Result<()> {
    init()?;

    let bot = Bot::from_env();
    Dispatcher::builder(bot, handler())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            UserId(
                std::env::var("OWNER_ID")
                    .unwrap()
                    .parse()
                    .context("INVALID OWNER_ID")
                    .unwrap(),
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
            case![State::Working].endpoint(async |bot: Bot, msg: Message| {
                if let MessageKind::Common(msg) = msg.kind {
                    match msg.media_kind {
                        MediaKind::Text(text) => {
                            debug!("Text: {:#?}", text);
                        }
                        MediaKind::Video(video) => {
                            let p = bot.get_file(video.video.file.id).send().await?.path;
                            let url = format!(
                                "https://api.telegram.org/file/bot{token}/{p}",
                                token = bot.token()
                            );
                            save(url).await?;
                        }
                        MediaKind::Photo(photo) => {
                            if let Some(p) = photo.photo.into_iter().max_by_key(|p| p.height) {
                                let p = bot.get_file(p.file.id).send().await?.path;
                                let url = format!(
                                    "https://api.telegram.org/file/bot{token}/{p}",
                                    token = bot.token()
                                );
                                save(url).await?;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }),
        )
        .branch(dptree::endpoint(async || Ok(())));

    let callback_query_handler = Update::filter_callback_query();

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
}

async fn save(url: String) -> Result<()> {
    info!("Downloading: {}", url);
    let res = client().get(&url).send().await?;
    let bytes = res.bytes().await?;
    let ext = url
        .split(".")
        .last()
        .map(|ext| format!(".{}", ext))
        .unwrap_or_default();
    let filename = sha256::digest(&bytes[..]);
    tokio::fs::write(format!("{}/{}{}", output_dir(), filename, ext), bytes).await?;
    info!("Saved: {}", filename);
    Ok(())
}
