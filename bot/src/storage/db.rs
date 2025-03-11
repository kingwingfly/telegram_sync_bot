use super::entity::{chat_state, file_handle, file_state};
use super::state::*;
use crate::migration::{Migrator, MigratorTrait};
use anyhow::Result;
use sea_orm::ActiveValue::*;
use sea_orm::TransactionTrait as _;
use sea_orm::prelude::*;
use sea_orm::sea_query;
use sea_orm::{Database, DatabaseConnection};
use tracing::info;

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
        let db = establish_connection(database_url.as_ref()).await?;
        info!(">> DB: connect to {}", database_url.as_ref());
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
        let txn = self.db.begin().await?;
        let current_state: ChatState =
            match chat_state::Entity::find_by_id(chat_id).one(&txn).await? {
                Some(m) => m.state.into(),
                None => ChatState::default(),
            };
        let new_state = current_state.troggle();
        chat_state::Entity::insert(chat_state::ActiveModel {
            chat_id: Set(chat_id),
            state: Set(new_state.to_string()),
        })
        .on_conflict(
            sea_query::OnConflict::column(chat_state::Column::ChatId)
                .update_column(chat_state::Column::State)
                .to_owned(),
        )
        .exec(&txn)
        .await?;
        txn.commit().await?;
        info!(
            ">> DB: set chat {} state from {} to {}",
            chat_id, current_state, new_state
        );
        Ok(new_state)
    }

    pub(super) async fn set_file_name(&self, file_id: String, file_name: String) -> Result<()> {
        file_state::Entity::insert(file_state::ActiveModel {
            file_id: Set(file_id.to_owned()),
            file_name: Set(file_name.to_owned()),
            ..Default::default()
        })
        .on_conflict(
            sea_query::OnConflict::column(file_state::Column::FileId)
                .update_column(file_state::Column::FileName)
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        info!(">> DB: set file {} name to {}", file_id, file_name);
        Ok(())
    }

    pub(super) async fn get_transport_state(&self, file_id: String) -> Result<TransportState> {
        match file_state::Entity::find_by_id(file_id)
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
        file_state::Entity::insert(file_state::ActiveModel {
            file_id: Set(file_id.as_ref().to_string()),
            transport_state: Set(tranport_state.to_string()),
            ..Default::default()
        })
        .on_conflict(
            sea_query::OnConflict::column(file_state::Column::FileId)
                .update_column(file_state::Column::TransportState)
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        info!(
            ">> DB: set transport state {} to {}",
            file_id.as_ref(),
            tranport_state
        );
        Ok(())
    }

    pub(super) async fn get_file_id_by_handle(&self, handle: (i64, i32)) -> Result<Option<String>> {
        match file_handle::Entity::find_by_id(handle)
            .one(&self.db)
            .await?
        {
            Some(m) => Ok(Some(m.file_id)),
            None => Ok(None),
        }
    }

    pub(super) async fn get_file_state_and_name_by_handle(
        &self,
        handle: (i64, i32),
    ) -> Result<(FileState, String)> {
        let txn = self.db.begin().await?;
        let file_id = match file_handle::Entity::find_by_id(handle).one(&txn).await? {
            Some(m) => m.file_id,
            None => return Err(anyhow::anyhow!("File not found")),
        };
        let state = match file_state::Entity::find_by_id(file_id).one(&txn).await? {
            Some(m) => (m.state.into(), m.file_name),
            None => return Err(anyhow::anyhow!("File not found")),
        };
        txn.commit().await?;
        Ok(state)
    }

    pub(super) async fn set_file_state_by_handle_returning_old_state(
        &self,
        handle: (i64, i32),
        state: FileState,
    ) -> Result<Option<FileState>> {
        let txn = self.db.begin().await?;
        let file_id = match file_handle::Entity::find_by_id(handle).one(&txn).await? {
            Some(m) => m.file_id,
            None => return Err(anyhow::anyhow!("File not found")),
        };
        let old_state = file_state::Entity::find_by_id(file_id.to_owned())
            .one(&txn)
            .await?
            .map(|m| m.state.into());
        file_state::Entity::insert(file_state::ActiveModel {
            file_id: Set(file_id.to_owned()),
            state: Set(state.to_string()),
            ..Default::default()
        })
        .on_conflict(
            sea_query::OnConflict::column(file_state::Column::FileId)
                .update_column(file_state::Column::State)
                .to_owned(),
        )
        .exec(&txn)
        .await?;
        txn.commit().await?;
        info!(
            ">> DB: set file {} state from {:?} to {}",
            file_id, old_state, state
        );
        Ok(old_state)
    }

    /// Set file handle for a chat message, return the old handle if exists
    pub(super) async fn set_file_handle(
        &self,
        handle: (i64, i32),
        file_id: String,
    ) -> Result<Option<(i64, i32)>> {
        let txn = self.db.begin().await?;

        // insert file state if not exists, foreign key constraint
        file_state::Entity::insert(file_state::ActiveModel {
            file_id: Set(file_id.to_owned()),
            ..Default::default()
        })
        .exec(&txn)
        .await
        .ok();

        let result = match file_handle::Entity::find()
            .filter(file_handle::Column::ChatId.eq(handle.0))
            .filter(file_handle::Column::FileId.eq(file_id.to_owned()))
            .one(&txn)
            .await?
        {
            Some(m) if m.msg_id != handle.1 => {
                let old_chat_id = m.chat_id;
                let old_msg_id = m.msg_id;
                file_handle::Entity::delete(file_handle::ActiveModel {
                    chat_id: Set(old_chat_id),
                    msg_id: Set(old_msg_id),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
                let mut m: file_handle::ActiveModel = m.into();
                m.msg_id = Set(handle.1);
                file_handle::Entity::insert(m).exec(&txn).await?;
                Some((old_chat_id, old_msg_id))
            }
            Some(_) => None,
            None => {
                file_handle::Entity::insert(file_handle::ActiveModel {
                    chat_id: Set(handle.0),
                    msg_id: Set(handle.1),
                    file_id: Set(file_id.to_owned()),
                })
                .exec(&txn)
                .await?;
                None
            }
        };
        txn.commit().await?;
        info!(">> DB: set file {} handle {:?}", file_id, handle);
        Ok(result)
    }
}
