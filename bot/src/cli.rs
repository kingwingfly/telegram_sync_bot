use crate::handler::handler;
use crate::{
    context::{Context, ContextInner},
    storage::MyStorage,
    utils::gen_key,
};
use anyhow::{Context as _, Result, anyhow};
use clap::{Parser, Subcommand};
use parking_lot::RwLock;
use std::{collections::HashSet, path::PathBuf, process::Stdio, sync::Arc};
use teloxide::{Bot, types::UserId};
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};
use tokio::fs;
use tokio::task::JoinHandle;
use tracing::{error, info};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    #[clap(subcommand)]
    subcmd: SubCmd,
}

#[derive(Debug, Subcommand)]
enum SubCmd {
    /// Run the bot.
    Run {
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
        /// If score >= limit, fav a file, limit >= 0 (channel only).
        #[arg(short, long, default_value = "10")]
        fav_score_limit: i32,
        /// If score < limit, delete a file, limit <= 0 (channel only, e.g `-d-10`).
        #[arg(short, long, default_value = "-10")]
        delete_score_limit: i32,
    },
    /// Delete files by file_name in the output dir, and delete the record in the database,
    /// delete the message in the channel. The database should not be locked by other process,
    /// and there should not be any other bot instance.
    Delete {
        /// The directory to store the files.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// The url if you are using a local server.
        #[arg(short, long)]
        local_server_url: Option<String>,
        /// The file name to delete.
        file_names: Vec<String>,
    },
}

fn init() -> Result<()> {
    dotenv::dotenv().ok();
    // mask token
    let token = std::env::var("TELOXIDE_TOKEN")
        .context("TELOXIDE_TOKEN unset")?
        .chars()
        .enumerate()
        .fold("".to_string(), |mut acc, (i, c)| {
            if i < 5 {
                acc.push(c);
            } else {
                acc.push('*');
            }
            acc
        });
    info!(">> INIT: TELOXIDE_TOKEN: {}", token);
    Ok(())
}

impl Cli {
    pub async fn run() -> Result<()> {
        let args = Self::parse();
        init()?;
        match args.subcmd {
            SubCmd::Run {
                output,
                local_server_url,
                container_manager,
                container_id,
                fav_score_limit,
                delete_score_limit,
            } => {
                if fav_score_limit < 0 || delete_score_limit > 0 {
                    return Err(anyhow!("Invalid score limit"));
                }
                let context = Context {
                    inner: Arc::new(ContextInner {
                        local_server: { local_server_url.is_some() },
                        container_manager: {
                            if let Some(c) = &container_manager {
                                info!(">> INIT: checking container manager");
                                if !std::process::Command::new(c)
                                    .args([
                                        "logs",
                                        container_id
                                            .as_deref()
                                            .context("Container Id not provided")?,
                                    ])
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status()?
                                    .success()
                                {
                                    return Err(anyhow!(
                                        "Container check not pass: invalid container manager or unsupport container"
                                    ));
                                }
                            }
                            container_manager
                        },
                        container_id,
                        bypasskey: RwLock::new(gen_key()),
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
                        output_dir: {
                            std::fs::create_dir_all(&output)?;
                            output
                        },
                        fav_score_limit,
                        delete_score_limit,
                    }),
                };
                info!(">> INIT: {}", context);
                let mut bot = Bot::from_env();
                if let Some(url) = local_server_url {
                    bot = bot.set_api_url(url.parse().context("Failed to parse local server url")?);
                }
                let storage = MyStorage::new(
                    format!("sqlite://{}/data.db?mode=rwc", context.output_dir.display()),
                    bot.clone(), // used to download files
                    context.clone(),
                )
                .await?;
                let mut dispatcher = Dispatcher::builder(bot, handler())
                    .dependencies(dptree::deps![InMemStorage::<()>::new(), context, storage])
                    .build();
                let shutdown = dispatcher.shutdown_token();
                let listener: JoinHandle<Result<()>> = tokio::spawn(async move {
                    tokio::signal::ctrl_c().await?;
                    shutdown.shutdown()?.await;
                    info!(">> BOT: shutdown");
                    Ok(())
                });
                dispatcher.dispatch().await;
                listener.await??;
                Ok(())
            }
            SubCmd::Delete {
                output,
                local_server_url,
                file_names,
            } => {
                let context = Context {
                    inner: Arc::new(ContextInner {
                        local_server: { local_server_url.is_some() },
                        container_manager: None,
                        container_id: None,
                        bypasskey: RwLock::new(gen_key()),
                        bypass_users: HashSet::new(),
                        output_dir: {
                            std::fs::create_dir_all(&output)?;
                            output
                        },
                        fav_score_limit: 0,
                        delete_score_limit: 0,
                    }),
                };
                info!(">> INIT: {}", context);
                let mut bot = Bot::from_env();
                if let Some(url) = local_server_url {
                    bot = bot.set_api_url(url.parse().context("Failed to parse local server url")?);
                }
                let storage = MyStorage::new(
                    format!("sqlite://{}/data.db?mode=rwc", context.output_dir.display()),
                    bot.clone(), // used to download files
                    context.clone(),
                )
                .await?;
                for file_name in file_names {
                    let file_ids = storage.get_file_ids_by_name(file_name.to_owned()).await?;
                    for file_id in file_ids {
                        if let Some((chat_id, msg_id)) =
                            storage.get_handle_by_file_id(file_id.to_owned()).await?
                        {
                            bot.delete_message(chat_id, msg_id).send().await.ok();
                            info!(">> BOT: delete message {:?}", (chat_id, msg_id));
                            storage.delete_file_record(file_id).await.ok();
                            for jh in WalkDir::new(&context.output_dir)
                                .into_iter()
                                .filter_map(|p| p.ok())
                                .filter(|e| e.file_name().to_str() == Some(&file_name))
                                .map(|e| {
                                    tokio::spawn(async move {
                                        fs::remove_file(e.path()).await?;
                                        info!(">> STOREAGE: delete file {}", e.path().display());
                                        Ok::<_, anyhow::Error>(())
                                    })
                                })
                                .collect::<Vec<_>>()
                                .into_iter()
                            {
                                if let Err(e) = jh.await? {
                                    error!(">> Storage: {}", e);
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
}
