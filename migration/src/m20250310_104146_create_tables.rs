use log::info;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let file_handle = Table::create()
            .table(FileHandle::Table)
            .if_not_exists()
            .col(big_integer(FileHandle::ChatID))
            .col(integer(FileHandle::MsgID))
            .col(string(FileHandle::FileID))
            .primary_key(
                Index::create()
                    .col(FileHandle::ChatID)
                    .col(FileHandle::MsgID),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("file_id_fk")
                    .from(FileHandle::Table, FileHandle::FileID)
                    .to(FileState::Table, FileState::FileID)
                    .on_delete(ForeignKeyAction::Cascade)
                    .on_update(ForeignKeyAction::Cascade),
            )
            .to_owned();
        info!(
            "create table: {}",
            file_handle.to_string(SqliteQueryBuilder)
        );
        manager.create_table(file_handle).await?;
        let index = Index::create()
            .table(FileHandle::Table)
            .name("file_id_index")
            .col(FileHandle::FileID)
            .to_owned();
        info!("create table: {}", index.to_string(SqliteQueryBuilder));
        manager.create_index(index).await?;

        let file_state = Table::create()
            .table(FileState::Table)
            .if_not_exists()
            .col(string(FileState::FileID).primary_key())
            .col(
                enumeration(
                    FileState::State,
                    Alias::new("state"),
                    [Alias::new("Fav"), Alias::new("Trash"), Alias::new("Normal")],
                )
                .default("NotReady"),
            )
            .col(
                enumeration(
                    FileState::TransportState,
                    Alias::new("transport_state"),
                    [
                        Alias::new("Pending"),
                        Alias::new("Downloading"),
                        Alias::new("Paused"),
                        Alias::new("Completed"),
                        Alias::new("Cancelled"),
                        Alias::new("Failed"),
                    ],
                )
                .default("Pending"),
            )
            .to_owned();
        info!("create table: {}", file_state.to_string(SqliteQueryBuilder));
        manager.create_table(file_state).await?;

        let chat_state = Table::create()
            .table(ChatState::Table)
            .if_not_exists()
            .col(big_integer(ChatState::ChatID).primary_key())
            .col(
                enumeration(
                    ChatState::State,
                    Alias::new("chat_state"),
                    [
                        Alias::new("Paused"),
                        Alias::new("Active"),
                        Alias::new("PartiallyActive"),
                    ],
                )
                .default(if cfg!(debug_assertions) {
                    "Active"
                } else {
                    "Paused"
                }),
            )
            .to_owned();
        info!("create table: {}", chat_state.to_string(SqliteQueryBuilder));
        manager.create_table(chat_state).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FileHandle::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(FileState::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ChatState::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum FileHandle {
    Table, // this is a special case; will be mapped to `filehandle`
    ChatID,
    MsgID,
    FileID,
}

#[derive(DeriveIden)]
enum FileState {
    Table, // this is a special case; will be mapped to `filestate`
    FileID,
    State,
    TransportState,
}

#[derive(DeriveIden)]
enum ChatState {
    Table, // this is a special case; will be mapped to `chatstate`
    ChatID,
    State,
}
