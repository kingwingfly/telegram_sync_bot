use super::transport::TransportHandle;
use super::{FileId, FileName};
use super::{db::Db, state::*, transport::Downloader};
use crate::context::Context;
use anyhow::{Result, anyhow};
use std::sync::{Arc, RwLock};
use teloxide::Bot;
use teloxide::types::{ChatId, MessageId};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct MyStorage {
    db: Db,
    downloader: Downloader,
    jhs: Arc<RwLock<Vec<JoinHandle<Result<()>>>>>,
}

impl MyStorage {
    pub async fn new(database_url: impl AsRef<str>, bot: Bot, context: Context) -> Result<Self> {
        let db = Db::new(database_url).await?;
        let downloader = Downloader::new(bot, context);
        Ok(Self {
            db,
            downloader,
            jhs: Arc::new(RwLock::new(Vec::new())),
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
            .get_file_state_by_handle((handle.0.0, handle.1.0))
            .await
    }

    pub async fn set_file_state_by_handle(
        &self,
        handle: (ChatId, MessageId),
        state: FileState,
    ) -> Result<()> {
        self.db
            .set_file_state_by_handle((handle.0.0, handle.1.0), state)
            .await
    }
}

impl MyStorage {
    /// add a download task, return a handle
    pub async fn add_task(&self, file_id: FileId, file_name: FileName) -> Result<TransportHandle> {
        if matches!(
            self.db.get_transport_state(&file_id).await,
            Ok(TransportState::Completed)
        ) {
            return Err(anyhow!("Dumplcated save"));
        }
        let handle = self.downloader.add(file_id.clone(), file_name);
        let db = self.db.clone();
        let handle_c = handle.clone();
        self.jhs.write().unwrap().push(tokio::spawn(async move {
            while TransportState::Pending == handle_c.get_state() {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
            db.set_transport_state(file_id.clone(), handle_c.get_state())
                .await?;
            db.set_transport_state(file_id, handle_c.result().await)
                .await?;
            Ok(())
        }));
        Ok(handle)
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
}

impl Drop for MyStorage {
    fn drop(&mut self) {
        for jh in self.jhs.write().unwrap().iter() {
            jh.abort();
        }
    }
}
