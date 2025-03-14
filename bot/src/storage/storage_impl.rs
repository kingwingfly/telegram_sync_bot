use super::transport::TransportHandle;
use super::{FileId, FileName};
use super::{db::Db, state::*, transport::Downloader};
use crate::context::Context;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use teloxide::Bot;
use teloxide::types::{ChatId, MessageId};
use tokio::fs;
use tracing::{info, instrument};

#[derive(Debug, Clone)]
pub struct MyStorage {
    db: Db,
    downloader: Arc<Downloader>,
    context: Context,
}

impl MyStorage {
    pub async fn new(database_url: impl AsRef<str>, bot: Bot, context: Context) -> Result<Self> {
        let db = Db::new(database_url).await?;
        let downloader = Arc::new(Downloader::new(bot, context.clone()));
        Ok(Self {
            db,
            downloader,
            context,
        })
    }
}

impl MyStorage {
    pub async fn get_chat_state(&self, chat_id: ChatId) -> Result<ChatState> {
        self.db.get_chat_state(chat_id.0).await
    }

    pub async fn troggle_chat_state(&self, chat_id: ChatId) -> Result<ChatState> {
        self.db.troggle_chat_state(chat_id.0).await
    }

    pub async fn get_file_state_by_handle(&self, handle: (ChatId, MessageId)) -> Result<FileState> {
        self.db
            .get_file_state_and_name_by_handle((handle.0.0, handle.1.0))
            .await
            .map(|state_name| state_name.0)
    }

    /// 1. hard link file to correct directory
    /// 2. set file state by handle
    #[instrument(level = "debug", fields(file_name, old_state))]
    pub async fn set_file_state_by_handle_and_link(
        &self,
        handle: (ChatId, MessageId),
        state: FileState,
    ) -> Result<()> {
        let (old_state, file_name) = self
            .db
            .get_file_state_and_name_by_handle((handle.0.0, handle.1.0))
            .await?;

        tracing::Span::current().record("file_name", &file_name);
        tracing::Span::current().record("old_state", old_state.to_string());

        let dir = self.context.data_dir.join(handle.0.to_string());
        let new_dir = dir.join(state.to_string().to_lowercase());
        fs::create_dir_all(&new_dir).await?;
        let from = dir
            .join(old_state.to_string().to_lowercase())
            .join(&file_name);
        let to = new_dir.join(&file_name);
        match fs::rename(&from, &to).await {
            Ok(_) => {
                info!(
                    ">> STORAGE: moved file from {} to {}",
                    from.display(),
                    to.display()
                );
                self.db
                    .set_file_state_by_handle_returning_old_state((handle.0.0, handle.1.0), state)
                    .await?;
            }
            Err(_) => {
                let origin = self.context.data_dir.join(&file_name);
                let db = self.db.clone();
                tokio::spawn(async move {
                    for _ in 0..32768 {
                        // waiting at most 27 hours or so, maybe is still downloading
                        if fs::try_exists(&origin).await? {
                            fs::hard_link(origin, &to).await?;
                            if from != to {
                                fs::remove_file(from).await.ok();
                            }
                            db.set_file_state_by_handle_returning_old_state(
                                (handle.0.0, handle.1.0),
                                state,
                            )
                            .await?;
                            return Ok::<_, anyhow::Error>(());
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                    Err(anyhow!("File not exists {}", file_name))
                });
            }
        }

        Ok(())
    }

    pub async fn get_handle_by_file_id(
        &self,
        file_id: FileId,
    ) -> Result<Option<(ChatId, MessageId)>> {
        Ok(self
            .db
            .get_handle_by_file_id(file_id)
            .await?
            .map(|(chat_id, msg_id)| (ChatId(chat_id), MessageId(msg_id))))
    }

    pub async fn get_file_ids_by_name(&self, file_name: String) -> Result<Vec<FileId>> {
        self.db.get_file_ids_by_name(file_name).await
    }

    /// set file_handle for file_id, return old handle if exists
    pub async fn set_file_handle(
        &self,
        chat_id: ChatId,
        msg_id: MessageId,
        file_id: FileId,
    ) -> Result<Option<(ChatId, MessageId)>> {
        let old_handle = self
            .db
            .set_file_handle((chat_id.0, msg_id.0), file_id)
            .await?;
        Ok(old_handle.map(|(chat_id, msg_id)| (ChatId(chat_id), MessageId(msg_id))))
    }

    pub async fn delete_file_record(&self, file_id: FileId) -> Result<()> {
        self.db.delete_file_record(file_id).await
    }

    pub async fn delete_handle(&self, handle: (ChatId, MessageId)) -> Result<()> {
        self.db.delete_handle((handle.0.0, handle.1.0)).await
    }
}

impl MyStorage {
    /// add a download task, return a handle
    pub async fn add_task(
        &self,
        file_id: FileId,
        file_name: FileName,
    ) -> Result<Option<TransportHandle>> {
        info!(">> Storage: Add new task {}", file_name);
        if matches!(
            self.db.get_transport_state(file_id.to_string()).await,
            Ok(TransportState::Completed)
        ) {
            info!(">> Storage: already finished");
            return Ok(None);
        }
        self.db
            .set_file_name(file_id.clone(), file_name.clone())
            .await?;
        let handle = self.downloader.add(file_id.clone(), file_name);
        let db = self.db.clone();
        let handle_c = handle.clone();
        tokio::spawn(async move {
            while TransportState::Pending == handle_c.get_state() {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }

            if !handle_c.is_cancelled() {
                db.set_transport_state(file_id.clone(), handle_c.get_state())
                    .await?;
            }

            db.set_transport_state(file_id, handle_c.result().await)
                .await?;
            Ok::<_, anyhow::Error>(())
        });
        Ok(Some(handle))
    }

    /// cancel a download task
    pub async fn cancel_task_by_handle(&self, chat_id: ChatId, msg_id: MessageId) -> Result<()> {
        match self.db.get_file_id_by_handle((chat_id.0, msg_id.0)).await? {
            Some(file_id) => {
                self.downloader.cancel(file_id);
                Ok(())
            }
            None => Err(anyhow!("No such handle")),
        }
    }
}
