use super::state::TransportState;
use crate::context::Context;
use crate::utils::cp_from_container;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use teloxide::Bot;
use teloxide::net::Download as _;
use teloxide::prelude::{Request as _, Requester as _};
use tokio::fs;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct TransportHandle {
    state: Arc<RwLock<TransportState>>,
    cancel: CancellationToken,
}

impl TransportHandle {
    fn new() -> Self {
        TransportHandle {
            state: Arc::new(RwLock::new(TransportState::default())),
            cancel: CancellationToken::new(),
        }
    }

    pub fn get_state(&self) -> TransportState {
        self.state.read().unwrap().clone()
    }

    fn set_state(&self, state: TransportState) {
        *self.state.write().unwrap() = state;
    }

    pub(super) fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn result(&self) -> TransportState {
        self.cancel.cancelled().await;
        self.get_state()
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn cancelled(&self) -> impl Future<Output = ()> {
        self.cancel.cancelled()
    }
}

pub type FileId = String;
pub type FileName = String;

#[derive(Debug, Clone)]
pub struct Downloader {
    downloads: Arc<RwLock<HashMap<FileId, TransportHandle>>>,
    tx: Sender<Message>,
    jh: Arc<RwLock<Option<JoinHandle<()>>>>,
}

enum Message {
    Add(FileId, FileName, TransportHandle),
    Cancel(FileId),
    Shutdown,
}

impl Downloader {
    pub(super) fn new(bot: Bot, context: Context) -> Self {
        let (tx, rx) = channel::<Message>();
        let jh = std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let cancels = Arc::new(RwLock::new(HashMap::new()));
                    while let Ok(msg) = rx.recv() {
                        match msg {
                            Message::Add(file_id, file_name, handle) => {
                                let bot = bot.clone();
                                let context = context.clone();
                                if let Some(old) = cancels.write().unwrap().insert(file_id.clone(), handle.cancel.clone()) {
                                    old.cancel();
                                };
                                async fn download(
                                    bot: Bot,
                                    file_id: FileId,
                                    file_name: FileName,
                                    context: Context,
                                    handle: TransportHandle
                                ) -> Result<()> {
                                    handle.set_state(TransportState::Downloading);
                                    let save_path = context.output_dir.join(file_name);
                                    let server_path = loop {
                                        if let Ok(f) = bot.get_file(&file_id).send().await {
                                            break PathBuf::from(f.path);
                                        }
                                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    };
                                    // download file
                                    match context.local_server {
                                        false => {
                                            let mut file = fs::File::create(&save_path).await?;
                                            bot.download_file(
                                                server_path.to_string_lossy().as_ref(),
                                                &mut file,
                                            )
                                            .await?;
                                        }
                                        true => match &context.container_manager {
                                            Some(container_manager) => {
                                                cp_from_container(
                                                    container_manager,
                                                    context.container_id.as_ref().unwrap(),
                                                    server_path,
                                                    save_path,
                                                )
                                                .await?;
                                            }
                                            None => {
                                                fs::copy(server_path, &save_path).await?;
                                            }
                                        },
                                    }
                                    Ok(())
                                }
                                let cancels_c = cancels.clone();
                                tokio::spawn(async move {
                                    tokio::select! {
                                        res = download(bot, file_id.clone(), file_name, context, handle.clone()) => {
                                            match res {
                                                Ok(_) => handle.set_state(TransportState::Completed),
                                                Err(_) => handle.set_state(TransportState::Failed),
                                            }
                                            handle.cancel(); // when downloading, await cancel.cancelled() avoiding loop checking
                                        },
                                        _ = handle.cancelled() => {
                                            handle.set_state(TransportState::Cancelled);
                                        },
                                    };
                                    cancels_c.write().unwrap().remove(&file_id);
                                });
                            }
                            Message::Cancel(k) => {
                                if let Some(cancel) = cancels.write().unwrap().remove(&k) {
                                    cancel.cancel();
                                }
                            }
                            Message::Shutdown => {
                                for (_, cancel) in cancels.read().unwrap().iter() {
                                    cancel.cancel();
                                }
                                break;
                            }
                        }
                    }
                })
        });
        let tx_c = tx.clone();
        let _listener = tokio::spawn(async move {
            tokio::signal::ctrl_c().await?;
            tx_c.send(Message::Shutdown).unwrap();
            Ok::<_, anyhow::Error>(())
        });
        Downloader {
            downloads: Arc::new(RwLock::new(HashMap::new())),
            tx,
            jh: Arc::new(RwLock::new(Some(jh))),
        }
    }

    #[must_use = "Caller should refresh db state with this handle"]
    pub(super) fn add(&self, file_id: FileId, file_name: FileName) -> TransportHandle {
        match self.downloads.read().unwrap().get(&file_id) {
            // the ReadGuard is dropped here only with Rust 2024
            Some(handle) if !handle.is_cancelled() => handle.clone(),
            _ => {
                let handle = TransportHandle::new();
                self.tx
                    .send(Message::Add(file_id.clone(), file_name, handle.clone()))
                    .unwrap();
                self.downloads
                    .write()
                    .unwrap()
                    .insert(file_id, handle.clone());
                handle
            }
        }
    }

    fn shutdown(&self) {
        self.tx.send(Message::Shutdown).unwrap();
        if let Some(jh) = self.jh.write().unwrap().take() {
            jh.join().unwrap();
        }
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        self.shutdown();
    }
}
