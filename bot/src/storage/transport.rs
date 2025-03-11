use super::state::TransportState;
use crate::context::Context;
use crate::utils::cp_from_container;
use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use teloxide::Bot;
use teloxide::net::Download as _;
use teloxide::prelude::{Request as _, Requester as _};
use tokio::fs;
use tokio_util::sync::CancellationToken;
use tracing::info;

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
        *self.state.read()
    }

    fn set_state(&self, state: TransportState) {
        *self.state.write() = state;
    }

    pub(super) fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn result(&self) -> TransportState {
        self.cancel.cancelled().await;
        self.get_state()
    }
}

pub type FileId = String;
pub type FileName = String;

#[derive(Debug)]
pub struct Downloader {
    downloads: RwLock<HashMap<FileId, TransportHandle>>,
    tx: Sender<Message>,
    jh: RwLock<Option<JoinHandle<()>>>,
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
                                if let Some(old) = cancels.write().insert(file_id.clone(), handle.cancel.clone()) {
                                    info!(">> DOWNLOADER: dumplicated, cancel old task {}", file_id);
                                    old.cancel();
                                };
                                async fn download(
                                    bot: Bot,
                                    file_id: FileId,
                                    file_name: FileName,
                                    context: Context,
                                    handle: TransportHandle
                                ) -> Result<()> {
                                    info!(">> DOWNLOADER: start task {}", file_id);
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
                                    info!(">> DOWNLOADER: finish task {}", file_id);
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
                                        _ = handle.cancel.cancelled() => {
                                            info!(">> DOWNLOADER: task cancelled {}", file_id);
                                            handle.set_state(TransportState::Cancelled);
                                        },
                                    };
                                    cancels_c.write().remove(&file_id);
                                });
                            }
                            Message::Cancel(k) => {
                                if let Some(cancel) = cancels.write().remove(&k) {
                                    cancel.cancel();
                                }
                            }
                            Message::Shutdown => {
                                info!(">> DOWNLOADER: shutdown");
                                for (_, cancel) in cancels.read().iter() {
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
            tx_c.send(Message::Shutdown).ok();
            Ok::<_, anyhow::Error>(())
        });
        Downloader {
            downloads: RwLock::new(HashMap::new()),
            tx,
            jh: RwLock::new(Some(jh)),
        }
    }

    #[must_use = "Caller should refresh db state with this handle"]
    pub(super) fn add(&self, file_id: FileId, file_name: FileName) -> TransportHandle {
        let read = self.downloads.read();
        match read.get(&file_id).cloned() {
            Some(handle)
                if matches!(
                    handle.get_state(),
                    TransportState::Downloading
                        | TransportState::Pending
                        | TransportState::Completed
                ) =>
            {
                handle
            }
            _ => {
                drop(read);
                let handle = TransportHandle::new();
                self.tx
                    .send(Message::Add(file_id.clone(), file_name, handle.clone()))
                    .unwrap();
                if let Some(old_handle) = self.downloads.write().insert(file_id, handle.clone()) {
                    old_handle.cancel();
                }
                handle
            }
        }
    }

    pub(super) fn cancel(&self, file_id: FileId) {
        self.tx.send(Message::Cancel(file_id)).unwrap();
    }

    fn shutdown(&self) {
        self.tx.send(Message::Shutdown).ok();
        if let Some(jh) = self.jh.write().take() {
            jh.join().unwrap();
        }
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        self.shutdown();
    }
}
