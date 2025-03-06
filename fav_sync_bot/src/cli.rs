use crate::context::Context;
use anyhow::{Context as _, Result, anyhow};
use clap::Parser;
use log::info;
use std::{path::PathBuf, process::Stdio};
use teloxide::Bot;

#[derive(Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    /// The directory to store the files
    #[arg(short, long, default_value = ".")]
    output: PathBuf,
    /// The url if you are using a local server
    #[arg(short, long)]
    local_server_url: Option<String>,
    /// The container manager to use if deploying server in a container
    #[arg(short, long)]
    container_manager: Option<String>,
    /// The container id or name if deploying server in a container
    #[arg(short = 'i', long)]
    container_id: Option<String>,
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

impl Cli {
    pub fn init() -> Result<(Bot, Context)> {
        let args = Self::parse();
        init()?;
        let context = Context {
            local_server: { args.local_server_url.is_some() },
            container_manager: {
                if let Some(c) = &args.container_manager {
                    info!("Checking container manager");
                    if !std::process::Command::new(c)
                        .args([
                            "logs",
                            args.container_id
                                .as_deref()
                                .context("Container Id not provided")?,
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()?
                        .success()
                    {
                        return Err(anyhow!(
                            "Container check not pass: invalid container manager or bad container"
                        ));
                    }
                }
                args.container_manager
            },
            container_id: args.container_id,
            fav_dir: {
                let fav_dir = args.output.join("favorite");
                std::fs::create_dir_all(&fav_dir)?;
                fav_dir
            },
            db: sled::open(args.output.join("data.sled")).context("Failed to open db")?,
            output_dir: args.output,
        };
        let bot = Bot::from_env();
        if let Some(url) = args.local_server_url {
            Ok((
                bot.set_api_url(url.parse().context("Failed to parse url")?),
                context,
            ))
        } else {
            Ok((bot, context))
        }
    }
}
