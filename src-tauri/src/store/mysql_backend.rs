//! note cacheのMySqlBackend(Issue #115 Phase 3)。`sqlx::MySqlPool`を使い、
//! `NoteCacheBackend`トレイトの非同期メソッドをネイティブに実装する。
//! DDLは`sea-query`の`Table::create()`で書く。CRUD文は`PostgresBackend`と同じ、
//! 手書きSQL文字列 + `sqlx::query()`のバインド方式で書くが、プレースホルダは`?`
//! (MySQLネイティブ)、UPSERTは`ON DUPLICATE KEY UPDATE`/`INSERT IGNORE`を使う
//! (設計書「Phase 3設計: MySqlBackend」参照)。

use crate::error::Result;
use sea_query::{ColumnDef, Index, IndexOrder, MysqlQueryBuilder, Table};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::Executor;

pub(crate) struct MySqlConnectParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

pub(crate) struct MySqlBackend {
    pool: sqlx::MySqlPool,
}

impl MySqlBackend {
    /// MySQLへ接続する。TLSモードは`MySqlSslMode::Preferred`(sqlxのデフォルトと同じ)を
    /// 明示的に指定している — 既定値に暗黙に頼るのではなく、選択を監査可能にするため。
    /// `Required`/`VerifyCa`/`VerifyIdentity`は使わない: このアプリはユーザーが自由に
    /// 設定した任意のMySQL/MariaDBインスタンス(TLS未設定のLAN/ホームラボ環境を含む)へ
    /// 接続するため、TLSを強制すると正当な構成が接続できなくなる
    /// (`PostgresBackend::connect`と同じ理由・同じ規約)。
    pub(crate) async fn connect(params: &MySqlConnectParams) -> Result<Self> {
        let opts = MySqlConnectOptions::new()
            .host(&params.host)
            .port(params.port)
            .database(&params.database)
            .username(&params.user)
            .password(&params.password)
            .ssl_mode(MySqlSslMode::Preferred);
        // 起動パス(`lib.rs::run()`内の`block_on`)からも呼ばれるため、sqlxの既定30秒だと
        // 到達不能/誤設定ホストでアプリの起動が最大30秒ブロックされうる。5秒に短縮する
        // (`PostgresBackend::connect`と同じ規約)。
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect_with(opts)
            .await?;
        ensure_schema(&pool).await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &sqlx::MySqlPool {
        &self.pool
    }
}

/// インデックス作成を実行し、既に存在する場合（Duplicate key name）は無視する。
async fn execute_index(pool: &sqlx::MySqlPool, index_sql: &str) -> Result<()> {
    match pool.execute(index_sql).await {
        Ok(_) => Ok(()),
        // MySQL error 1061: Duplicate key name — CREATE INDEXにはネイティブの
        // IF NOT EXISTSが無いため、2回目以降のensure_schema()呼び出しで既存の
        // インデックスに対して発生する。無害なので握りつぶす。
        //
        // `DatabaseError::code()`はSQLSTATE("42000"のような汎用カテゴリ)を返すため、
        // MySQL固有の数値エラーコード(1061)はここでは判定できない。
        // `MySqlDatabaseError::number()`にダウンキャストして正確な数値コードで判定する。
        // (メッセージ文字列の"Duplicate"部分一致では1062 Duplicate entry(実際の
        // UNIQUE制約違反)まで誤って握りつぶしてしまうため、それは避ける)
        Err(sqlx::Error::Database(db_err))
            if db_err
                .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .map(|e| e.number())
                == Some(1061) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// キャッシュDBのテーブルをすべて作成する(`CREATE TABLE IF NOT EXISTS`相当、冪等)。
pub(crate) async fn ensure_schema(pool: &sqlx::MySqlPool) -> Result<()> {
    let note = Table::create()
        .table(NoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTable::Id).string().primary_key())
        .col(ColumnDef::new(NoteTable::CreatedAt).big_integer().not_null())
        .col(ColumnDef::new(NoteTable::Text).text())
        .col(ColumnDef::new(NoteTable::TextLength).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::Cw).text())
        .col(ColumnDef::new(NoteTable::Visibility).text().not_null())
        .col(ColumnDef::new(NoteTable::LocalOnly).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::UserId).string().not_null())
        .col(ColumnDef::new(NoteTable::ReplyId).text())
        .col(ColumnDef::new(NoteTable::ReplyUserId).text())
        .col(ColumnDef::new(NoteTable::RenoteId).text())
        .col(ColumnDef::new(NoteTable::ChannelId).text())
        .col(ColumnDef::new(NoteTable::Via).text())
        .col(ColumnDef::new(NoteTable::Lang).text())
        .col(ColumnDef::new(NoteTable::FilesCount).big_integer().not_null().default(0))
        // has_poll/has_link/is_pinned/is_renoted_by_me/is_favorited_by_meは
        // Postgres版と違いBOOLEANのままでよい(MySQLのBOOLEANはTINYINT(1)のエイリアスで、
        // filter/sql.rs::bool_fieldが発行する`= 1`比較がそのまま通ることを実DBで確認済み)。
        .col(ColumnDef::new(NoteTable::HasPoll).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::HasLink).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsPinned).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::ReactionCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::RenoteCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::ReplyCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::MyReaction).text())
        .col(ColumnDef::new(NoteTable::IsRenotedByMe).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsFavoritedByMe).boolean().not_null().default(false))
        // MySQLのTEXT型はUNIQUE/PRIMARY KEY制約に使う場合、暗黙の長さ制限(通常255バイト
        // 相当のインデックスプレフィックス)を要求されることがあるが、sea-queryの
        // ColumnDef::text()はMySQLでは`TEXT`(可変長無制限)を出力しつつ、主キー列には
        // 別途プレフィックス長指定が必要になる場合がある。実装時にidカラムでエラーが
        // 出た場合は`.string()`(sea-queryの`VARCHAR(255)`相当)に変更すること
        // (MisskeyのIDは高々数十文字なのでVARCHAR(255)で実用上問題ない)。
        .col(ColumnDef::new(NoteTable::Payload).text().not_null())
        .build(MysqlQueryBuilder);
    pool.execute(note.as_str()).await?;

    let idx_note_created = Index::create()
        .if_not_exists()
        .name("idx_note_created")
        .table(NoteTable::Table)
        .col(NoteTable::CreatedAt)
        .build(MysqlQueryBuilder);
    execute_index(pool, idx_note_created.as_str()).await?;

    let idx_note_user = Index::create()
        .if_not_exists()
        .name("idx_note_user")
        .table(NoteTable::Table)
        .col(NoteTable::UserId)
        .build(MysqlQueryBuilder);
    execute_index(pool, idx_note_user.as_str()).await?;

    let user = Table::create()
        .table(UserTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(UserTable::Id).string().primary_key())
        .col(ColumnDef::new(UserTable::Username).text().not_null())
        .col(ColumnDef::new(UserTable::Host).text())
        .col(ColumnDef::new(UserTable::Name).text())
        .col(ColumnDef::new(UserTable::AvatarUrl).text())
        .col(ColumnDef::new(UserTable::IsBot).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::IsCat).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::FollowersCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::FollowingCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::NotesCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::Emojis).text().not_null())
        .col(ColumnDef::new(UserTable::Bio).text())
        .col(ColumnDef::new(UserTable::BannerUrl).text())
        .col(ColumnDef::new(UserTable::InstanceName).text())
        .col(ColumnDef::new(UserTable::InstanceIconUrl).text())
        .col(ColumnDef::new(UserTable::InstanceThemeColor).text())
        .build(MysqlQueryBuilder);
    pool.execute(user.as_str()).await?;

    let note_reaction = Table::create()
        .table(NoteReactionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteReactionTable::NoteId).string())
        .col(ColumnDef::new(NoteReactionTable::EmojiKey).string())
        .col(ColumnDef::new(NoteReactionTable::Count).big_integer())
        .build(MysqlQueryBuilder);
    pool.execute(note_reaction.as_str()).await?;

    let note_tag = Table::create()
        .table(NoteTagTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTagTable::NoteId).string())
        .col(ColumnDef::new(NoteTagTable::Tag).string())
        .build(MysqlQueryBuilder);
    pool.execute(note_tag.as_str()).await?;

    let note_mention = Table::create()
        .table(NoteMentionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteMentionTable::NoteId).string())
        .col(ColumnDef::new(NoteMentionTable::UserId).string())
        .build(MysqlQueryBuilder);
    pool.execute(note_mention.as_str()).await?;

    let note_emoji = Table::create()
        .table(NoteEmojiTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteEmojiTable::NoteId).string())
        .col(ColumnDef::new(NoteEmojiTable::Emoji).string())
        .build(MysqlQueryBuilder);
    pool.execute(note_emoji.as_str()).await?;

    let note_file = Table::create()
        .table(NoteFileTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteFileTable::NoteId).string())
        .col(ColumnDef::new(NoteFileTable::MimeType).string())
        .col(ColumnDef::new(NoteFileTable::MimeCategory).string())
        .col(ColumnDef::new(NoteFileTable::IsSensitive).boolean())
        .build(MysqlQueryBuilder);
    pool.execute(note_file.as_str()).await?;

    let idx_nr_note = Index::create().if_not_exists().name("idx_nr_note").table(NoteReactionTable::Table).col(NoteReactionTable::NoteId).build(MysqlQueryBuilder);
    execute_index(pool, idx_nr_note.as_str()).await?;
    let idx_nt_note = Index::create().if_not_exists().name("idx_nt_note").table(NoteTagTable::Table).col(NoteTagTable::NoteId).build(MysqlQueryBuilder);
    execute_index(pool, idx_nt_note.as_str()).await?;
    let idx_nm_note = Index::create().if_not_exists().name("idx_nm_note").table(NoteMentionTable::Table).col(NoteMentionTable::NoteId).build(MysqlQueryBuilder);
    execute_index(pool, idx_nm_note.as_str()).await?;
    let idx_ne_note = Index::create().if_not_exists().name("idx_ne_note").table(NoteEmojiTable::Table).col(NoteEmojiTable::NoteId).build(MysqlQueryBuilder);
    execute_index(pool, idx_ne_note.as_str()).await?;
    let idx_nf_note = Index::create().if_not_exists().name("idx_nf_note").table(NoteFileTable::Table).col(NoteFileTable::NoteId).build(MysqlQueryBuilder);
    execute_index(pool, idx_nf_note.as_str()).await?;

    let idx_nr_unique = Index::create().if_not_exists().unique().name("idx_nr_unique").table(NoteReactionTable::Table).col(NoteReactionTable::NoteId).col(NoteReactionTable::EmojiKey).build(MysqlQueryBuilder);
    execute_index(pool, idx_nr_unique.as_str()).await?;
    let idx_nt_unique = Index::create().if_not_exists().unique().name("idx_nt_unique").table(NoteTagTable::Table).col(NoteTagTable::NoteId).col(NoteTagTable::Tag).build(MysqlQueryBuilder);
    execute_index(pool, idx_nt_unique.as_str()).await?;
    let idx_nm_unique = Index::create().if_not_exists().unique().name("idx_nm_unique").table(NoteMentionTable::Table).col(NoteMentionTable::NoteId).col(NoteMentionTable::UserId).build(MysqlQueryBuilder);
    execute_index(pool, idx_nm_unique.as_str()).await?;
    let idx_ne_unique = Index::create().if_not_exists().unique().name("idx_ne_unique").table(NoteEmojiTable::Table).col(NoteEmojiTable::NoteId).col(NoteEmojiTable::Emoji).build(MysqlQueryBuilder);
    execute_index(pool, idx_ne_unique.as_str()).await?;
    let idx_nf_unique = Index::create().if_not_exists().unique().name("idx_nf_unique").table(NoteFileTable::Table).col(NoteFileTable::NoteId).col(NoteFileTable::MimeType).col(NoteFileTable::MimeCategory).col(NoteFileTable::IsSensitive).build(MysqlQueryBuilder);
    execute_index(pool, idx_nf_unique.as_str()).await?;

    let column_note = Table::create()
        .table(ColumnNoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnNoteTable::ColumnId).string().not_null())
        .col(ColumnDef::new(ColumnNoteTable::NoteId).string().not_null())
        .col(ColumnDef::new(ColumnNoteTable::ReceivedAt).big_integer().not_null())
        .col(ColumnDef::new(ColumnNoteTable::CreatedAt).big_integer().not_null().default(0))
        .primary_key(Index::create().col(ColumnNoteTable::ColumnId).col(ColumnNoteTable::NoteId))
        .build(MysqlQueryBuilder);
    pool.execute(column_note.as_str()).await?;

    let idx_cn_column = Index::create().if_not_exists().name("idx_cn_column").table(ColumnNoteTable::Table).col(ColumnNoteTable::ColumnId).build(MysqlQueryBuilder);
    execute_index(pool, idx_cn_column.as_str()).await?;

    let idx_cn_column_created = Index::create()
        .if_not_exists()
        .name("idx_cn_column_created")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .col((ColumnNoteTable::CreatedAt, IndexOrder::Desc))
        .col((ColumnNoteTable::NoteId, IndexOrder::Desc))
        .build(MysqlQueryBuilder);
    execute_index(pool, idx_cn_column_created.as_str()).await?;

    let column_fetch_boundary = Table::create()
        .table(ColumnFetchBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnFetchBoundaryTable::ColumnId).string().primary_key())
        .col(ColumnDef::new(ColumnFetchBoundaryTable::OldestFetchedId).text().not_null())
        .build(MysqlQueryBuilder);
    pool.execute(column_fetch_boundary.as_str()).await?;

    Ok(())
}

#[derive(sea_query::Iden)]
enum NoteTable {
    #[iden = "note"]
    Table, Id, CreatedAt, Text, TextLength, Cw, Visibility, LocalOnly, UserId,
    ReplyId, ReplyUserId, RenoteId, ChannelId, Via, Lang, FilesCount, HasPoll,
    HasLink, IsPinned, ReactionCount, RenoteCount, ReplyCount, MyReaction,
    IsRenotedByMe, IsFavoritedByMe, Payload,
}

#[derive(sea_query::Iden)]
enum UserTable {
    #[iden = "user"]
    Table, Id, Username, Host, Name, AvatarUrl, IsBot, IsCat, FollowersCount,
    FollowingCount, NotesCount, Emojis, Bio, BannerUrl, InstanceName,
    InstanceIconUrl, InstanceThemeColor,
}

#[derive(sea_query::Iden)]
enum NoteReactionTable {
    #[iden = "note_reaction"]
    Table, NoteId, EmojiKey, Count,
}

#[derive(sea_query::Iden)]
enum NoteTagTable {
    #[iden = "note_tag"]
    Table, NoteId, Tag,
}

#[derive(sea_query::Iden)]
enum NoteMentionTable {
    #[iden = "note_mention"]
    Table, NoteId, UserId,
}

#[derive(sea_query::Iden)]
enum NoteEmojiTable {
    #[iden = "note_emoji"]
    Table, NoteId, Emoji,
}

#[derive(sea_query::Iden)]
enum NoteFileTable {
    #[iden = "note_file"]
    Table, NoteId, MimeType, MimeCategory, IsSensitive,
}

#[derive(sea_query::Iden)]
enum ColumnNoteTable {
    #[iden = "column_note"]
    Table, ColumnId, NoteId, ReceivedAt, CreatedAt,
}

#[derive(sea_query::Iden)]
enum ColumnFetchBoundaryTable {
    #[iden = "column_fetch_boundary"]
    Table, ColumnId, OldestFetchedId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::{mysql::Mysql, testcontainers::runners::AsyncRunner};

    /// Docker上の使い捨てMySQLへ接続し、スキーマが2回適用してもエラーにならず
    /// (冪等)、テーブルが実際に作成されることを確認する。CI常時実行はしない方針
    /// (`#[ignore]`、既存のPostgres統合テストと同様)。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_is_idempotent_and_creates_tables() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        ensure_schema(&pool).await.unwrap();
        ensure_schema(&pool).await.unwrap(); // 2回目も成功する(冪等)

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'note' AND table_schema = 'test'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    /// MySQL接続の組み立て(host/port/database/user/password)が実際に使えることの確認。
    #[tokio::test]
    #[ignore]
    async fn connect_succeeds_against_running_mysql() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let params = MySqlConnectParams {
            host: "127.0.0.1".into(),
            port,
            database: "test".into(),
            user: "root".into(),
            password: "".into(),
        };
        let backend = MySqlBackend::connect(&params).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(backend.pool()).await.unwrap();
        assert_eq!(count, 0);
    }

    /// `execute_index()`が握りつぶすのはMySQLエラー1061(Duplicate key name)のみで
    /// あることを確認する。存在しないテーブルに対するCREATE INDEXはエラー1146
    /// (table doesn't exist)になり、1061ではないため`Err`として伝播しなければならない。
    #[tokio::test]
    #[ignore]
    async fn execute_index_propagates_non_1061_errors() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        let result = execute_index(
            &pool,
            "CREATE INDEX idx_bogus ON nonexistent_table (col)",
        )
        .await;

        assert!(
            result.is_err(),
            "存在しないテーブルへのCREATE INDEX(エラー1146)は握りつぶされず伝播するべき"
        );
    }

    /// レビュー指摘の本丸: 文字列部分一致での判定だとMySQLエラー1062
    /// (Duplicate entry — 実際のUNIQUE制約違反によるデータ整合性エラー)まで
    /// "Duplicate"にマッチして握りつぶしてしまう。1061(Duplicate key name)専用の
    /// エラーコード判定に直したことで、1062は握りつぶされず伝播することを確認する。
    #[tokio::test]
    #[ignore]
    async fn execute_index_does_not_swallow_unique_constraint_violation() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        pool.execute("CREATE TABLE dup_test (v INT)").await.unwrap();
        pool.execute("INSERT INTO dup_test VALUES (1), (1)").await.unwrap();

        let result = execute_index(&pool, "CREATE UNIQUE INDEX idx_dup ON dup_test (v)").await;

        assert!(
            result.is_err(),
            "重複データに対するCREATE UNIQUE INDEX(エラー1062)は握りつぶされず伝播するべき"
        );
    }
}
