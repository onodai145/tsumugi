//! note cacheのPostgresBackend(Issue #115 Phase 2)。`sqlx::PgPool`を使い、
//! `NoteCacheBackend`トレイトの非同期メソッドをネイティブに(spawn_blockingなしで)実装する。
//! DDLは`sea-query`の`Table::create()`で書く。CRUD文は既存の`SqliteBackend`と同じ、
//! 手書きSQL文字列 + `$N`プレースホルダのバインド方式で書く(設計書「Global Constraints」参照)。

use crate::error::Result;
use sea_query::{ColumnDef, PostgresQueryBuilder, Table};
use sqlx::postgres::{PgPoolOptions, PgConnectOptions};
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
    pub(crate) async fn connect(params: &PostgresConnectParams) -> Result<Self> {
        let opts = PgConnectOptions::new()
            .host(&params.host)
            .port(params.port)
            .database(&params.database)
            .username(&params.user)
            .password(&params.password);
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
    pool.execute("CREATE INDEX IF NOT EXISTS idx_note_created ON note(created_at)").await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_note_user ON note(user_id)").await?;

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

    for (table, cols) in [
        ("note_reaction", "note_id TEXT, emoji_key TEXT, count BIGINT"),
        ("note_tag", "note_id TEXT, tag TEXT"),
        ("note_mention", "note_id TEXT, user_id TEXT"),
        ("note_emoji", "note_id TEXT, emoji TEXT"),
        ("note_file", "note_id TEXT, mime_type TEXT, mime_category TEXT, is_sensitive BOOLEAN"),
    ] {
        pool.execute(format!("CREATE TABLE IF NOT EXISTS {table} ({cols})").as_str()).await?;
    }
    pool.execute("CREATE INDEX IF NOT EXISTS idx_nr_note ON note_reaction(note_id)").await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_nt_note ON note_tag(note_id)").await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_nm_note ON note_mention(note_id)").await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_ne_note ON note_emoji(note_id)").await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_nf_note ON note_file(note_id)").await?;
    pool.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_nr_unique ON note_reaction(note_id, emoji_key)",
    )
    .await?;
    pool.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_nt_unique ON note_tag(note_id, tag)").await?;
    pool.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_nm_unique ON note_mention(note_id, user_id)")
        .await?;
    pool.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_ne_unique ON note_emoji(note_id, emoji)").await?;
    pool.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_nf_unique ON note_file(note_id, mime_type, mime_category, is_sensitive)",
    )
    .await?;

    pool.execute(
        "CREATE TABLE IF NOT EXISTS column_note (
            column_id TEXT NOT NULL,
            note_id TEXT NOT NULL,
            received_at BIGINT NOT NULL,
            created_at BIGINT NOT NULL DEFAULT 0,
            PRIMARY KEY (column_id, note_id)
        )",
    )
    .await?;
    pool.execute("CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id)").await?;
    pool.execute(
        "CREATE INDEX IF NOT EXISTS idx_cn_column_created ON column_note(column_id, created_at DESC, note_id DESC)",
    )
    .await?;

    pool.execute(
        "CREATE TABLE IF NOT EXISTS column_fetch_boundary (
            column_id TEXT PRIMARY KEY,
            oldest_fetched_id TEXT NOT NULL
        )",
    )
    .await?;

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
