//! note cacheのPostgresBackend(Issue #115 Phase 2)。`sqlx::PgPool`を使い、
//! `NoteCacheBackend`トレイトの非同期メソッドをネイティブに(spawn_blockingなしで)実装する。
//! DDLは`sea-query`の`Table::create()`で書く。CRUD文は既存の`SqliteBackend`と同じ、
//! 手書きSQL文字列 + `$N`プレースホルダのバインド方式で書く(設計書「Global Constraints」参照)。

use crate::error::Result;
use sea_query::{ColumnDef, Index, IndexOrder, PostgresQueryBuilder, Table};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::Executor;

pub(crate) struct PostgresConnectParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

pub(crate) struct PostgresBackend {
    pool: sqlx::PgPool,
}

impl PostgresBackend {
    /// Postgresへ接続する。TLSモードは`PgSslMode::Prefer`(sqlxのデフォルトと同じ)を
    /// 明示的に指定している — 既定値に暗黙に頼るのではなく、選択を監査可能にするため。
    /// `Require`/`VerifyFull`は使わない: このアプリはユーザーが自由に設定した任意の
    /// Postgresインスタンス(TLS未設定のLAN/ホームラボ環境を含む)へ接続するため、TLSを
    /// 強制すると正当な構成が接続できなくなる。
    pub(crate) async fn connect(params: &PostgresConnectParams) -> Result<Self> {
        let opts = PgConnectOptions::new()
            .host(&params.host)
            .port(params.port)
            .database(&params.database)
            .username(&params.user)
            .password(&params.password)
            .ssl_mode(PgSslMode::Prefer);
        let pool = PgPoolOptions::new().max_connections(5).connect_with(opts).await?;
        ensure_schema(&pool).await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }
}

/// キャッシュDBのテーブルをすべて作成する(`CREATE TABLE IF NOT EXISTS`相当、冪等)。
pub(crate) async fn ensure_schema(pool: &sqlx::PgPool) -> Result<()> {
    let note = Table::create()
        .table(NoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTable::Id).text().primary_key())
        .col(ColumnDef::new(NoteTable::CreatedAt).big_integer().not_null())
        .col(ColumnDef::new(NoteTable::Text).text())
        .col(ColumnDef::new(NoteTable::TextLength).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::Cw).text())
        .col(ColumnDef::new(NoteTable::Visibility).text().not_null())
        .col(ColumnDef::new(NoteTable::LocalOnly).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::UserId).text().not_null())
        .col(ColumnDef::new(NoteTable::ReplyId).text())
        .col(ColumnDef::new(NoteTable::ReplyUserId).text())
        .col(ColumnDef::new(NoteTable::RenoteId).text())
        .col(ColumnDef::new(NoteTable::ChannelId).text())
        .col(ColumnDef::new(NoteTable::Via).text())
        .col(ColumnDef::new(NoteTable::Lang).text())
        .col(ColumnDef::new(NoteTable::FilesCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::HasPoll).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::HasLink).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsPinned).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::ReactionCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::RenoteCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::ReplyCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::MyReaction).text())
        .col(ColumnDef::new(NoteTable::IsRenotedByMe).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsFavoritedByMe).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::Payload).text().not_null())
        .build(PostgresQueryBuilder);
    pool.execute(note.as_str()).await?;

    let idx_note_created = Index::create()
        .if_not_exists()
        .name("idx_note_created")
        .table(NoteTable::Table)
        .col(NoteTable::CreatedAt)
        .build(PostgresQueryBuilder);
    pool.execute(idx_note_created.as_str()).await?;

    let idx_note_user = Index::create()
        .if_not_exists()
        .name("idx_note_user")
        .table(NoteTable::Table)
        .col(NoteTable::UserId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_note_user.as_str()).await?;

    let user = Table::create()
        .table(UserTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(UserTable::Id).text().primary_key())
        .col(ColumnDef::new(UserTable::Username).text().not_null())
        .col(ColumnDef::new(UserTable::Host).text())
        .col(ColumnDef::new(UserTable::Name).text())
        .col(ColumnDef::new(UserTable::AvatarUrl).text())
        .col(ColumnDef::new(UserTable::IsBot).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::IsCat).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::FollowersCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::FollowingCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::NotesCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::Emojis).text().not_null().default("{}"))
        .col(ColumnDef::new(UserTable::Bio).text())
        .col(ColumnDef::new(UserTable::BannerUrl).text())
        .col(ColumnDef::new(UserTable::InstanceName).text())
        .col(ColumnDef::new(UserTable::InstanceIconUrl).text())
        .col(ColumnDef::new(UserTable::InstanceThemeColor).text())
        .build(PostgresQueryBuilder);
    pool.execute(user.as_str()).await?;

    let note_reaction = Table::create()
        .table(NoteReactionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteReactionTable::NoteId).text())
        .col(ColumnDef::new(NoteReactionTable::EmojiKey).text())
        .col(ColumnDef::new(NoteReactionTable::Count).big_integer())
        .build(PostgresQueryBuilder);
    pool.execute(note_reaction.as_str()).await?;

    let note_tag = Table::create()
        .table(NoteTagTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTagTable::NoteId).text())
        .col(ColumnDef::new(NoteTagTable::Tag).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_tag.as_str()).await?;

    let note_mention = Table::create()
        .table(NoteMentionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteMentionTable::NoteId).text())
        .col(ColumnDef::new(NoteMentionTable::UserId).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_mention.as_str()).await?;

    let note_emoji = Table::create()
        .table(NoteEmojiTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteEmojiTable::NoteId).text())
        .col(ColumnDef::new(NoteEmojiTable::Emoji).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_emoji.as_str()).await?;

    let note_file = Table::create()
        .table(NoteFileTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteFileTable::NoteId).text())
        .col(ColumnDef::new(NoteFileTable::MimeType).text())
        .col(ColumnDef::new(NoteFileTable::MimeCategory).text())
        .col(ColumnDef::new(NoteFileTable::IsSensitive).boolean())
        .build(PostgresQueryBuilder);
    pool.execute(note_file.as_str()).await?;

    let idx_nr_note = Index::create()
        .if_not_exists()
        .name("idx_nr_note")
        .table(NoteReactionTable::Table)
        .col(NoteReactionTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nr_note.as_str()).await?;

    let idx_nt_note = Index::create()
        .if_not_exists()
        .name("idx_nt_note")
        .table(NoteTagTable::Table)
        .col(NoteTagTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nt_note.as_str()).await?;

    let idx_nm_note = Index::create()
        .if_not_exists()
        .name("idx_nm_note")
        .table(NoteMentionTable::Table)
        .col(NoteMentionTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nm_note.as_str()).await?;

    let idx_ne_note = Index::create()
        .if_not_exists()
        .name("idx_ne_note")
        .table(NoteEmojiTable::Table)
        .col(NoteEmojiTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_ne_note.as_str()).await?;

    let idx_nf_note = Index::create()
        .if_not_exists()
        .name("idx_nf_note")
        .table(NoteFileTable::Table)
        .col(NoteFileTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nf_note.as_str()).await?;

    let idx_nr_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nr_unique")
        .table(NoteReactionTable::Table)
        .col(NoteReactionTable::NoteId)
        .col(NoteReactionTable::EmojiKey)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nr_unique.as_str()).await?;

    let idx_nt_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nt_unique")
        .table(NoteTagTable::Table)
        .col(NoteTagTable::NoteId)
        .col(NoteTagTable::Tag)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nt_unique.as_str()).await?;

    let idx_nm_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nm_unique")
        .table(NoteMentionTable::Table)
        .col(NoteMentionTable::NoteId)
        .col(NoteMentionTable::UserId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nm_unique.as_str()).await?;

    let idx_ne_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_ne_unique")
        .table(NoteEmojiTable::Table)
        .col(NoteEmojiTable::NoteId)
        .col(NoteEmojiTable::Emoji)
        .build(PostgresQueryBuilder);
    pool.execute(idx_ne_unique.as_str()).await?;

    let idx_nf_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nf_unique")
        .table(NoteFileTable::Table)
        .col(NoteFileTable::NoteId)
        .col(NoteFileTable::MimeType)
        .col(NoteFileTable::MimeCategory)
        .col(NoteFileTable::IsSensitive)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nf_unique.as_str()).await?;

    let column_note = Table::create()
        .table(ColumnNoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnNoteTable::ColumnId).text().not_null())
        .col(ColumnDef::new(ColumnNoteTable::NoteId).text().not_null())
        .col(ColumnDef::new(ColumnNoteTable::ReceivedAt).big_integer().not_null())
        .col(ColumnDef::new(ColumnNoteTable::CreatedAt).big_integer().not_null().default(0))
        .primary_key(
            Index::create()
                .col(ColumnNoteTable::ColumnId)
                .col(ColumnNoteTable::NoteId),
        )
        .build(PostgresQueryBuilder);
    pool.execute(column_note.as_str()).await?;

    let idx_cn_column = Index::create()
        .if_not_exists()
        .name("idx_cn_column")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_cn_column.as_str()).await?;

    let idx_cn_column_created = Index::create()
        .if_not_exists()
        .name("idx_cn_column_created")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .col((ColumnNoteTable::CreatedAt, IndexOrder::Desc))
        .col((ColumnNoteTable::NoteId, IndexOrder::Desc))
        .build(PostgresQueryBuilder);
    pool.execute(idx_cn_column_created.as_str()).await?;

    let column_fetch_boundary = Table::create()
        .table(ColumnFetchBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnFetchBoundaryTable::ColumnId).text().primary_key())
        .col(ColumnDef::new(ColumnFetchBoundaryTable::OldestFetchedId).text().not_null())
        .build(PostgresQueryBuilder);
    pool.execute(column_fetch_boundary.as_str()).await?;

    Ok(())
}

#[derive(sea_query::Iden)]
enum NoteTable {
    #[iden = "note"]
    Table,
    Id,
    CreatedAt,
    Text,
    TextLength,
    Cw,
    Visibility,
    LocalOnly,
    UserId,
    ReplyId,
    ReplyUserId,
    RenoteId,
    ChannelId,
    Via,
    Lang,
    FilesCount,
    HasPoll,
    HasLink,
    IsPinned,
    ReactionCount,
    RenoteCount,
    ReplyCount,
    MyReaction,
    IsRenotedByMe,
    IsFavoritedByMe,
    Payload,
}

#[derive(sea_query::Iden)]
enum UserTable {
    #[iden = "user"]
    Table,
    Id,
    Username,
    Host,
    Name,
    AvatarUrl,
    IsBot,
    IsCat,
    FollowersCount,
    FollowingCount,
    NotesCount,
    Emojis,
    Bio,
    BannerUrl,
    InstanceName,
    InstanceIconUrl,
    InstanceThemeColor,
}

#[derive(sea_query::Iden)]
enum NoteReactionTable {
    #[iden = "note_reaction"]
    Table,
    NoteId,
    EmojiKey,
    Count,
}

#[derive(sea_query::Iden)]
enum NoteTagTable {
    #[iden = "note_tag"]
    Table,
    NoteId,
    Tag,
}

#[derive(sea_query::Iden)]
enum NoteMentionTable {
    #[iden = "note_mention"]
    Table,
    NoteId,
    UserId,
}

#[derive(sea_query::Iden)]
enum NoteEmojiTable {
    #[iden = "note_emoji"]
    Table,
    NoteId,
    Emoji,
}

#[derive(sea_query::Iden)]
enum NoteFileTable {
    #[iden = "note_file"]
    Table,
    NoteId,
    MimeType,
    MimeCategory,
    IsSensitive,
}

#[derive(sea_query::Iden)]
enum ColumnNoteTable {
    #[iden = "column_note"]
    Table,
    ColumnId,
    NoteId,
    ReceivedAt,
    CreatedAt,
}

#[derive(sea_query::Iden)]
enum ColumnFetchBoundaryTable {
    #[iden = "column_fetch_boundary"]
    Table,
    ColumnId,
    OldestFetchedId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    /// Docker上の使い捨てPostgresへ接続し、スキーマが2回適用してもエラーにならず
    /// (冪等)、テーブルが実際に作成されることを確認する。CI常時実行はしない方針
    /// (`#[ignore]`、既存の実Misskey接続テストと同様)。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_is_idempotent_and_creates_tables() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();

        ensure_schema(&pool).await.unwrap();
        ensure_schema(&pool).await.unwrap(); // 2回目も成功する(冪等)

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'note'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    /// Postgres接続の組み立て(host/port/database/user/password)が実際に使えることの確認。
    #[tokio::test]
    #[ignore]
    async fn connect_succeeds_against_running_postgres() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let params = PostgresConnectParams {
            host: "127.0.0.1".into(),
            port,
            database: "postgres".into(),
            user: "postgres".into(),
            password: "postgres".into(),
        };
        let backend = PostgresBackend::connect(&params).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(backend.pool()).await.unwrap();
        assert_eq!(count, 0);
    }
}
