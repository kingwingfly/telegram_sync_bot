use super::state::*;
use anyhow::Result;
use entity::{chat_state, file_handle, file_state};
use log::info;
use migration::{Migrator, MigratorTrait};
use sea_orm::ActiveValue::*;
use sea_orm::prelude::*;
use sea_orm::{Database, DatabaseConnection};
use teloxide::types::ChatId;
use teloxide::types::MessageId;

pub(super) async fn establish_connection(
    database_url: impl AsRef<str>,
) -> Result<DatabaseConnection> {
    info!("Connecting to database");
    let connection = Database::connect(database_url.as_ref()).await?;
    #[cfg(debug_assertions)]
    Migrator::refresh(&connection).await?;
    #[cfg(not(debug_assertions))]
    Migrator::up(&connection, None).await?;

    info!("Connected to database");
    Ok(connection)
}

#[derive(Debug, Clone)]
pub(super) struct Db {
    db: DatabaseConnection,
}

impl Db {
    pub(super) async fn new(database_url: impl AsRef<str>) -> Result<Self> {
        let db = establish_connection(database_url).await?;
        Ok(Self { db })
    }
}

impl Db {
    pub(super) async fn get_chat_state(&self, chat_id: i64) -> Result<ChatState> {
        match chat_state::Entity::find_by_id(chat_id)
            .one(&self.db)
            .await?
        {
            Some(m) => Ok(m.state.into()),
            None => Ok(ChatState::default()),
        }
    }

    pub(super) async fn troggle_chat_state(&self, chat_id: i64) -> Result<ChatState> {
        let current_state = self.get_chat_state(chat_id).await?;

        let new_state = current_state.troggle();
        chat_state::ActiveModel {
            chat_id: Unchanged(chat_id),
            state: Set(new_state.to_string()),
        }
        .save(&self.db)
        .await?;

        Ok(new_state)
    }

    pub(super) async fn get_transport_state(
        &self,
        file_id: impl AsRef<str>,
    ) -> Result<TransportState> {
        match file_state::Entity::find_by_id(file_id.as_ref().to_string())
            .one(&self.db)
            .await?
        {
            Some(m) => Ok(m.transport_state.into()),
            None => Err(anyhow::anyhow!("File not found")),
        }
    }

    pub(super) async fn set_transport_state(
        &self,
        file_id: impl AsRef<str>,
        tranport_state: TransportState,
    ) -> Result<()> {
        file_state::ActiveModel {
            file_id: Unchanged(file_id.as_ref().to_string()),
            transport_state: Set(tranport_state.to_string()),
            ..Default::default()
        }
        .save(&self.db)
        .await?;

        Ok(())
    }

    pub(super) async fn get_file_state_by_handle(
        &self,
        (chat_id, msg_id): (i64, i32),
    ) -> Result<FileState> {
        todo!()
    }

    pub(crate) async fn set_file_state_by_handle(
        &self,
        handle: (i64, i32),
        state: FileState,
    ) -> std::result::Result<(), anyhow::Error> {
        todo!()
    }
}
