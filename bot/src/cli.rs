use crate::{context::Context, utils::gen_pwd};
use anyhow::{Context as _, Result, anyhow};
use clap::Parser;
use log::info;
use std::{
    collections::HashSet,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, RwLock},
};
use teloxide::{Bot, types::UserId};

#[derive(Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    /// The directory to store the files.
    #[arg(short, long, default_value = ".")]
    output: PathBuf,
    /// The url if you are using a local server.
    #[arg(short, long)]
    local_server_url: Option<String>,
    /// The container manager to use if deploying server in a container.
    #[arg(short, long)]
    container_manager: Option<String>,
    /// The container id or name if deploying server in a container.
    #[arg(short = 'i', long)]
    container_id: Option<String>,
    /// Score limit to favorite a file, > 0 (channel only).
    #[arg(short, long, default_value = "10")]
    fav_score_limit: i32,
    /// Score limit to delete a file, < 0 (channel only, e.g `-d-10`).
    #[arg(short, long, default_value = "-10")]
    delete_score_limit: i32,
}

fn init() -> Result<()> {
    pretty_env_logger::env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("sled", log::LevelFilter::Info)
        .init();
    dotenv::dotenv().ok();
    info!(
        "TELOXIDE_TOKEN: {}",
        std::env::var("TELOXIDE_TOKEN").context("TELOXIDE_TOKEN unset")?
    );
    Ok(())
}

impl Cli {
    pub fn init() -> Result<(Bot, Context)> {
        let args = Self::parse();
        if args.fav_score_limit < 0 || args.delete_score_limit > 0 {
            return Err(anyhow!("Invalid score limit"));
        }
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
            bypass_pwd: Arc::new(RwLock::new(gen_pwd())),
            bypass_users: {
                let mut allow_users = HashSet::new();
                for id in std::env::var("BYPASS_USERS")
                    .context("BYPASS_USERS unset")?
                    .split(',')
                {
                    allow_users.insert(UserId(id.parse()?));
                }
                allow_users
            },
            fav_dir: {
                let fav_dir = args.output.join("favorite");
                std::fs::create_dir_all(&fav_dir)?;
                fav_dir
            },
            db: sled::open(args.output.join("data.sled")).context("Failed to open db")?,
            output_dir: args.output,
            fav_score_limit: args.fav_score_limit,
            delete_score_limit: args.delete_score_limit,
        };
        info!("Context: {}", context);
        let bot = Bot::from_env();
        if let Some(url) = args.local_server_url {
            Ok((
                bot.set_api_url(url.parse().context("Failed to parse local server url")?),
                context,
            ))
        } else {
            Ok((bot, context))
        }
    }
}
