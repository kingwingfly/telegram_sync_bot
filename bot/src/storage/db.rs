use super::FileId;
use super::state::*;
use anyhow::Result;
use entity::{chat_state, file_handle, file_state};
use log::info;
use migration::{Migrator, MigratorTrait};
use sea_orm::ActiveValue::*;
use sea_orm::TransactionTrait as _;
use sea_orm::prelude::*;
use sea_orm::sea_query;
use sea_orm::{Database, DatabaseConnection};

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

    pub(super) async fn get_file_id_by_handle(&self, handle: (i64, i32)) -> Result<Option<FileId>> {
        match file_handle::Entity::find_by_id(handle)
            .one(&self.db)
            .await?
        {
            Some(m) => Ok(Some(m.file_id)),
            None => Ok(None),
        }
    }

    pub(super) async fn get_file_state_by_handle(&self, handle: (i64, i32)) -> Result<FileState> {
        let txn = self.db.begin().await?;
        let file_id = match file_handle::Entity::find_by_id(handle).one(&txn).await? {
            Some(m) => m.file_id,
            None => return Err(anyhow::anyhow!("File not found")),
        };
        let state = match file_state::Entity::find_by_id(file_id).one(&txn).await? {
            Some(m) => m.state.into(),
            None => return Err(anyhow::anyhow!("File not found")),
        };
        txn.commit().await?;
        Ok(state)
    }

    pub(super) async fn set_file_state_by_handle(
        &self,
        handle: (i64, i32),
        state: FileState,
    ) -> Result<()> {
        let txn = self.db.begin().await?;
        let file_id = match file_handle::Entity::find_by_id(handle).one(&txn).await? {
            Some(m) => m.file_id,
            None => return Err(anyhow::anyhow!("File not found")),
        };
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
        info!(">> DB: set file {} state to {}", file_id, state);
        Ok(())
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
        .on_conflict(
            sea_query::OnConflict::column(file_state::Column::FileId)
                .do_nothing()
                .to_owned(),
        )
        .exec(&txn)
        .await?;

        let result = match file_handle::Entity::find_by_id((handle.0, handle.1))
            .one(&txn)
            .await?
        {
            Some(record) => {
                let old_chat_id = record.chat_id;
                let old_msg_id = record.msg_id;
                if record.msg_id != handle.1 {
                    file_handle::Entity::update(file_handle::ActiveModel {
                        msg_id: Set(handle.1),
                        ..record.into()
                    })
                    .exec(&txn)
                    .await?;
                }
                Ok(Some((old_chat_id, old_msg_id)))
            }
            None => {
                file_handle::Entity::insert(file_handle::ActiveModel {
                    chat_id: Set(handle.0),
                    msg_id: Set(handle.1),
                    file_id: Set(file_id.to_owned()),
                })
                .exec(&txn)
                .await?;
                Ok(None)
            }
        };
        txn.commit().await?;
        info!(">> DB: set file {} handle {:?}", file_id, handle);
        result
    }
}
