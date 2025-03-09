use anyhow::{Context as _, Result};
use log::info;
use sqlx::{Row, SqlitePool, query, sqlite::SqliteConnectOptions};
use std::ops::Deref;
use teloxide::types::{ChatId, MessageId};

const CREATE_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS files (
id INTEGER PRIMARY KEY AUTOINCREMENT,
file_id TEXT NOT NULL,
chat_id BIGINT NOT NULL,
msg_id INTEGER NOT NULL,
path TEXT NOT NULL,
UNIQUE(chat_id, file_id),
UNIQUE(chat_id, msg_id)
);
CREATE TABLE IF NOT EXISTS chats (
chat_id BIGINT PRIMARY KEY
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

    /// get msg_id:path in chat_id
    pub async fn get_mp_pair(
        &self,
        chat_id: ChatId,
        file_id: &str,
    ) -> Result<Option<(MessageId, String)>> {
        query("SELECT msg_id, path FROM files WHERE chat_id = ? AND file_id = ?")
            .bind(chat_id.0)
            .bind(file_id)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get mp pair")
            .map(|row| row.map(|row| (MessageId(row.get(0)), row.get(1))))
    }

    /// get current path of chat_id/msg_id
    pub async fn get_path(&self, chat_id: ChatId, msg_id: MessageId) -> Result<Option<String>> {
        query("SELECT path FROM files WHERE chat_id = ? AND msg_id = ?")
            .bind(chat_id.0)
            .bind(msg_id.0)
            .fetch_optional(&self.db)
            .await
            .context("Failed to get file path")
            .map(|row| row.map(|row| row.get(0)))
    }

    pub async fn insert_mp_pair(
        &self,
        chat_id: ChatId,
        file_id: &str,
        msg_id: MessageId,
        path: &str,
    ) -> Result<()> {
        query("INSERT INTO files (file_id, chat_id, msg_id, path) VALUES (?, ?, ?, ?)")
            .bind(file_id)
            .bind(chat_id.0)
            .bind(msg_id.0)
            .bind(path)
            .execute(&self.db)
            .await
            .context("Failed to insert mp pair")
            .map(|_| ())
    }

    /// update msg_id:path in chat_id
    pub async fn update_mp_pair(
        &self,
        chat_id: ChatId,
        msg_id: MessageId,
        path: &str,
    ) -> Result<()> {
        query("UPDATE files SET path = ?, msg_id = ? WHERE chat_id = ?")
            .bind(path)
            .bind(msg_id.0)
            .bind(chat_id.0)
            .execute(&self.db)
            .await
            .context("Failed to update mp pair")
            .map(|_| ())
    }
}
