use anyhow::{Context as _, Result, anyhow};
use log::info;
use sqlx::{Row, SqlitePool, query, sqlite::SqliteConnectOptions};
use std::{ops::Deref, path::Path};
use teloxide::types::{ChatId, MessageId};

const CREATE_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id BIGINT NOT NULL,
    file_id TEXT NOT NULL,
    msg_id INTEGER NOT NULL,
    UNIQUE(chat_id, file_id),
    UNIQUE(chat_id, msg_id)
);
CREATE TABLE IF NOT EXISTS paths (
    file_id TEXT PRIMARY KEY,
    path TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS chats (
    chat_id BIGINT PRIMARY KEY,
    syncing BOOLEAN DEFAULT TRUE
);"#;

#[derive(Debug, Clone)]
pub struct MyStorage {
    db: SqlitePool,
}

impl Deref for MyStorage {
    type Target = SqlitePool;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl MyStorage {
    pub async fn new(filename: String) -> Self {
        info!("Create sqlite: {}", filename);
        let db = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(filename)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        let mut tx = db.begin().await.unwrap();
        sqlx::query(CREATE_TABLES)
            .execute(&mut *tx)
            .await
            .context("Failed to create tables")
            .unwrap();
        tx.commit().await.unwrap();
        Self { db }
    }

    pub async fn get_chat_state(&self, chat_id: ChatId) -> bool {
        query("SELECT 1 FROM chats WHERE chat_id = ?")
            .bind(chat_id.0)
            .fetch_one(&self.db)
            .await
            .map(|_| true)
            .unwrap_or(false)
    }

    /// return true if switched to working
    pub async fn switch_chat_state(&self, chat_id: ChatId) -> Result<bool> {
        let chat_state = self.get_chat_state(chat_id).await;
        if chat_state {
            query("DELETE FROM chats WHERE chat_id = ?")
                .bind(chat_id.0)
                .execute(&self.db)
                .await
                .context("Failed to delete chat state")?;
        } else {
            query("INSERT INTO chats (chat_id) VALUES (?)")
                .bind(chat_id.0)
                .execute(&self.db)
                .await
                .context("Failed to insert chat state")?;
        }
        Ok(!chat_state)
    }

    /// get syncing state
    pub async fn get_syncing_state(&self, chat_id: ChatId) -> bool {
        query("SELECT syncing FROM chats WHERE chat_id = ?")
            .bind(chat_id.0)
            .fetch_one(&self.db)
            .await
            .map(|row| row.get("syncing"))
            .context("Failed to get syncing state")
            .unwrap_or_default()
    }

    /// troggle syncing state
    pub async fn troggle_syncing(&self, chat_id: ChatId) -> Result<bool> {
        let syncing = self.get_syncing_state(chat_id).await;
        query("UPDATE chats SET syncing = NOT syncing WHERE chat_id = ?")
            .bind(chat_id.0)
            .execute(&self.db)
            .await
            .context("Failed to troggle syncing state")?;
        Ok(!syncing)
    }

    /// get file_id for msg_id in chat_id
    pub async fn get_file_id(&self, chat_id: ChatId, msg_id: MessageId) -> Result<Option<String>> {
        query("SELECT file_id FROM files WHERE chat_id = ? AND msg_id = ?")
            .bind(chat_id.0)
            .bind(msg_id.0)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get file_id")
            .map(|row| row.map(|row| row.get("file_id")))
    }

    /// get msg_id for file_id in chat_id
    pub async fn get_msg_id(&self, chat_id: ChatId, file_id: &str) -> Result<Option<MessageId>> {
        query("SELECT msg_id FROM files WHERE chat_id = ? AND file_id = ?")
            .bind(chat_id.0)
            .bind(file_id)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get msg_id")
            .map(|row| row.map(|row| MessageId(row.get("msg_id"))))
    }

    /// get path for file_id of chat_id and msg_id
    pub async fn get_path(&self, chat_id: ChatId, msg_id: MessageId) -> Result<Option<String>> {
        let Some(file_id) = self.get_file_id(chat_id, msg_id).await? else {
            return Ok(None);
        };
        query("SELECT path FROM paths WHERE file_id = ?")
            .bind(file_id)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get path")
            .map(|row| match row.map(|row| row.get("path")) {
                Some(path) if Path::new(&path).exists() => Some(path),
                _ => None,
            })
    }

    /// get path for file_id
    pub async fn get_path_by_file_id(&self, file_id: &str) -> Result<Option<String>> {
        query("SELECT path FROM paths WHERE file_id = ?")
            .bind(file_id)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get path")
            .map(|row| match row.map(|row| row.get("path")) {
                Some(path) if Path::new(&path).exists() => Some(path),
                _ => None,
            })
    }

    /// update path for file_id of chat_id and msg_id
    pub async fn update_path(&self, chat_id: ChatId, msg_id: MessageId, path: &str) -> Result<()> {
        let Some(file_id) = self.get_file_id(chat_id, msg_id).await? else {
            return Err(anyhow!("Failed to update path"));
        };
        self.insert_or_replace_path(&file_id, path).await
    }

    /// insert msg_id for file_id in chat_id
    pub async fn insert_or_replace_files(
        &self,
        chat_id: ChatId,
        file_id: &str,
        msg_id: MessageId,
    ) -> Result<()> {
        query("INSERT OR REPLACE INTO files (chat_id, file_id, msg_id) VALUES (?, ?, ?)")
            .bind(chat_id.0)
            .bind(file_id)
            .bind(msg_id.0)
            .execute(&self.db)
            .await
            .context("Failed to insert file")?;
        Ok(())
    }

    /// insert or replace path for file_id
    pub async fn insert_or_replace_path(&self, file_id: &str, path: &str) -> Result<()> {
        query("INSERT OR REPLACE INTO paths (file_id, path) VALUES (?, ?)")
            .bind(file_id)
            .bind(path)
            .execute(&self.db)
            .await
            .context("Failed to insert or replace path")?;
        Ok(())
    }
}
