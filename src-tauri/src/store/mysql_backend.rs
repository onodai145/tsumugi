//! note cacheのMySqlBackend(Issue #115 Phase 3)。`sqlx::MySqlPool`を使い、
//! `NoteCacheBackend`トレイトの非同期メソッドをネイティブに実装する。
//! DDLは`sea-query`の`Table::create()`で書く。CRUD文は`PostgresBackend`と同じ、
//! 手書きSQL文字列 + `sqlx::query()`のバインド方式で書くが、プレースホルダは`?`
//! (MySQLネイティブ)、UPSERTは`ON DUPLICATE KEY UPDATE`/`INSERT IGNORE`を使う
//! (設計書「Phase 3設計: MySqlBackend」参照)。

use crate::error::Result;
use sea_query::{Alias, ColumnDef, Index, IndexOrder, MysqlQueryBuilder, Table};
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

/// `payload`/`text`/`cw`/`bio`のような無制限長のユーザーコンテンツを保持する列に使う。
/// sea-query 0.32.7の`ColumnDef::text()`はMySQLでは`TEXT`(65,535バイト上限)を出力する。
/// SQLiteの`TEXT`・Postgresの`text`はどちらも実質無制限なので、`payload`(ネストした
/// `renote`/`emojis`/`reactions`/`files`/`mentions`を含むNote全体のシリアライズJSON)が
/// この上限を超えると strict mode ではエラー1406でトランザクション全体がロールバックし、
/// non-strict では黙って切り詰められてJSONが壊れ`resolve_payload_rows`がそのnoteを
/// サイレントに読み捨てる(実データ損失)。`ColumnDef`にLONGTEXT専用のビルダーが無いため
/// `.custom(Alias::new("LONGTEXT"))`でMySQL方言の生の型名を直接指定する
/// (このバックエンドは`ensure_schema`が`CREATE TABLE IF NOT EXISTS`のみで
/// `ALTER TABLE`によるマイグレーション手段を持たないため、初回リリース時点で
/// 十分な上限を選んでおく必要がある)。
fn long_text(col: &mut ColumnDef) -> &mut ColumnDef {
    col.custom(Alias::new("LONGTEXT"))
}

/// インデックス作成を実行し、既に存在する場合（Duplicate key name）は無視する。
///
/// 呼び出し元は全てsea-queryの`Index::create()`ビルダー出力かリテラルのDDL文字列を渡す
/// (`ensure_schema`参照)ため、`sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
async fn execute_index(pool: &sqlx::MySqlPool, index_sql: &str) -> Result<()> {
    match pool.execute(sqlx::AssertSqlSafe(index_sql)).await {
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

/// `ALTER TABLE ... ADD COLUMN`を実行し、列が既に存在する場合(Duplicate column name)は無視する。
/// MySQLには`ADD COLUMN IF NOT EXISTS`のネイティブ構文が無い(MariaDB専用拡張であり、
/// 実際のMySQLでは構文エラー(1064)になる)ため、`alter_sql`には`IF NOT EXISTS`を含めず
/// 素のALTER文を渡し、2回目以降の`ensure_schema()`呼び出しで既存の列に対して発生する
/// エラー1060(ER_DUP_FIELDNAME)だけをここで握りつぶす。`execute_index`と同じ理由で、
/// `MySqlDatabaseError::number()`で正確な数値コードを判定する(メッセージ文字列の
/// 部分一致では他のエラーまで誤って握りつぶしてしまうため避ける)。
///
/// 呼び出し元は全てリテラルの`ALTER TABLE ... ADD COLUMN`文字列を渡す(`ensure_schema`
/// 参照)ため、`sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
async fn add_column_if_missing(pool: &sqlx::MySqlPool, alter_sql: &str) -> Result<()> {
    match pool.execute(sqlx::AssertSqlSafe(alter_sql)).await {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err))
            if db_err
                .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                .map(|e| e.number())
                == Some(1060) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Issue #409: `user.instance_*` を `instance` テーブルへ移す(一度きり)。
/// 旧 `instance_name` 列が残っていることを「未移行」のマーカーとして使う(MySQLには
/// 「一度だけ実行」のマーカー機構が無いため)。MySQLのDDLは暗黙コミットされ、コピーとDROPを
/// 1トランザクションにできない。そのためコピーは競合時に何もしない `INSERT IGNORE` にして、
/// コピー後・DROP前に中断して再実行しても、その間に再受信した新しい値を旧列の値で潰さない
/// (旧列が残っている限り再度コピーを試みるが、既存の行は変更されない)。
/// 同一ホストで旧行の値が食い違う場合は列ごとのMAXで1つに決める(次回の受信で自己修復される)。
async fn migrate_instance_columns(pool: &sqlx::MySqlPool) -> Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
         WHERE table_schema = DATABASE() AND table_name = 'user' AND column_name = 'instance_name'",
    )
    .fetch_one(pool)
    .await?;
    if legacy == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT IGNORE INTO `instance` (host, name, icon_url, theme_color)
         SELECT host, MAX(instance_name), MAX(instance_icon_url), MAX(instance_theme_color)
         FROM `user`
         WHERE host IS NOT NULL
           AND (instance_name IS NOT NULL OR instance_icon_url IS NOT NULL OR instance_theme_color IS NOT NULL)
         GROUP BY host",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE `user`
         DROP COLUMN instance_name,
         DROP COLUMN instance_icon_url,
         DROP COLUMN instance_theme_color",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Issue #238: 単一ソース境界 `column_fetch_boundary` を `column_source_boundary`
/// (source_idx=0)へ移す(一度きり)。旧テーブルが残っていることを「未移行」のマーカーとして使う。
/// MySQLのDDLは暗黙コミットされ、コピーとDROPを1トランザクションにできない。コピーは競合時に
/// 何もしない `INSERT IGNORE` なので、コピー後・DROP前に中断して再実行しても、移行後に更新された
/// 値を旧値で潰さない。
async fn migrate_fetch_boundary(pool: &sqlx::MySqlPool) -> Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables
         WHERE table_schema = DATABASE() AND table_name = 'column_fetch_boundary'",
    )
    .fetch_one(pool)
    .await?;
    if legacy == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT IGNORE INTO column_source_boundary (column_id, source_idx, oldest_fetched_id)
         SELECT column_id, 0, oldest_fetched_id FROM column_fetch_boundary",
    )
    .execute(pool)
    .await?;
    sqlx::query("DROP TABLE column_fetch_boundary").execute(pool).await?;
    Ok(())
}

/// キャッシュDBのテーブルをすべて作成する(`CREATE TABLE IF NOT EXISTS`相当、冪等)。
///
/// この関数内の`pool.execute(sqlx::AssertSqlSafe(..))`は全てsea-queryの`Table::create()`
/// `/Index::create()`ビルダーが生成したDDL文字列であり、外部入力・ユーザーデータは一切
/// 混入しない(列名/型/インデックス名はすべてソース中のリテラル)。sqlx 0.9の`SqlSafeStr`
/// が要求する監査は本コメントで満たす(sea-queryの出力は`String`であり
/// `&'static str`ではないためラップが必要)。
pub(crate) async fn ensure_schema(pool: &sqlx::MySqlPool) -> Result<()> {
    let note = Table::create()
        .table(NoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTable::Id).string_len(64).primary_key())
        .col(ColumnDef::new(NoteTable::CreatedAt).big_integer().not_null())
        .col(long_text(&mut ColumnDef::new(NoteTable::Text)))
        .col(ColumnDef::new(NoteTable::TextLength).big_integer().not_null().default(0))
        .col(long_text(&mut ColumnDef::new(NoteTable::Cw)))
        .col(ColumnDef::new(NoteTable::Visibility).text().not_null())
        .col(ColumnDef::new(NoteTable::LocalOnly).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::UserId).string_len(64).not_null())
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
        .col(long_text(&mut ColumnDef::new(NoteTable::Payload)).not_null())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note)).await?;

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
        .col(ColumnDef::new(UserTable::Id).string_len(64).primary_key())
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
        .col(long_text(&mut ColumnDef::new(UserTable::Bio)))
        .col(ColumnDef::new(UserTable::BannerUrl).text())
        .col(ColumnDef::new(UserTable::AvatarBlurhash).text())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(user)).await?;
    // Issue #41: 猫耳表示の色抽出用。sea_query の CREATE TABLE IF NOT EXISTS は既存テーブルへの
    // 列追加を行わないため、`user` テーブルが既に存在する既存インストール向けに明示的な
    // ALTER TABLE ... ADD COLUMN を別途実行する(add_column_if_missingが事前に列有無を
    // 確認するため冪等。MySQLにはADD COLUMN IF NOT EXISTS構文が無いため使用していない)。
    add_column_if_missing(pool, "ALTER TABLE `user` ADD COLUMN avatar_blurhash TEXT").await?;

    // Issue #409: インスタンス単位の表示情報。user.instance_* から移行する(旧列があるときだけ)。
    // MySQLはTEXTにPKを張れないため host は VARCHAR(255)(ホスト名の上限は253文字)。
    let instance = Table::create()
        .table(InstanceTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(InstanceTable::Host).string_len(255).primary_key())
        .col(ColumnDef::new(InstanceTable::Name).text())
        .col(ColumnDef::new(InstanceTable::IconUrl).text())
        .col(ColumnDef::new(InstanceTable::ThemeColor).text())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(instance)).await?;
    migrate_instance_columns(pool).await?;

    let note_reaction = Table::create()
        .table(NoteReactionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteReactionTable::NoteId).string_len(64))
        .col(ColumnDef::new(NoteReactionTable::EmojiKey).string_len(64))
        .col(ColumnDef::new(NoteReactionTable::Count).big_integer())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_reaction)).await?;

    let note_tag = Table::create()
        .table(NoteTagTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTagTable::NoteId).string_len(64))
        .col(ColumnDef::new(NoteTagTable::Tag).string_len(64))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_tag)).await?;

    let note_mention = Table::create()
        .table(NoteMentionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteMentionTable::NoteId).string_len(64))
        .col(ColumnDef::new(NoteMentionTable::UserId).string_len(64))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_mention)).await?;

    let note_emoji = Table::create()
        .table(NoteEmojiTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteEmojiTable::NoteId).string_len(64))
        .col(ColumnDef::new(NoteEmojiTable::Emoji).string_len(64))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_emoji)).await?;

    let note_file = Table::create()
        .table(NoteFileTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteFileTable::NoteId).string_len(64))
        .col(ColumnDef::new(NoteFileTable::MimeType).string_len(64))
        .col(ColumnDef::new(NoteFileTable::MimeCategory).string_len(64))
        .col(ColumnDef::new(NoteFileTable::IsSensitive).boolean())
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_file)).await?;

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
        .col(ColumnDef::new(ColumnNoteTable::ColumnId).string_len(64).not_null())
        .col(ColumnDef::new(ColumnNoteTable::NoteId).string_len(64).not_null())
        .col(ColumnDef::new(ColumnNoteTable::ReceivedAt).big_integer().not_null())
        .col(ColumnDef::new(ColumnNoteTable::CreatedAt).big_integer().not_null().default(0))
        .primary_key(Index::create().col(ColumnNoteTable::ColumnId).col(ColumnNoteTable::NoteId))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(column_note)).await?;

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

    let column_source_boundary = Table::create()
        .table(ColumnSourceBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnSourceBoundaryTable::ColumnId).string_len(64).not_null())
        .col(ColumnDef::new(ColumnSourceBoundaryTable::SourceIdx).integer().not_null())
        .col(ColumnDef::new(ColumnSourceBoundaryTable::OldestFetchedId).text().not_null())
        .primary_key(Index::create().col(ColumnSourceBoundaryTable::ColumnId).col(ColumnSourceBoundaryTable::SourceIdx))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(column_source_boundary)).await?;

    migrate_fetch_boundary(pool).await?;

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
    FollowingCount, NotesCount, Emojis, Bio, BannerUrl, AvatarBlurhash,
}

#[derive(sea_query::Iden)]
enum InstanceTable {
    #[iden = "instance"]
    Table, Host, Name, IconUrl, ThemeColor,
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
enum ColumnSourceBoundaryTable {
    #[iden = "column_source_boundary"]
    Table, ColumnId, SourceIdx, OldestFetchedId,
}

use crate::domain::Note;
use crate::store::note_cache::NoteCacheBackend;

fn visibility_str(v: crate::domain::Visibility) -> &'static str {
    use crate::domain::Visibility::*;
    match v {
        Public => "public",
        Home => "home",
        Followers => "followers",
        Specified => "specified",
    }
}

fn mime_category(mime: &str) -> &str {
    mime.split('/').next().unwrap_or("other")
}

fn has_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

/// noteが参照するuserをすべてupsertする。`PostgresBackend::upsert_note_users`と
/// 同じ理由・同じ規約(トランザクション保持中に呼んではならない)。
async fn upsert_note_users(pool: &sqlx::MySqlPool, n: &Note) -> Result<()> {
    for user in crate::store::user_ref::collect_users(n) {
        crate::store::mysql_user_ref::upsert_user(pool, user).await?;
    }
    Ok(())
}

/// `note`行 + 側テーブルをUPSERTする。`PostgresBackend::upsert_note_tx`と等価だが、
/// UPSERT構文・プレースホルダ・配列バインドの扱いをMySQL方言に変換している
/// (Global Constraints参照)。呼び出し元が開始したトランザクション`tx`の中で実行する。
async fn upsert_note_tx(tx: &mut sqlx::MySqlTransaction<'_>, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    crate::store::user_ref::stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false);

    sqlx::query(
        "INSERT INTO note (
            id, created_at, text, text_length, cw, visibility, local_only, user_id,
            reply_id, reply_user_id, renote_id, channel_id, via, lang,
            files_count, has_poll, has_link, is_pinned,
            reaction_count, renote_count, reply_count, my_reaction,
            is_renoted_by_me, is_favorited_by_me, payload
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        ON DUPLICATE KEY UPDATE
            created_at = VALUES(created_at), text = VALUES(text), text_length = VALUES(text_length),
            cw = VALUES(cw), visibility = VALUES(visibility), local_only = VALUES(local_only),
            user_id = VALUES(user_id), reply_id = VALUES(reply_id), reply_user_id = VALUES(reply_user_id),
            renote_id = VALUES(renote_id), channel_id = VALUES(channel_id), via = VALUES(via),
            lang = VALUES(lang), files_count = VALUES(files_count), has_poll = VALUES(has_poll),
            has_link = VALUES(has_link), is_pinned = VALUES(is_pinned),
            reaction_count = VALUES(reaction_count), renote_count = VALUES(renote_count),
            reply_count = VALUES(reply_count), my_reaction = VALUES(my_reaction),
            is_renoted_by_me = VALUES(is_renoted_by_me), is_favorited_by_me = VALUES(is_favorited_by_me),
            payload = VALUES(payload)",
    )
    .bind(&n.id)
    .bind(n.created_at)
    .bind(&n.text)
    .bind(text_length)
    .bind(&n.cw)
    .bind(visibility_str(n.visibility))
    .bind(n.local_only)
    .bind(&n.user.id)
    .bind(&n.reply_id)
    .bind(&n.reply_user_id)
    .bind(&n.renote_id)
    .bind(&n.channel_id)
    .bind(&n.via)
    .bind(&n.lang)
    .bind(n.files.len() as i64)
    .bind(n.poll.is_some())
    .bind(has_link)
    .bind(n.is_pinned)
    .bind(n.reaction_count as i64)
    .bind(n.renote_count as i64)
    .bind(n.reply_count as i64)
    .bind(&n.my_reaction)
    .bind(n.is_renoted_by_me)
    .bind(n.is_favorited_by_me)
    .bind(&payload)
    .execute(&mut **tx)
    .await?;

    for (emoji, count) in &n.reactions {
        sqlx::query(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?,?,?)
             ON DUPLICATE KEY UPDATE count = VALUES(count)",
        )
        .bind(&n.id)
        .bind(emoji)
        .bind(*count as i64)
        .execute(&mut **tx)
        .await?;
    }
    for tag in &n.tags {
        sqlx::query("INSERT IGNORE INTO note_tag (note_id, tag) VALUES (?,?)")
            .bind(&n.id)
            .bind(tag)
            .execute(&mut **tx)
            .await?;
    }
    for uid in &n.mentions {
        sqlx::query("INSERT IGNORE INTO note_mention (note_id, user_id) VALUES (?,?)")
            .bind(&n.id)
            .bind(uid)
            .execute(&mut **tx)
            .await?;
    }
    for e in n.emojis.keys() {
        sqlx::query("INSERT IGNORE INTO note_emoji (note_id, emoji) VALUES (?,?)")
            .bind(&n.id)
            .bind(e)
            .execute(&mut **tx)
            .await?;
    }
    for f in &n.files {
        sqlx::query(
            "INSERT IGNORE INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?,?,?,?)",
        )
        .bind(&n.id)
        .bind(&f.mime_type)
        .bind(mime_category(&f.mime_type))
        .bind(f.is_sensitive)
        .execute(&mut **tx)
        .await?;
    }

    // 旧行の掃除。Postgresの`= ANY($N)`配列バインドはMySQLで使えないため、
    // `NOT IN (?,?,...)`を要素数ぶん動的に組み立てる(Global Constraints参照)。
    // 集合が空の場合は`NOT IN ()`が構文エラーになるため、全削除のSQLへ分岐する。
    delete_stale_by_key(tx, "note_reaction", "emoji_key", &n.id, &n.reactions.keys().cloned().collect::<Vec<_>>()).await?;
    delete_stale_by_key(tx, "note_tag", "tag", &n.id, &n.tags).await?;
    delete_stale_by_key(tx, "note_mention", "user_id", &n.id, &n.mentions).await?;
    delete_stale_by_key(tx, "note_emoji", "emoji", &n.id, &n.emojis.keys().cloned().collect::<Vec<_>>()).await?;

    // note_fileは複合キーで`NOT IN`が使えないため、SQLite/Postgres版と同様に
    // 既存行を取得してRust側で差分判定し、現存しないキーの組だけ個別にDELETEする。
    let current_file_keys: std::collections::HashSet<(String, String, bool)> = n
        .files
        .iter()
        .map(|f| (f.mime_type.clone(), mime_category(&f.mime_type).to_string(), f.is_sensitive))
        .collect();
    let existing_files: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = ?",
    )
    .bind(&n.id)
    .fetch_all(&mut **tx)
    .await?;
    for (mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = (mime_type.clone(), mime_category_val.clone(), is_sensitive);
        if !current_file_keys.contains(&key) {
            sqlx::query(
                "DELETE FROM note_file WHERE note_id = ? AND mime_type = ? AND mime_category = ? AND is_sensitive = ?",
            )
            .bind(&n.id)
            .bind(&mime_type)
            .bind(&mime_category_val)
            .bind(is_sensitive)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

/// `table`のうち`note_id = note_id`で`key_col`の値が`current_keys`に含まれない行を削除する
/// (取り消されたリアクション/タグ/メンション/絵文字の掃除)。`current_keys`が空なら
/// `NOT IN ()`が構文エラーになるため、`key_col IS NOT NULL`(=全行削除)に分岐する。
///
/// `table`/`key_col`は呼び出し元(`"note_reaction"`/`"emoji_key"`等)がすべてリテラルで
/// 渡す固定値であり、`current_keys`(ユーザーデータ由来)は`?`プレースホルダ経由の`.bind()`
/// でのみ渡すため、`sqlx::AssertSqlSafe`でのSQL文字列化は安全(SQLインジェクション監査済み)。
async fn delete_stale_by_key(
    tx: &mut sqlx::MySqlTransaction<'_>,
    table: &str,
    key_col: &str,
    note_id: &str,
    current_keys: &[String],
) -> Result<()> {
    if current_keys.is_empty() {
        let sql = format!("DELETE FROM {table} WHERE note_id = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql)).bind(note_id).execute(&mut **tx).await?;
        return Ok(());
    }
    let placeholders = current_keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM {table} WHERE note_id = ? AND {key_col} NOT IN ({placeholders})");
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(note_id);
    for k in current_keys {
        query = query.bind(k);
    }
    query.execute(&mut **tx).await?;
    Ok(())
}

/// 単発呼び出し用の薄いラッパー(`PostgresBackend::upsert_note`と同じ規約)。
async fn upsert_note(pool: &sqlx::MySqlPool, n: &Note) -> Result<()> {
    upsert_note_users(pool, n).await?;
    let mut tx = pool.begin().await?;
    upsert_note_tx(&mut tx, n).await?;
    tx.commit().await?;
    Ok(())
}

async fn self_heal_node(pool: &sqlx::MySqlPool, node: &mut serde_json::Value) -> Result<bool> {
    let mut changed = false;
    if let Some(user_value) = node.get("user").cloned() {
        if crate::store::user_ref::is_legacy_full_user(&user_value) {
            if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                crate::store::mysql_user_ref::fill_user_from_snapshot(pool, &user).await?;
                if let Some(id) = user_value.get("id").cloned() {
                    node["user"] = serde_json::json!({ "id": id });
                    changed = true;
                }
            }
        }
    }
    if node.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        changed |= Box::pin(self_heal_node(pool, &mut node["renote"])).await?;
    }
    Ok(changed)
}

async fn self_heal_legacy_row(pool: &sqlx::MySqlPool, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(pool, value).await?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        sqlx::query("UPDATE note SET payload = ? WHERE id = ?").bind(new_payload).bind(note_id).execute(pool).await?;
    }
    Ok(())
}

async fn resolve_payload_rows(pool: &sqlx::MySqlPool, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
    let mut values: Vec<(String, serde_json::Value)> = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(mut v) => {
                if crate::store::user_ref::has_legacy_full_user(&v) {
                    self_heal_legacy_row(pool, &id, &mut v).await?;
                }
                values.push((id, v));
            }
            Err(e) => log::warn!("skipping note cache row {id} with unparsable payload: {e}"),
        }
    }

    let mut ids = Vec::new();
    for (_, v) in &values {
        crate::store::user_ref::collect_user_id_refs(v, &mut ids);
    }
    ids.sort();
    ids.dedup();
    let users = crate::store::mysql_user_ref::fetch_users_by_ids(pool, &ids).await?;

    let mut out = Vec::with_capacity(values.len());
    for (id, mut v) in values {
        if !crate::store::user_ref::hydrate_user_refs(&mut v, &users) {
            log::warn!("skipping note cache row {id}: referenced user not found in user table");
            continue;
        }
        match serde_json::from_value::<Note>(v) {
            Ok(note) => out.push(note),
            Err(e) => log::warn!("skipping note cache row {id} with undeserializable payload: {e}"),
        }
    }
    Ok(out)
}

#[async_trait::async_trait]
impl NoteCacheBackend for MySqlBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let now = crate::store::note_cache::now_epoch();
        for n in notes {
            upsert_note_users(&self.pool, n).await?;
        }
        let mut tx = self.pool.begin().await?;
        for n in notes {
            upsert_note_tx(&mut tx, n).await?;
            sqlx::query(
                "INSERT IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?,?,?,?)",
            )
            .bind(column_id)
            .bind(&n.id)
            .bind(now)
            .bind(n.created_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?",
        )
        .bind(column_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        resolve_payload_rows(&self.pool, rows).await
    }

    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        // `note_id < ?`はMySQLのデフォルト照合順序に依存する(Global Constraints参照)。
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ? AND cn.note_id < ?
             ORDER BY cn.note_id DESC
             LIMIT ?",
        )
        .bind(column_id)
        .bind(until_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = resolve_payload_rows(&self.pool, rows).await?;
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }

    async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, payload FROM note WHERE id = ?").bind(note_id).fetch_optional(&self.pool).await?;
        Ok(match row {
            Some(r) => resolve_payload_rows(&self.pool, vec![r]).await?.into_iter().next(),
            None => None,
        })
    }

    async fn update_note(&self, note: &Note) -> Result<()> {
        upsert_note(&self.pool, note).await
    }

    async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM column_note WHERE column_id = ?").bind(column_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM column_source_boundary WHERE column_id = ?").bind(column_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>> {
        let rows: Vec<(i32, String)> = sqlx::query_as(
            "SELECT source_idx, oldest_fetched_id FROM column_source_boundary
             WHERE column_id = ? ORDER BY source_idx",
        )
        .bind(column_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(i, b)| (i as u32, b)).collect())
    }

    async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM column_source_boundary WHERE column_id = ?")
            .bind(column_id)
            .execute(&mut *tx)
            .await?;
        for (idx, id) in entries {
            sqlx::query("INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES (?,?,?)")
                .bind(column_id)
                .bind(*idx as i32)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        // LEAST(...)はMySQLも同名関数をサポートするため変更不要(Global Constraints参照)。
        let mut tx = self.pool.begin().await?;
        for (idx, id) in entries {
            sqlx::query(
                "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES (?,?,?)
                 ON DUPLICATE KEY UPDATE
                    oldest_fetched_id = LEAST(oldest_fetched_id, VALUES(oldest_fetched_id))",
            )
            .bind(column_id)
            .bind(*idx as i32)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        sqlx::query("DELETE FROM column_source_boundary").execute(&self.pool).await?;
        Ok(())
    }

    async fn note_count(&self) -> Result<i32> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note").fetch_one(&self.pool).await?;
        Ok(count as i32)
    }

    async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM note WHERE created_at >= ?").bind(since_epoch_secs as i64).fetch_one(&self.pool).await?;
        Ok(count as i32)
    }

    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        prune_impl(&self.pool, keep, max_age_days, max_size_mb).await
    }

    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        search_cache_impl(&self.pool, where_sql, until_id, limit).await
    }
}

/// SQLite版`delete_matching`と等価。TEMP TABLEを使わずRust側で対象IDを
/// `Vec<String>`として確定してから`IN (?,?,...)`でDELETEする
/// (プールの別コネクションに割り当てられてもTEMP TABLEが「無い」扱いになる問題を回避)。
/// `tx`は呼び出し元が開始したトランザクション。
///
/// MySQLのプリペアドステートメントはプレースホルダ数に65,535個の上限
/// (`ER_PS_MANY_PARAM`/エラー1390)があり、`ids`を丸ごと1本の`IN (?,...)`に
/// 展開すると`note_cache_limit: 0`(無制限、仕様上有効な設定)と経過日数プルーン
/// (`SELECT id FROM note WHERE created_at < ?`に`LIMIT`が無い)の組み合わせで
/// noteテーブルが65,536行を超えて成長した場合に到達しうる。`CHUNK_SIZE`ごとに
/// 分割して`delete_matching_ids_chunk`を繰り返し呼び、削除件数・影響カラム・
/// カラムごとの最大削除IDをチャンク間でマージしてから、fetch_boundary調整を
/// 1回だけ実行する(調整ロジック自体はチャンク化前と同一)。
const DELETE_MATCHING_IDS_CHUNK_SIZE: usize = 1_000;

async fn delete_matching_ids(tx: &mut sqlx::MySqlTransaction<'_>, ids: &[String]) -> Result<i64> {
    delete_matching_ids_with_chunk_size(tx, ids, DELETE_MATCHING_IDS_CHUNK_SIZE).await
}

async fn delete_matching_ids_with_chunk_size(
    tx: &mut sqlx::MySqlTransaction<'_>,
    ids: &[String],
    chunk_size: usize,
) -> Result<i64> {
    if ids.is_empty() {
        return Ok(0);
    }
    debug_assert!(chunk_size > 0, "chunk_size must be positive");

    let mut deleted: i64 = 0;
    let mut affected_columns: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut max_deleted_by_column: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for chunk in ids.chunks(chunk_size.max(1)) {
        let (chunk_deleted, chunk_affected_columns, chunk_max_deleted_by_column) =
            delete_matching_ids_chunk(tx, chunk).await?;
        deleted += chunk_deleted;
        for column_id in chunk_affected_columns {
            affected_columns.insert(column_id);
        }
        for (column_id, max_deleted) in chunk_max_deleted_by_column {
            max_deleted_by_column
                .entry(column_id)
                .and_modify(|existing| {
                    if max_deleted.as_str() > existing.as_str() {
                        *existing = max_deleted.clone();
                    }
                })
                .or_insert(max_deleted);
        }
    }

    for column_id in &affected_columns {
        let survivor: Option<(String,)> = sqlx::query_as(
            "SELECT note_id FROM column_note WHERE column_id = ? ORDER BY note_id ASC LIMIT 1",
        )
        .bind(column_id)
        .fetch_optional(&mut **tx)
        .await?;
        match survivor.map(|(s,)| s) {
            Some(oldest) => {
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                sqlx::query(
                    "UPDATE column_source_boundary SET oldest_fetched_id = ? WHERE column_id = ? AND oldest_fetched_id < ?",
                )
                .bind(&candidate)
                .bind(column_id)
                .bind(&candidate)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM column_source_boundary WHERE column_id = ?").bind(column_id).execute(&mut **tx).await?;
            }
        }
    }
    Ok(deleted)
}

/// `delete_matching_ids_with_chunk_size`の1チャンク分の実処理。`ids`は事前に
/// `chunk_size`以下に分割済みであることを前提とする(プレースホルダ数の上限はここでは
/// チェックしない、呼び出し元が保証する)。noteおよび側テーブルからの削除と、
/// このチャンクで影響を受けたカラムID・カラムごとの削除ID最大値の収集のみを行い、
/// fetch_boundaryの調整は呼び出し元が全チャンク分をマージしてから1回だけ行う。
///
/// この関数内で`format!`組み立てのSQLに`sqlx::AssertSqlSafe`を使っている箇所は、
/// `placeholders`が`ids.len()`個の`?`を繰り返し連結しただけ(値そのものは含まない)、
/// `table`はソース中に直書きしたテーブル名のリスト由来で、`ids`の各値は必ず`.bind()`
/// 経由で渡すため、SQLインジェクションの懸念はない(監査済み)。
async fn delete_matching_ids_chunk(
    tx: &mut sqlx::MySqlTransaction<'_>,
    ids: &[String],
) -> Result<(i64, Vec<String>, std::collections::HashMap<String, String>)> {
    if ids.is_empty() {
        return Ok((0, Vec::new(), std::collections::HashMap::new()));
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let sql = format!("DELETE FROM note WHERE id IN ({placeholders})");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        q = q.bind(id);
    }
    let deleted = q.execute(&mut **tx).await?.rows_affected() as i64;

    let sql = format!("SELECT DISTINCT column_id FROM column_note WHERE note_id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(sql));
    for id in ids {
        q = q.bind(id);
    }
    let affected_columns: Vec<(String,)> = q.fetch_all(&mut **tx).await?;

    // MIN/MAX(note_id)による大小比較も、load_cached_beforeと同様MySQLのデフォルト
    // 照合順序に依存する(Global Constraints参照)。
    let sql = format!("SELECT column_id, MAX(note_id) FROM column_note WHERE note_id IN ({placeholders}) GROUP BY column_id");
    let mut q = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql));
    for id in ids {
        q = q.bind(id);
    }
    let max_deleted_rows: Vec<(String, String)> = q.fetch_all(&mut **tx).await?;
    let max_deleted_by_column: std::collections::HashMap<String, String> = max_deleted_rows.into_iter().collect();

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        let sql = format!("DELETE FROM {table} WHERE note_id IN ({placeholders})");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in ids {
            q = q.bind(id);
        }
        q.execute(&mut **tx).await?;
    }

    Ok((deleted, affected_columns.into_iter().map(|(c,)| c).collect(), max_deleted_by_column))
}

async fn prune_impl(pool: &sqlx::MySqlPool, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
    let mut deleted: i64 = 0;
    let mut tx = pool.begin().await?;

    if max_age_days > 0 {
        let cutoff = crate::store::note_cache::now_epoch() - max_age_days as i64 * 86_400;
        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM note WHERE created_at < ?").bind(cutoff).fetch_all(&mut *tx).await?;
        let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
        deleted += delete_matching_ids(&mut tx, &ids).await?;
    }
    if keep > 0 {
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note").fetch_one(&mut *tx).await?;
        let overflow = total - keep as i64;
        if overflow > 0 {
            let ids: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT ?").bind(overflow).fetch_all(&mut *tx).await?;
            let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
            deleted += delete_matching_ids(&mut tx, &ids).await?;
        }
    }
    tx.commit().await?;

    if max_size_mb > 0 {
        // Postgres版と同じ理由でMySQLでもバイト単位のサイズ上限は未対応とする
        // (`information_schema.tables`のサイズ統計は概算値でANALYZE TABLE等の
        // 再集計が必要になり、DELETEだけでは即座に反映されない点もPostgresと同様)。
        log::warn!(
            "max_size_mb is configured but has no effect on the MySQL cache backend \
             (byte-budget pruning is not supported here); use keep/max_age_days instead"
        );
    }
    Ok(deleted as usize)
}

async fn search_cache_impl(
    pool: &sqlx::MySqlPool,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::SqlParam;

    // SqlWhere.sql(`?`プレースホルダ、` REGEXP `)は無変換でそのまま使う
    // (Global Constraints参照、to_mysql_sqlに相当する変換関数は実装しない)。
    // `where_sql.sql`はTQLコンパイラ(`filter/sql.rs`)が生成する固定文字列で、値は
    // 一切埋め込まず`where_sql.params`(下でbind)経由のみで渡す設計のため、
    // `sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN `user` u ON u.id = n.user_id WHERE ({})", where_sql.sql);
    if until_id.is_some() {
        sql.push_str(" AND n.id < ?");
    }
    sql.push_str(" ORDER BY n.created_at DESC, n.id DESC LIMIT ?");

    let mut query = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql));
    for p in &where_sql.params {
        query = match p {
            SqlParam::Text(s) => query.bind(s.clone()),
            SqlParam::Real(x) => query.bind(*x),
        };
    }
    if let Some(u) = until_id {
        query = query.bind(u.to_string());
    }
    query = query.bind(limit);

    let rows = query.fetch_all(pool).await?;
    resolve_payload_rows(pool, rows).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::{mysql::Mysql, testcontainers::{runners::AsyncRunner, ContainerAsync}};

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

    /// Issue #409: 旧スキーマ(`user.instance_*` あり)の既存インストールが、
    /// `ensure_schema` で `instance` テーブルへ移行され、旧列が消え、再実行しても値が戻らないこと。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_migrates_legacy_instance_columns() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        // 旧スキーマの user テーブルを手で作る(instance_* あり、avatar_blurhash は ensure_schema が追加する)
        sqlx::query(
            "CREATE TABLE `user` (
                id VARCHAR(64) PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT, avatar_url TEXT,
                is_bot BOOLEAN NOT NULL DEFAULT FALSE, is_cat BOOLEAN NOT NULL DEFAULT FALSE,
                followers_count BIGINT NOT NULL DEFAULT 0, following_count BIGINT NOT NULL DEFAULT 0,
                notes_count BIGINT NOT NULL DEFAULT 0, emojis TEXT NOT NULL,
                bio LONGTEXT, banner_url TEXT,
                instance_name TEXT, instance_icon_url TEXT, instance_theme_color TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO `user` (id, username, host, emojis, instance_name, instance_icon_url, instance_theme_color) VALUES
              ('u1', 'a', 'remote.example', '{}', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u2', 'b', 'remote.example', '{}', 'Remote', 'https://remote.example/favicon.ico', '#ff8800'),
              ('u3', 'c', 'conflict.example', '{}', 'Aaa', 'https://conflict.example/a.ico', NULL),
              ('u4', 'd', 'conflict.example', '{}', 'Zzz', NULL, '#123456'),
              ('u5', 'e', NULL, '{}', 'Local', NULL, NULL),
              ('u6', 'f', 'empty.example', '{}', NULL, NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        ensure_schema(&pool).await.unwrap();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `instance`").fetch_one(&pool).await.unwrap();
        assert_eq!(total, 2, "ローカル(host NULL)と全列NULLのホストは移さない");
        let (name, icon, theme): (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT name, icon_url, theme_color FROM `instance` WHERE host = 'conflict.example'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name.as_deref(), Some("Zzz"), "食い違う旧行は列ごとのMAXで1行に集約");
        assert_eq!(icon.as_deref(), Some("https://conflict.example/a.ico"));
        assert_eq!(theme.as_deref(), Some("#123456"));

        let old_cols: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_schema = DATABASE() AND table_name = 'user' AND column_name LIKE 'instance\\_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_cols, 0, "旧 instance_* 列は DROP される");

        // 移行後に更新した値は、ensure_schema を再実行しても戻らない
        sqlx::query("UPDATE `instance` SET name = 'Renamed' WHERE host = 'remote.example'")
            .execute(&pool)
            .await
            .unwrap();
        ensure_schema(&pool).await.unwrap();
        let name: String = sqlx::query_scalar("SELECT name FROM `instance` WHERE host = 'remote.example'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Renamed");
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

    fn note(id: &str, created_at: i64) -> Note {
        use crate::domain::{DriveFile, User, Visibility};
        Note {
            id: id.into(),
            created_at,
            text: Some("hello https://example.com #rust".into()),
            cw: None,
            visibility: Visibility::Home,
            local_only: false,
            user: User {
                id: "u1".into(), username: "alice".into(), host: None, name: Some("Alice".into()),
                avatar_url: None, is_bot: false, is_cat: false,
                followers_count: 5, following_count: 3, notes_count: 42,
                emojis: std::collections::HashMap::new(), bio: None, banner_url: None, avatar_blurhash: None, instance: None,
            },
            reply_id: None, reply_user_id: None, renote_id: None, renote: None,
            files: vec![DriveFile { id: "f1".into(), mime_type: "image/png".into(), is_sensitive: false, url: "http://x/f1".into(), thumbnail_url: None, name: "f1.png".into(), size: None }],
            poll: None, tags: vec!["rust".into()], mentions: vec![],
            emojis: std::collections::HashMap::new(), channel_id: None, via: None, lang: None,
            reactions: std::collections::HashMap::from([("👍".into(), 3u32)]),
            reaction_count: 3, renote_count: 1, reply_count: 0,
            my_reaction: Some("👍".into()), is_renoted_by_me: false, is_favorited_by_me: false, is_pinned: false,
        }
    }

    /// 実DBテスト用のバックエンド。コンテナを一緒に持ち、drop でコンテナと匿名ボリュームを消す。
    /// フィールドは宣言順に drop されるので、接続プールを閉じてからコンテナを消す。
    struct TestBackend {
        backend: MySqlBackend,
        _container: ContainerAsync<Mysql>,
    }

    impl std::ops::Deref for TestBackend {
        type Target = MySqlBackend;
        fn deref(&self) -> &MySqlBackend {
            &self.backend
        }
    }

    async fn backend() -> TestBackend {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let params = MySqlConnectParams { host: "127.0.0.1".into(), port, database: "test".into(), user: "root".into(), password: "".into() };
        let backend = MySqlBackend::connect(&params).await.unwrap();
        TestBackend { backend, _container: container }
    }

    fn b(idx: u32, id: &str) -> (u32, String) {
        (idx, id.to_string())
    }

    /// source_idx=0 だけの境界を置き換える(単一ソース時代のテストを新APIへ移すためのヘルパー)。
    async fn set0(s: &MySqlBackend, column_id: &str, id: &str) {
        s.replace_fetch_boundaries(column_id, &[b(0, id)]).await.unwrap();
    }

    async fn extend0(s: &MySqlBackend, column_id: &str, id: &str) {
        s.extend_fetch_boundaries(column_id, &[b(0, id)]).await.unwrap();
    }

    async fn b0(s: &MySqlBackend, column_id: &str) -> Option<String> {
        s.get_fetch_boundaries(column_id).await.unwrap().into_iter().find(|(i, _)| *i == 0).map(|(_, v)| v)
    }

    #[tokio::test]
    #[ignore]
    async fn cache_roundtrip_preserves_note_and_order() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 150)]).await.unwrap();
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n3", "n1"]);
        assert_eq!(got[0].reactions.get("👍"), Some(&3));
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_note_removes_stale_reaction_after_unreact() {
        let s = backend().await;
        let mut n = note("n1", 100);
        n.reactions = std::collections::HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).await.unwrap();

        n.reactions = std::collections::HashMap::new();
        n.reaction_count = 0;
        n.my_reaction = None;
        s.update_note(&n).await.unwrap();

        let (rc,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'").fetch_one(s.pool()).await.unwrap();
        assert_eq!(rc, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_note_stores_reply_user_id_column() {
        let s = backend().await;
        let mut n = note("n1", 100);
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some("bob-id".into());
        s.cache_note("col1", &n).await.unwrap();

        let (stored,): (Option<String>,) =
            sqlx::query_as("SELECT reply_user_id FROM note WHERE id = 'n1'").fetch_one(s.pool()).await.unwrap();
        assert_eq!(stored.as_deref(), Some("bob-id"));
    }

    #[tokio::test]
    #[ignore]
    async fn prune_removes_oldest_beyond_keep_and_related_rows() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        let deleted = s.prune(2, 0, 0).await.unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(s.note_count().await.unwrap(), 2);
        let (rc,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'").fetch_one(s.pool()).await.unwrap();
        assert_eq!(rc, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn search_cache_applies_predicate_and_until_id_boundary() {
        use crate::filter::{parser, sql};
        let s = backend().await;
        s.cache_notes("col1", &[note("a1", 300), note("a2", 200), note("a3", 100)]).await.unwrap();
        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, Some("a3"), 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a1", "a2"]);
    }

    /// `search_cache_applies_boolean_predicates_without_type_error`(Postgres版)相当:
    /// `has_poll`/`pinned`/`bot`のようなbool系TQL述語がMySQLでもエラーにならず
    /// 正しくフィルタされることを確認する(Phase 2の最終レビューでCritical不具合が
    /// 見つかったのと同じ種類の述語)。
    #[tokio::test]
    #[ignore]
    async fn search_cache_applies_boolean_predicates_without_type_error() {
        use crate::domain::{Poll, PollChoice};
        use crate::filter::{parser, sql};
        let s = backend().await;

        let mut with_poll = note("p1", 300);
        with_poll.poll = Some(Poll {
            choices: vec![
                PollChoice { text: "a".into(), votes: 0, is_voted: false },
                PollChoice { text: "b".into(), votes: 0, is_voted: false },
            ],
            multiple: false,
            expires_at: None,
        });
        let mut without_poll = note("p2", 200);
        without_poll.poll = None;
        s.cache_notes("col1", &[with_poll, without_poll]).await.unwrap();

        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };

        // has_poll = 1 相当
        let expr = parser::parse_predicate("has_poll").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["p1"]);

        // pinned = 1 相当
        let mut pinned = note("p3", 100);
        pinned.is_pinned = true;
        s.cache_note("col1", &pinned).await.unwrap();
        let expr = parser::parse_predicate("pinned").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["p3"]);

        // bot = 1 相当(u.is_bot)。他のテストノートと同じuser id("u1")を使うと、
        // usersテーブルの同一行を共有しているため`is_bot`の上書きが全ノートに波及して
        // しまう(userはnote単位ではなくid単位でupsertされる)。別ユーザーIDを使う。
        let mut bot_note = note("p4", 50);
        bot_note.user.id = "u_bot".into();
        bot_note.user.is_bot = true;
        s.cache_note("col1", &bot_note).await.unwrap();
        let expr = parser::parse_predicate("bot").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["p4"]);
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_boundary_roundtrip_and_extend_only_moves_older() {
        let s = backend().await;
        assert!(b0(&s, "col1").await.is_none());
        set0(&s, "col1", "n500").await;
        assert_eq!(b0(&s, "col1").await.as_deref(), Some("n500"));
        extend0(&s, "col1", "n300").await;
        assert_eq!(b0(&s, "col1").await.as_deref(), Some("n300"));
        extend0(&s, "col1", "n800").await;
        assert_eq!(b0(&s, "col1").await.as_deref(), Some("n300"));
    }

    /// レビュー指摘の本丸(Fix 2): `delete_matching_ids`が`ids`を1本の`IN (?,...)`に
    /// 展開すると、MySQLのプリペアドステートメント65,535プレースホルダ上限
    /// (`ER_PS_MANY_PARAM`)を超えうる。`delete_matching_ids_with_chunk_size`で
    /// 小さいチャンクサイズ(1)と、全件が1チャンクに収まる大きいチャンクサイズを
    /// それぞれ与えて同じ削除セットを処理し、削除件数・残存ノート・
    /// fetch_boundaryの調整結果がチャンク数に関わらず一致することを確認する
    /// (MAX(note_id)のチャンクをまたいだマージが正しく機能していることの検証)。
    async fn setup_chunking_fixture(s: &MySqlBackend) {
        // col1: n1..n5の5件。col2: n2,n4のみ(2列にまたがるnoteを含める)。
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300), note("n4", 400), note("n5", 500)]).await.unwrap();
        s.cache_notes("col2", &[note("n2", 200), note("n4", 400)]).await.unwrap();
        set0(&s, "col1", "n1").await;
        set0(&s, "col2", "n2").await;
    }

    async fn delete_via_chunk_size(s: &MySqlBackend, ids: &[String], chunk_size: usize) -> i64 {
        let mut tx = s.pool().begin().await.unwrap();
        let deleted = delete_matching_ids_with_chunk_size(&mut tx, ids, chunk_size).await.unwrap();
        tx.commit().await.unwrap();
        deleted
    }

    #[tokio::test]
    #[ignore]
    async fn delete_matching_ids_chunking_matches_single_chunk_result() {
        let ids: Vec<String> = ["n1", "n2", "n3"].iter().map(|s| s.to_string()).collect();

        // chunk_size=1: 各idが個別チャンクになる(最も細かい分割)。
        let s_chunked = backend().await;
        setup_chunking_fixture(&s_chunked).await;
        let deleted_chunked = delete_via_chunk_size(&s_chunked, &ids, 1).await;

        // chunk_size=10: 全件が1チャンクに収まる(チャンク化前と同じ挙動)。
        let s_single = backend().await;
        setup_chunking_fixture(&s_single).await;
        let deleted_single = delete_via_chunk_size(&s_single, &ids, 10).await;

        assert_eq!(deleted_chunked, 3);
        assert_eq!(deleted_chunked, deleted_single);

        assert_eq!(s_chunked.note_count().await.unwrap(), s_single.note_count().await.unwrap());
        assert_eq!(s_chunked.note_count().await.unwrap(), 2); // n4, n5が残存

        // col1: 残存はn4,n5 → MIN=n4。max_deleted_by_column[col1]=n3 < n4なのでoldest=n4を採用。
        // col1の元boundaryはn1(<n4)なので更新される。
        assert_eq!(b0(&s_chunked, "col1").await.as_deref(), Some("n4"));
        assert_eq!(b0(&s_single, "col1").await.as_deref(), Some("n4"));

        // col2: 残存はn4のみ → MIN=n4。max_deleted_by_column[col2]=n2 < n4なのでoldest=n4を採用。
        // col2の元boundaryはn2(<n4)なので更新される。
        assert_eq!(b0(&s_chunked, "col2").await.as_deref(), Some("n4"));
        assert_eq!(b0(&s_single, "col2").await.as_deref(), Some("n4"));
    }

    /// 全件削除でカラムの生存noteがゼロになった場合、fetch_boundary行自体が
    /// 削除されることをチャンク化後も確認する(チャンク境界をまたいでも
    /// `affected_columns`/`max_deleted_by_column`のマージが正しいことの追加確認)。
    #[tokio::test]
    #[ignore]
    async fn delete_matching_ids_chunking_removes_boundary_when_column_emptied() {
        let s = backend().await;
        setup_chunking_fixture(&s).await;
        let ids: Vec<String> = ["n1", "n2", "n3", "n4", "n5"].iter().map(|s| s.to_string()).collect();

        let deleted = delete_via_chunk_size(&s, &ids, 2).await;

        assert_eq!(deleted, 5);
        assert_eq!(s.note_count().await.unwrap(), 0);
        assert!(b0(&s, "col1").await.is_none());
        assert!(b0(&s, "col2").await.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_boundaries_multi_source_roundtrip_extend_and_prune() {
        let s = backend().await;
        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());

        s.replace_fetch_boundaries("col1", &[b(1, "n2"), b(0, "n1"), b(2, "")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n1"), b(1, "n2"), b(2, "")]);

        s.extend_fetch_boundaries("col1", &[b(0, "n0"), b(1, "n9"), b(3, "n5")]).await.unwrap();
        assert_eq!(
            s.get_fetch_boundaries("col1").await.unwrap(),
            vec![b(0, "n0"), b(1, "n2"), b(2, ""), b(3, "n5")]
        );

        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "")]).await.unwrap();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        assert_eq!(s.prune(2, 0, 0).await.unwrap(), 1);
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n2"), b(1, "n2")]);

        s.clear_column_notes("col1").await.unwrap();
        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn extend_fetch_boundaries_moves_to_empty_string_and_never_back() {
        let s = backend().await;
        s.replace_fetch_boundaries("col1", &[b(0, "n500"), b(1, "n300")]).await.unwrap();

        // 既存行を ""(枯渇済み)へ延長できる
        s.extend_fetch_boundaries("col1", &[b(0, "")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, ""), b(1, "n300")]);

        // 枯渇済みは、後から通常のIDで延長しても戻らない
        s.extend_fetch_boundaries("col1", &[b(0, "n1")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, ""), b(1, "n300")]);
    }

    #[tokio::test]
    #[ignore]
    async fn prune_removes_every_source_row_when_multi_source_column_fully_pruned() {
        let s = backend().await;
        s.cache_notes("col1", &[note("old", 100)]).await.unwrap();
        s.replace_fetch_boundaries("col1", &[b(0, "old"), b(1, ""), b(2, "old")]).await.unwrap();

        assert_eq!(s.prune(0, 1, 0).await.unwrap(), 1);

        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    /// 重複した source_idx は主キー違反で失敗し、トランザクションごとロールバックされて既存行が残る。
    #[tokio::test]
    #[ignore]
    async fn replace_fetch_boundaries_rejects_duplicate_source_idx_and_rolls_back() {
        let s = backend().await;
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "n2")]).await.unwrap();

        let result = s.replace_fetch_boundaries("col1", &[b(0, "n5"), b(0, "n6")]).await;

        assert!(result.is_err());
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n1"), b(1, "n2")]);
    }

    #[tokio::test]
    #[ignore]
    async fn ensure_schema_migrates_legacy_fetch_boundary_table() {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();

        sqlx::query("CREATE TABLE column_fetch_boundary (column_id VARCHAR(64) PRIMARY KEY, oldest_fetched_id TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO column_fetch_boundary VALUES ('c1', 'n100'), ('c2', 'n200')")
            .execute(&pool)
            .await
            .unwrap();

        ensure_schema(&pool).await.unwrap();

        let rows: Vec<(String, i32, String)> = sqlx::query_as(
            "SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary ORDER BY column_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![("c1".to_string(), 0, "n100".to_string()), ("c2".to_string(), 0, "n200".to_string())]
        );
        let legacy: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'column_fetch_boundary' AND table_schema = 'test'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(legacy, 0, "旧テーブルは DROP される");

        sqlx::query("UPDATE column_source_boundary SET oldest_fetched_id = 'n50' WHERE column_id = 'c1'")
            .execute(&pool)
            .await
            .unwrap();
        ensure_schema(&pool).await.unwrap();
        let v: String = sqlx::query_scalar("SELECT oldest_fetched_id FROM column_source_boundary WHERE column_id = 'c1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(v, "n50");
    }

    /// 境界の書きロックの中で `clear_all_fetch_boundaries` を実行しても、実DBで詰まらず、全カラムの境界が
    /// 空になり、古い世代の書き込みが境界を復活させない(Issue #456。#453 の最終レビュー指摘)。
    ///
    /// 順序は、時間ではなく、合図(`mpsc` と `Semaphore`)で決める。読みロックを持つ書き込みが
    /// DB に書く直前で止まっている間に、`invalidate_boundaries` が書きロックを要求し、待たされる。
    /// その後で書き込みを放すと、書き込みが先に完了し(`boundaries_ok` は真のまま)、`clear` はその後に走る。
    /// 書きロックが無ければ、`clear` が書き込みより先に走り、後から入る書き込みが境界を復活させる。
    #[tokio::test]
    #[ignore]
    async fn clear_all_fetch_boundaries_inside_the_boundary_write_lock_completes_and_blocks_stale_writes() {
        use crate::fence::ColumnFence;
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::sync::{mpsc, Semaphore};

        let s = Arc::new(backend().await);
        let fence = Arc::new(ColumnFence::default());
        let columns = ["c1", "c2", "c3", "c4"];
        for c in columns {
            set0(&s, c, "n500").await;
        }
        let (ready_tx, mut ready_rx) = mpsc::unbounded_channel();
        let gate = Arc::new(Semaphore::new(0));
        // 読みロックを持ったまま、DB に書く直前で止まる、実行中の取得
        let mut writers = Vec::new();
        for c in columns {
            let epoch = fence.begin(c);
            let (s, fence, ready_tx, gate) = (Arc::clone(&s), Arc::clone(&fence), ready_tx.clone(), Arc::clone(&gate));
            writers.push(tokio::spawn(async move {
                fence
                    .write_if_current(c, &epoch, |boundaries_ok| async move {
                        ready_tx.send(()).unwrap();
                        gate.acquire().await.unwrap().forget();
                        if boundaries_ok {
                            s.extend_fetch_boundaries(c, &[b(0, "n100")]).await.unwrap();
                        }
                        boundaries_ok
                    })
                    .await
            }));
        }
        for _ in columns {
            ready_rx.recv().await.unwrap(); // 全員が、読みロックを持った
        }
        let late_epoch = fence.begin("c1"); // invalidate_boundaries より前に控える

        let invalidate = {
            let (s, fence) = (Arc::clone(&s), Arc::clone(&fence));
            tokio::spawn(async move { fence.invalidate_boundaries(|| async { s.clear_all_fetch_boundaries().await }).await })
        };
        tokio::task::yield_now().await;
        tokio::task::yield_now().await; // invalidate_boundaries が、書きロックの待ちに入る
        tokio::time::sleep(Duration::from_millis(200)).await; // 書きロックが無い実装なら、この間に clear が終わる
        assert!(!invalidate.is_finished(), "書きロックは、読みロックを持つ実行中の書き込みの完了を待つ");

        gate.add_permits(columns.len()); // 実行中の書き込みを放す
        let cleared = tokio::time::timeout(Duration::from_secs(30), invalidate).await; // デッドロック検出の上限
        assert!(cleared.expect("書きロックの中の clear_all_fetch_boundaries が詰まらない").unwrap().is_ok());
        for writer in writers {
            assert_eq!(writer.await.unwrap(), Some(true), "実行中の書き込みは、書きロックより先に完了する");
        }
        for c in columns {
            assert!(s.get_fetch_boundaries(c).await.unwrap().is_empty(), "{c}: 書き込みの後で clear が走り、空になる");
        }
        let late = fence.write_if_current("c1", &late_epoch, |boundaries_ok| async move { boundaries_ok }).await;
        assert_eq!(late, Some(false), "invalidate_boundaries より前に控えた世代の境界の書き込みは、古い扱い");
    }
}
