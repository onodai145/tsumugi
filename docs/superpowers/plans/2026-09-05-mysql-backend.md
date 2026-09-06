# MySqlBackend (Issue #115 Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** note cache のバックエンドとして MySQL/MariaDB を選択できるようにする(`PostgresBackend`と並ぶ3つ目の`NoteCacheBackend`実装 `MySqlBackend` を追加し、設定画面から SQLite/PostgreSQL/MySQL を再起動なしに切り替えられるようにする)。

**Architecture:** `store/mysql_backend.rs`(接続・DDL・`NoteCacheBackend` trait実装)+ `store/mysql_user_ref.rs`(user正規化テーブルの読み書き)を新規追加する。これらは`store/postgres_backend.rs`/`store/postgres_user_ref.rs`と同じ形だが、MySQL固有のSQL方言(`ON DUPLICATE KEY UPDATE`、`IN (?,?,...)`手動組み立て、`?`ネイティブプレースホルダ)に翻訳する。`domain::CacheBackendConfig`に`MySql`variantを追加し、既存の`NoteCacheStore::swap_backend`/`SettingsStore::load_cache_backend`/`save_cache_backend`/keyringパスワード関数(いずれもPhase 2で汎用に作られている)はそのまま再利用する。

**Tech Stack:** `sqlx`(MySQLドライバ、`tokio`ランタイム)、`sea-query`(DDLのみ)、`testcontainers-modules`(mysql feature、使い捨てMySQL統合テスト用)。既存: `PostgresBackend`(Phase 2、実装パターンの参照元)、`domain::CacheBackendConfig`/`SettingsStore`/`session`モジュール(拡張のみ、新規実装なし)。

## Global Constraints

以下は `docs/superpowers/specs/2026-09-03-external-note-cache-db-design.md` の「Phase 3設計: MySqlBackend」節で確定した値・事前スパイクで実DB(Docker上のMySQL 8.0)で検証済みの内容。全タスクに共通して適用される。

- 追加する依存クレートと feature は正確に以下の通り(スパイクで動作確認済み):
  - `sqlx = { version = "0.8.6", default-features = false, features = ["mysql", "runtime-tokio", "tls-rustls"] }`
  - `sea-query`は既存の依存(Phase 2で追加済み、`features = ["backend-postgres", "derive"]`)に`backend-mysql`を追加するだけでよい(`sea-query = { version = "0.32.7", default-features = false, features = ["backend-postgres", "backend-mysql", "derive"] }`)。新たに`sea-query-binder`は使わない(Postgres版と同じ理由: CRUD文は手書きSQLで書くため不要。Phase 2の最終レビューで`sea-query-binder`自体が不要と判明し依存から削除済み)
- **`sqlx`の`chrono`/`json`featureは絶対に有効化しない**(Postgresと全く同じ理由で`sqlx-sqlite`が依存グラフに要求され`rusqlite`と`links`競合する。スパイクで再現確認済み)。タイムスタンプは`BIGINT`、JSON payloadは`TEXT`(`serde_json`手動シリアライズ)、Postgres版と同じ規約。
- **DDL(テーブル作成)は`sea-query`の`Table::create()`/`Index::create()`で書く**(`MysqlQueryBuilder`を使う)。CRUD文(INSERT/SELECT/UPDATE/DELETE)は手書きSQL文字列 + `sqlx::query()`のバインド方式で書く(Postgres版と同じ方針)。
- **プレースホルダは`?`をそのまま使う(MySQLはネイティブに`?`をサポートし、`$1`のような番号付けは不要)**。Postgres版の`$1,$2,...`はすべて単なる`?`の繰り返しに置き換える。
- **UPSERT構文はPostgresと異なる(スパイクで実証済み)**:
  - `ON CONFLICT (col) DO UPDATE SET x = excluded.x` → `ON DUPLICATE KEY UPDATE x = VALUES(x)`
  - `ON CONFLICT (...) DO NOTHING` → `INSERT IGNORE INTO ...`
  - 複合主キーの`DO NOTHING`相当(例: `column_note`)は`INSERT IGNORE INTO column_note (...) VALUES (...)`を使う(Postgres版は`ON CONFLICT DO NOTHING`だった箇所)
  - `VALUES()`関数はMySQL 8.0.19以降で非推奨(deprecated)警告が出るが、削除はされておらず、MariaDBでは非推奨扱いですらない。本プロジェクトはMySQL/MariaDB両対応を掲げているため、より新しい構文(`INSERT ... AS new_alias ON DUPLICATE KEY UPDATE col = new_alias.col`、MySQL 8.0.19+限定でMariaDB非対応)ではなく、**互換性の高い`VALUES()`関数を使う**(deprecation警告は許容する)
- **配列バインドは使えない(スパイクで実証済み)**: Postgresの`WHERE col = ANY($1)`(`sqlx`が`Vec<T>`を1つのバインド値として展開する機能)はMySQLドライバでは使えない。`IN (?, ?, ..., ?)`を要素数ぶんRust側で動的に組み立て、要素ごとに`.bind()`する(具体的なコードは各タスクのCRUD実装内で示す)。
- **BOOLEAN列はPostgresと違い型不一致が起きないため、DDLは`sea_query::ColumnDef::boolean()`をそのまま使う**(SMALLINTへの書き換えは不要。MySQLの`BOOLEAN`/`BOOL`は`TINYINT(1)`のエイリアスで、`filter/sql.rs::bool_field`が発行する`col = 1`比較がそのまま通ることをDocker上のMySQL 8.0で実証済み)。
- **`REGEXP`もプレースホルダ同様、変換不要でそのまま使える**(MySQLがネイティブにサポートする中置演算子。`?`バインドでの動作もスパイクで確認済み)。`to_postgres_sql`に相当する変換関数(`to_mysql_sql`)は**実装しない** — `search_cache`は`SqlWhere.sql`を完全に無変換のままバインドする。`filter/sql.rs`・`build_where`は一切変更しない。
- `LEAST(...)`(`extend_fetch_boundary`で使用)はMySQLも同名関数をサポートするため変更不要。
- ID順序比較(`note_id < ?`、`MIN`/`MAX(note_id)`)はMySQLのデフォルト照合順序(大文字小文字を区別しない`utf8mb4_0900_ai_ci`等)に依存する。MisskeyのID(base36/aidx系、常に小文字)であれば大文字小文字非区別は実用上影響しないと考えられるが、Postgres版と同じ「今日的には問題ないが将来ID体系が変わった場合は要再検証」という趣旨のコメントをコードに残す。
- `NoteCacheBackend`トレイト(`store/note_cache.rs`)のシグネチャは変更しない(全15メソッド、Phase 1で確定済み)。
- `sqlx::Error`は既存の`impl From<sqlx::Error> for Error`(Phase 2で`error.rs`に追加済み、Postgres/MySQL共用)をそのまま使う。**このタスクでは`error.rs`を変更しない**。
- keyringパスワード保存(`session::save_cache_backend_password`/`load_cache_backend_password`/`delete_cache_backend_password`)は**Postgres/MySQLで共用の単一スロット**(同時にアクティブにできるバックエンドは1つだけなので、パスワードも1つだけ保存すればよい設計)。**新しい関数は追加しない**、既存の3関数をそのまま呼ぶ。
- `SettingsStore::load_cache_backend`/`save_cache_backend`(`store/settings.rs`)は`CacheBackendConfig`に対して汎用に実装済み。**このタスクでは`settings.rs`を変更しない**(`CacheBackendConfig`enum自体への`MySql` variant追加だけで済む)。
- 既存の`SqliteBackend`(`store/sqlite_backend.rs`)・`PostgresBackend`(`store/postgres_backend.rs`)・`filter/sql.rs`・`store/settings.rs`は本計画では**変更しない**(新規ファイルのみで完結させる。Task 3のみ`domain/cache_backend.rs`・`commands/cache_backend.rs`・`lib.rs`という既存の「切替インフラ」ファイルに追記する)。

---

### Task 1: 依存クレート追加 + `MySqlBackend`接続・DDLの土台

**Files:**
- Modify: `src-tauri/Cargo.toml`(依存追加)
- Create: `src-tauri/src/store/mysql_backend.rs`
- Modify: `src-tauri/src/store/mod.rs`(新規モジュール宣言)

**Interfaces:**
- Consumes: なし(新規ファイルのみで完結する第一歩)
- Produces:
  - `pub(crate) struct MySqlConnectParams { pub host: String, pub port: u16, pub database: String, pub user: String, pub password: String }`
  - `pub(crate) struct MySqlBackend { pool: sqlx::MySqlPool }`
  - `impl MySqlBackend { pub(crate) async fn connect(params: &MySqlConnectParams) -> crate::error::Result<Self> }`
  - `pub(crate) async fn ensure_schema(pool: &sqlx::MySqlPool) -> crate::error::Result<()>`

- [ ] **Step 1: 依存クレートを追加する**

```bash
cd src-tauri
cargo add sqlx --no-default-features --features mysql,runtime-tokio,tls-rustls
cargo add sea-query@0.32.7 --no-default-features --features backend-postgres,backend-mysql,derive
cargo add testcontainers-modules --dev --features mysql
```

`Cargo.toml`を開き、`sqlx`の行が既存のPostgres用エントリと衝突せず(同じ`sqlx`依存に`postgres`と`mysql`両方のfeatureが並ぶ形になるはず)、`chrono`/`json`featureが含まれていないことを確認する。`sea-query`の行が`backend-postgres`と`backend-mysql`の両方を持つ1行になっていることを確認する(2つの`sea-query`エントリにならないこと)。

- [ ] **Step 2: ビルドが壊れていないことを確認する**

Run: `cargo check --lib` (from `src-tauri/`)
Expected: 成功する。`libsqlite3-sys`の`links`競合エラーが出たら、Step 1で入れたfeatureが`chrono`/`json`を含んでいないか確認すること。

- [ ] **Step 3: `store/mod.rs`にモジュール宣言を追加する**

`src-tauri/src/store/mod.rs`を開き、`pub(crate) mod postgres_user_ref;`の直後に追加する:

```rust
pub(crate) mod mysql_backend;
```

- [ ] **Step 4: `MySqlBackend`とDDLを書く**

`src-tauri/src/store/mysql_backend.rs`を新規作成する:

```rust
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

/// キャッシュDBのテーブルをすべて作成する(`CREATE TABLE IF NOT EXISTS`相当、冪等)。
pub(crate) async fn ensure_schema(pool: &sqlx::MySqlPool) -> Result<()> {
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
    pool.execute(idx_note_created.as_str()).await?;

    let idx_note_user = Index::create()
        .if_not_exists()
        .name("idx_note_user")
        .table(NoteTable::Table)
        .col(NoteTable::UserId)
        .build(MysqlQueryBuilder);
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
    pool.execute(idx_nr_note.as_str()).await?;
    let idx_nt_note = Index::create().if_not_exists().name("idx_nt_note").table(NoteTagTable::Table).col(NoteTagTable::NoteId).build(MysqlQueryBuilder);
    pool.execute(idx_nt_note.as_str()).await?;
    let idx_nm_note = Index::create().if_not_exists().name("idx_nm_note").table(NoteMentionTable::Table).col(NoteMentionTable::NoteId).build(MysqlQueryBuilder);
    pool.execute(idx_nm_note.as_str()).await?;
    let idx_ne_note = Index::create().if_not_exists().name("idx_ne_note").table(NoteEmojiTable::Table).col(NoteEmojiTable::NoteId).build(MysqlQueryBuilder);
    pool.execute(idx_ne_note.as_str()).await?;
    let idx_nf_note = Index::create().if_not_exists().name("idx_nf_note").table(NoteFileTable::Table).col(NoteFileTable::NoteId).build(MysqlQueryBuilder);
    pool.execute(idx_nf_note.as_str()).await?;

    let idx_nr_unique = Index::create().if_not_exists().unique().name("idx_nr_unique").table(NoteReactionTable::Table).col(NoteReactionTable::NoteId).col(NoteReactionTable::EmojiKey).build(MysqlQueryBuilder);
    pool.execute(idx_nr_unique.as_str()).await?;
    let idx_nt_unique = Index::create().if_not_exists().unique().name("idx_nt_unique").table(NoteTagTable::Table).col(NoteTagTable::NoteId).col(NoteTagTable::Tag).build(MysqlQueryBuilder);
    pool.execute(idx_nt_unique.as_str()).await?;
    let idx_nm_unique = Index::create().if_not_exists().unique().name("idx_nm_unique").table(NoteMentionTable::Table).col(NoteMentionTable::NoteId).col(NoteMentionTable::UserId).build(MysqlQueryBuilder);
    pool.execute(idx_nm_unique.as_str()).await?;
    let idx_ne_unique = Index::create().if_not_exists().unique().name("idx_ne_unique").table(NoteEmojiTable::Table).col(NoteEmojiTable::NoteId).col(NoteEmojiTable::Emoji).build(MysqlQueryBuilder);
    pool.execute(idx_ne_unique.as_str()).await?;
    let idx_nf_unique = Index::create().if_not_exists().unique().name("idx_nf_unique").table(NoteFileTable::Table).col(NoteFileTable::NoteId).col(NoteFileTable::MimeType).col(NoteFileTable::MimeCategory).col(NoteFileTable::IsSensitive).build(MysqlQueryBuilder);
    pool.execute(idx_nf_unique.as_str()).await?;

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
    pool.execute(idx_cn_column.as_str()).await?;

    let idx_cn_column_created = Index::create()
        .if_not_exists()
        .name("idx_cn_column_created")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .col((ColumnNoteTable::CreatedAt, IndexOrder::Desc))
        .col((ColumnNoteTable::NoteId, IndexOrder::Desc))
        .build(MysqlQueryBuilder);
    pool.execute(idx_cn_column_created.as_str()).await?;

    let column_fetch_boundary = Table::create()
        .table(ColumnFetchBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnFetchBoundaryTable::ColumnId).text().primary_key())
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
```

`note_reaction`等の側テーブルで`NoteId`/`Tag`等を`.string()`(`VARCHAR(255)`)にしているのは、MySQLではUNIQUE制約を持つ列にインデックスプレフィックス長の制約があり、`TEXT`型に直接UNIQUE制約を張れないため(`.text()`のままUNIQUE制約を張るとMySQLは`BLOB/TEXT column used in key specification without a key length`エラーを返す)。`note`/`user`テーブルの`id`(主キー)も同じ理由でエラーになる可能性がある — その場合は`.string()`に変更すること(コメントに記載済み)。

- [ ] **Step 5: DDLのテストを書く(testcontainers、`#[ignore]`)**

`mysql_backend.rs`の末尾に追加する:

```rust
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
}
```

`testcontainers-modules`の`mysql`モジュールが公開する使い捨てMySQLの既定認証情報(rootユーザー・パスワード有無・既定データベース名)は実装時にクレートのドキュメント(docs.rs/testcontainers-modules)で確認し、上記の接続文字列・`MySqlConnectParams`の値を実際の値に合わせること(推測で書かない)。

- [ ] **Step 6: テストを実行する**

Run: `cargo test --lib -- --ignored mysql_backend` (Dockerがあれば)、`cargo check --lib`(常に)
Expected: コンパイルが通り、Dockerがあれば全PASS。`TEXT`列へのUNIQUE制約でエラーが出た場合はStep 4の注記に従い該当列を`.string()`に変更してから再実行すること。

- [ ] **Step 7: 通常のビルド・既存テストが壊れていないことを最終確認する**

Run: `cargo build --lib && cargo test --lib` (from `src-tauri/`, `--ignored`は付けない)
Expected: 既存のテストがすべてPASSし、新規追加分は`#[ignore]`なのでスキップされる。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/store/mysql_backend.rs src-tauri/src/store/mod.rs
git commit -m "feat: MySqlBackendの接続とDDLを追加(Issue #115 Phase 3)"
```

---

### Task 2: `impl NoteCacheBackend for MySqlBackend`

**Files:**
- Create: `src-tauri/src/store/mysql_user_ref.rs`
- Modify: `src-tauri/src/store/mysql_backend.rs`(trait実装を追加)
- Modify: `src-tauri/src/store/mod.rs`(新規モジュール宣言)

**Interfaces:**
- Consumes: Task 1の`MySqlBackend`(`pool`フィールド)、`MySqlConnectParams`
- Consumes(既存・変更なし): `crate::store::user_ref::{collect_users, stub_user_refs, is_legacy_full_user, has_legacy_full_user, collect_user_id_refs, hydrate_user_refs}`
- Produces:
  - `impl NoteCacheBackend for MySqlBackend`(15メソッド全実装)
  - `mysql_user_ref.rs`: `pub(crate) async fn upsert_user(pool: &sqlx::MySqlPool, user: &crate::domain::User) -> Result<()>`、`pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::MySqlPool, user: &crate::domain::User) -> Result<()>`、`pub(crate) async fn fetch_users_by_ids(pool: &sqlx::MySqlPool, ids: &[String]) -> Result<std::collections::HashMap<String, crate::domain::User>>`

- [ ] **Step 1: `store/mod.rs`にモジュール宣言を追加する**

`pub(crate) mod mysql_backend;`の直後に追加:

```rust
pub(crate) mod mysql_user_ref;
```

- [ ] **Step 2: `mysql_user_ref.rs`を書く(user正規化テーブルの読み書き)**

`src-tauri/src/store/mysql_user_ref.rs`を新規作成する。ロジックは`store/postgres_user_ref.rs`と等価だが、UPSERT構文とプレースホルダをMySQL方言に変換する:

```rust
//! `user`テーブル(正規化済みユーザー情報)への読み書き(MySQL版)。
//! ロジックは`store/postgres_user_ref.rs`(Postgres版)と等価。UPSERT構文のみ異なる
//! (`ON CONFLICT ... DO UPDATE SET x = excluded.x` → `ON DUPLICATE KEY UPDATE x = VALUES(x)`)。
//! DB非依存の純粋関数(`stub_user_refs`等)は`user_ref.rs`のものをそのまま再利用する。

use crate::domain::{InstanceInfo, User};
use crate::error::Result;
use std::collections::HashMap;

pub(crate) async fn upsert_user(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = VALUES(username),
            host = VALUES(host),
            name = VALUES(name),
            avatar_url = VALUES(avatar_url),
            is_bot = VALUES(is_bot),
            is_cat = VALUES(is_cat),
            followers_count = VALUES(followers_count),
            following_count = VALUES(following_count),
            notes_count = VALUES(notes_count),
            emojis = VALUES(emojis),
            bio = COALESCE(VALUES(bio), bio),
            banner_url = COALESCE(VALUES(banner_url), banner_url),
            instance_name = COALESCE(VALUES(instance_name), instance_name),
            instance_icon_url = COALESCE(VALUES(instance_icon_url), instance_icon_url),
            instance_theme_color = COALESCE(VALUES(instance_theme_color), instance_theme_color)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&instance_name)
    .bind(&instance_icon_url)
    .bind(&instance_theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス専用のupsert(`postgres_user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細はそちらのdocコメント参照)。
/// MySQLの`ON DUPLICATE KEY UPDATE`では、挿入直前の既存行の値は列名をそのまま
/// (`col`)、新しく挿入しようとした値は`VALUES(col)`で参照する — Postgresの
/// `"user".col`(既存値)/`excluded.col`(新値)と役割の対応が逆になる点に注意。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = COALESCE(username, VALUES(username)),
            host = COALESCE(host, VALUES(host)),
            name = COALESCE(name, VALUES(name)),
            avatar_url = COALESCE(avatar_url, VALUES(avatar_url)),
            is_bot = is_bot,
            is_cat = is_cat,
            followers_count = followers_count,
            following_count = following_count,
            notes_count = notes_count,
            emojis = COALESCE(NULLIF(emojis, '{}'), VALUES(emojis)),
            bio = COALESCE(bio, VALUES(bio)),
            banner_url = COALESCE(banner_url, VALUES(banner_url)),
            instance_name = COALESCE(instance_name, VALUES(instance_name)),
            instance_icon_url = COALESCE(instance_icon_url, VALUES(instance_icon_url)),
            instance_theme_color = COALESCE(instance_theme_color, VALUES(instance_theme_color))",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&instance_name)
    .bind(&instance_icon_url)
    .bind(&instance_theme_color)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn fetch_users_by_ids(pool: &sqlx::MySqlPool, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    // MySQLのsqlxドライバは配列バインドをサポートしないため、`IN (?,?,...)`を
    // 要素数ぶん動的に組み立てる(Global Constraints参照)。
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM `user` WHERE id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, bool, i64, i64, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;

    for (id, username, host, name, avatar_url, is_bot, is_cat, followers_count, following_count, notes_count, emojis_json, bio, banner_url, instance_name, instance_icon_url, instance_theme_color) in rows {
        let emojis: HashMap<String, String> = serde_json::from_str(&emojis_json).unwrap_or_default();
        let instance = if instance_name.is_some() || instance_icon_url.is_some() || instance_theme_color.is_some() {
            Some(InstanceInfo { name: instance_name, icon_url: instance_icon_url, theme_color: instance_theme_color })
        } else {
            None
        };
        out.insert(
            id.clone(),
            User {
                id, username, host, name, avatar_url,
                is_bot, is_cat,
                followers_count: followers_count as u32,
                following_count: following_count as u32,
                notes_count: notes_count as u32,
                emojis, bio, banner_url, instance,
            },
        );
    }
    Ok(out)
}
```

`fetch_users_by_ids`の読み出し型が`(..., bool, bool, ...)`(Postgres版は`i16, i16`)になっているのは、MySQLの`BOOLEAN`列(実体は`TINYINT(1)`)を`sqlx-mysql`が素直に`bool`として読み書きできるため — Postgres版のような`!= 0`変換は不要。`crate::domain::User`の実フィールド型(`src-tauri/src/domain/user.rs`で確認済み: `followers_count`等は`u32`)に合わせ、書き込みは`as i64`、読み出しは`as u32`で変換する(Postgres版と同じ理由)。

- [ ] **Step 3: `mysql_user_ref.rs`のユニットテストを書く(testcontainers、`#[ignore]`)**

`postgres_user_ref.rs`の`#[cfg(test)] mod tests`(`upsert_user_roundtrip`/`upsert_user_preserves_bio_when_later_write_has_none`/`fetch_users_by_ids_returns_empty_map_for_empty_input`)と同じ内容を、`sqlx::mysql::MySqlPoolOptions`+`testcontainers_modules::mysql::Mysql`で書く。Task 1 Step 5の接続文字列パターンをそのまま使う:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::User;
    use testcontainers_modules::{mysql::Mysql, testcontainers::runners::AsyncRunner};

    async fn pool() -> sqlx::MySqlPool {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();
        crate::store::mysql_backend::ensure_schema(&pool).await.unwrap();
        std::mem::forget(container);
        pool
    }

    fn user(id: &str) -> User {
        User {
            id: id.into(), username: "alice".into(), host: None, name: Some("Alice".into()),
            avatar_url: None, is_bot: false, is_cat: false,
            followers_count: 5, following_count: 3, notes_count: 42,
            emojis: HashMap::new(), bio: None, banner_url: None, instance: None,
        }
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_roundtrip() {
        let pool = pool().await;
        upsert_user(&pool, &user("u1")).await.unwrap();
        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(got.get("u1").unwrap().name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_preserves_bio_when_later_write_has_none() {
        let pool = pool().await;
        let mut u = user("u1");
        u.bio = Some("hello".into());
        upsert_user(&pool, &u).await.unwrap();

        u.bio = None;
        upsert_user(&pool, &u).await.unwrap();

        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(got.get("u1").unwrap().bio.as_deref(), Some("hello"), "bioは既存値を保持すること");
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_users_by_ids_returns_empty_map_for_empty_input() {
        let pool = pool().await;
        let got = fetch_users_by_ids(&pool, &[]).await.unwrap();
        assert!(got.is_empty());
    }
}
```

- [ ] **Step 4: Dockerが使える環境でテストを実行する**

Run: `cargo test --lib -- --ignored mysql_user_ref` (Dockerがあれば)、`cargo check --lib`(常に)

- [ ] **Step 5: `MySqlBackend`に`NoteCacheBackend`を実装する(note本体+側テーブルのUPSERT)**

`mysql_backend.rs`の`impl MySqlBackend { ... }`ブロックの後に追加する:

```rust
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
    .bind(Option::<String>::None)
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
async fn delete_stale_by_key(
    tx: &mut sqlx::MySqlTransaction<'_>,
    table: &str,
    key_col: &str,
    note_id: &str,
    current_keys: &[String],
) -> Result<()> {
    if current_keys.is_empty() {
        let sql = format!("DELETE FROM {table} WHERE note_id = ?");
        sqlx::query(&sql).bind(note_id).execute(&mut **tx).await?;
        return Ok(());
    }
    let placeholders = current_keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM {table} WHERE note_id = ? AND {key_col} NOT IN ({placeholders})");
    let mut query = sqlx::query(&sql).bind(note_id);
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
```

**注意(実装者向け)**: `note_reaction`のUPSERTで対象キーが`(note_id, emoji_key)`のUNIQUE制約に依存する`ON DUPLICATE KEY UPDATE`を使っているが、`note_tag`/`note_mention`/`note_emoji`/`note_file`は`INSERT IGNORE`(制約違反時に何もしない、`DO NOTHING`相当)を使っている——`ON DUPLICATE KEY UPDATE col = col`のような無害な自己代入更新でも同じ効果は得られるが、`INSERT IGNORE`の方が意図が明確なため後者を採用している(Task 1のスパイクで両方とも動作確認済み)。

- [ ] **Step 6: `resolve_payload_rows`のMySQL版を書く(payload復元・自己修復)**

`upsert_note`関数の後に追加する。`PostgresBackend`の同名関数群と等価:

```rust
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
```

- [ ] **Step 7: `NoteCacheBackend`トレイトの残り12メソッドを実装する**

```rust
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
        sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = ?").bind(column_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let v: Option<(String,)> =
            sqlx::query_as("SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = ?")
                .bind(column_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(v.map(|(s,)| s))
    }

    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?,?)
             ON DUPLICATE KEY UPDATE oldest_fetched_id = VALUES(oldest_fetched_id)",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        // LEAST(...)はMySQLも同名関数をサポートするため変更不要(Global Constraints参照)。
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?,?)
             ON DUPLICATE KEY UPDATE
                oldest_fetched_id = LEAST(oldest_fetched_id, VALUES(oldest_fetched_id))",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        sqlx::query("DELETE FROM column_fetch_boundary").execute(&self.pool).await?;
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
```

`load_cached`/`notes_since`等の`LIMIT ?`/`>= ?`バインドで`u32`/`i32`をそのまま`.bind()`しているのは、`sqlx-mysql`が符号なし整数を含む幅広い整数型を素直にバインドできるため(Postgres版のような`as i64`変換は`LIMIT`/カウント引数に関しては不要。ただし実装時に型エラーが出た場合はPostgres版と同様`as i64`/`as i32`キャストを追加すること)。

- [ ] **Step 8: `prune`(サイズ上限による間引き含む)を実装する**

MySQLも`Vec<String>`の配列バインドが使えないため、SQLite版の`delete_matching`をベースに`IN (?,?,...)`方式で書き換える(`PostgresBackend::delete_matching_ids`と同じ設計だが配列バインドではなく手動IN-list):

```rust
/// SQLite版`delete_matching`と等価。TEMP TABLEを使わずRust側で対象IDを
/// `Vec<String>`として確定してから`IN (?,?,...)`でDELETEする
/// (プールの別コネクションに割り当てられてもTEMP TABLEが「無い」扱いになる問題を回避)。
/// `tx`は呼び出し元が開始したトランザクション。
async fn delete_matching_ids(tx: &mut sqlx::MySqlTransaction<'_>, ids: &[String]) -> Result<i64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let sql = format!("DELETE FROM note WHERE id IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let deleted = q.execute(&mut **tx).await?.rows_affected() as i64;

    let sql = format!("SELECT DISTINCT column_id FROM column_note WHERE note_id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, (String,)>(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let affected_columns: Vec<(String,)> = q.fetch_all(&mut **tx).await?;

    // MIN/MAX(note_id)による大小比較も、load_cached_beforeと同様MySQLのデフォルト
    // 照合順序に依存する(Global Constraints参照)。
    let sql = format!("SELECT column_id, MAX(note_id) FROM column_note WHERE note_id IN ({placeholders}) GROUP BY column_id");
    let mut q = sqlx::query_as::<_, (String, String)>(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let max_deleted_rows: Vec<(String, String)> = q.fetch_all(&mut **tx).await?;
    let max_deleted_by_column: std::collections::HashMap<String, String> = max_deleted_rows.into_iter().collect();

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        let sql = format!("DELETE FROM {table} WHERE note_id IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(id);
        }
        q.execute(&mut **tx).await?;
    }

    for (column_id,) in &affected_columns {
        let survivor: Option<(String,)> =
            sqlx::query_as("SELECT MIN(note_id) FROM column_note WHERE column_id = ?").bind(column_id).fetch_optional(&mut **tx).await?;
        match survivor.map(|(s,)| s) {
            Some(oldest) => {
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                sqlx::query(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = ? WHERE column_id = ? AND oldest_fetched_id < ?",
                )
                .bind(&candidate)
                .bind(column_id)
                .bind(&candidate)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = ?").bind(column_id).execute(&mut **tx).await?;
            }
        }
    }
    Ok(deleted)
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
```

- [ ] **Step 9: `search_cache`を実装する(変換なしで`SqlWhere`をそのまま使う)**

`prune_impl`の後に追加する:

```rust
async fn search_cache_impl(
    pool: &sqlx::MySqlPool,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::SqlParam;

    // SqlWhere.sql(`?`プレースホルダ、` REGEXP `)は無変換でそのまま使う
    // (Global Constraints参照、to_mysql_sqlに相当する変換関数は実装しない)。
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN `user` u ON u.id = n.user_id WHERE ({})", where_sql.sql);
    if until_id.is_some() {
        sql.push_str(" AND n.id < ?");
    }
    sql.push_str(" ORDER BY n.created_at DESC, n.id DESC LIMIT ?");

    let mut query = sqlx::query_as::<_, (String, String)>(&sql);
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
```

- [ ] **Step 10: コンパイルを確認する**

Run: `cargo check --lib` (from `src-tauri/`)
Expected: 成功する。`self_heal_node`の再帰(`Box::pin`忘れ)、`sqlx::MySqlTransaction`の二重デリファレンス(`&mut **tx`)まわりでエラーが出やすい箇所なので、Postgres版(`postgres_backend.rs`)の対応する既存コードと見比べて確認すること。

- [ ] **Step 11: 統合テストを書く(testcontainers、`PostgresBackend`のテストと対応させる)**

`mysql_backend.rs`の`#[cfg(test)] mod tests`(Task 1 Step 5で作った`tests`モジュール)に追記する。`PostgresBackend`の対応するテスト(`postgres_backend.rs`の`cache_roundtrip_preserves_note_and_order`/`upsert_note_removes_stale_reaction_after_unreact`/`prune_removes_oldest_beyond_keep_and_related_rows`/`search_cache_applies_predicate_and_until_id_boundary`/`search_cache_applies_boolean_predicates_without_type_error`/`fetch_boundary_roundtrip_and_extend_only_moves_older`)と同じ内容・同じアサーションを、`testcontainers_modules::mysql::Mysql`+Task 1 Step 5の接続文字列パターンで書く。**特に`search_cache_applies_boolean_predicates_without_type_error`相当のテストは必須**(Phase 2の最終レビューでCritical不具合が見つかった箇所と同じ種類の述語を検証するため)。`note`/`user`ヘルパー関数(`fn note(...)`)は`postgres_backend.rs`の同名関数と同じ内容でよい。

- [ ] **Step 12: Dockerが使える環境でテストを実行する**

Run: `cargo test --lib -- --ignored mysql_backend` (Dockerがあれば)
Expected: 全PASS。

- [ ] **Step 13: 既存テストが壊れていないことを確認する**

Run: `cargo test --lib` (from `src-tauri/`, `--ignored`は付けない)

- [ ] **Step 14: コミット**

```bash
git add src-tauri/src/store/mysql_backend.rs src-tauri/src/store/mysql_user_ref.rs src-tauri/src/store/mod.rs
git commit -m "feat: MySqlBackendにNoteCacheBackendを実装(Issue #115 Phase 3)"
```

---

### Task 3: 切替インフラへのMySQL追加

**Files:**
- Modify: `src-tauri/src/domain/cache_backend.rs`(`MySql` variant追加)
- Modify: `src-tauri/src/commands/cache_backend.rs`(`set_cache_backend`にMySql分岐追加)
- Modify: `src-tauri/src/lib.rs`(起動時フォールバックにMySql分岐追加)

Phase 2で構築済みの`NoteCacheStore::swap_backend`/`SettingsStore::load_cache_backend`/`save_cache_backend`/keyringパスワード関数(`session::save_cache_backend_password`等)はいずれも`CacheBackendConfig`に対して汎用的に実装されているため、**このタスクでは変更しない**。`store/settings.rs`・`store/note_cache.rs`・`session/secrets.rs`は本タスクでは触らない。

**Interfaces:**
- Consumes: Task 2の`MySqlBackend::connect(&MySqlConnectParams) -> Result<Self>`、`MySqlConnectParams`
- Consumes(既存・変更なし): `NoteCacheStore::swap_backend`、`SettingsStore::load_cache_backend`/`save_cache_backend`、`session::{save_cache_backend_password, load_cache_backend_password}`
- Produces: `CacheBackendConfig::MySql { host: String, port: u16, database: String, user: String }`(既存の`Sqlite`/`Postgres`と同じ形)

- [ ] **Step 1: `CacheBackendConfig`に`MySql` variantを追加する**

`src-tauri/src/domain/cache_backend.rs`を編集する:

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// note cacheのバックエンド選択(Issue #115 Phase 2/3)。パスワードはここに含まず、
/// OS keyringへ別途保存する(`session`モジュール参照。Postgres/MySQLで単一スロットを共用)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CacheBackendConfig {
    Sqlite,
    Postgres { host: String, port: u16, database: String, user: String },
    MySql { host: String, port: u16, database: String, user: String },
}

impl Default for CacheBackendConfig {
    fn default() -> Self {
        CacheBackendConfig::Sqlite
    }
}
```

- [ ] **Step 2: 既存テストが壊れていないことを確認する**

Run: `cargo check --lib && cargo test --lib` (from `src-tauri/`)
Expected: 成功する。`match config { ... }`で`MySql`を網羅していない箇所があれば、この時点で`non-exhaustive match`のコンパイルエラーとして検出される(次のStepで直す)。

- [ ] **Step 3: `set_cache_backend`にMySql分岐を追加する**

`src-tauri/src/commands/cache_backend.rs`の`set_cache_backend`内、`match &config { ... }`に`MySql`アームを追加する:

```rust
        CacheBackendConfig::MySql { host, port, database, user } => {
            let password = password.ok_or("password is required for MySQL backend")?;
            let params = crate::store::mysql_backend::MySqlConnectParams {
                host: host.clone(),
                port: *port,
                database: database.clone(),
                user: user.clone(),
                password: password.clone(),
            };
            let backend = crate::store::mysql_backend::MySqlBackend::connect(&params)
                .await
                .map_err(|e| format!("failed to connect to MySQL: {e}"))?;
            crate::session::save_cache_backend_password(&password).map_err(|e| e.to_string())?;
            std::sync::Arc::new(backend)
        }
```

`Postgres`アームの直後に追加すること。既存の「接続確認済みのバックエンドのみをswapする」「設定の永続化を先に行う」というコメント・ロジック(`state.settings.save_cache_backend`→`state.cache.swap_backend`の順序)は変更しない——MySql分岐もこの共通の後段ロジックに合流する。

- [ ] **Step 4: 起動時フォールバックにMySql分岐を追加する**

`src-tauri/src/lib.rs`の`run()`内、`match configured_backend { ... }`に`MySql`アームを追加する(`Postgres`アームと同じ構造):

```rust
                domain::CacheBackendConfig::MySql { host, port, database, user } => {
                    match session::load_cache_backend_password() {
                        Ok(Some(password)) => {
                            let params = store::mysql_backend::MySqlConnectParams {
                                host, port, database, user, password,
                            };
                            match tauri::async_runtime::block_on(store::mysql_backend::MySqlBackend::connect(&params)) {
                                Ok(backend) => std::sync::Arc::new(backend),
                                Err(e) => {
                                    log::error!(
                                        "failed to connect to configured MySQL cache backend at startup, \
                                         falling back to SQLite cache: {e}"
                                    );
                                    std::sync::Arc::new(store::SqliteBackend::new(cache_conn))
                                }
                            }
                        }
                        _ => {
                            log::error!(
                                "MySQL cache backend configured but no password found in keyring, \
                                 falling back to SQLite cache"
                            );
                            std::sync::Arc::new(store::SqliteBackend::new(cache_conn))
                        }
                    }
                }
```

`Postgres`アームの直後に追加すること。`cache_conn`(`rusqlite::Connection`)は`Sqlite`/`Postgres`失敗時フォールバック/`MySql`失敗時フォールバックの3箇所すべてに登場するが、実行時に到達するのはそのうち1箇所だけなので(`match`の各アームは互いに排他)、Rustの借用チェッカー上も問題なくコンパイルできる(Phase 2で同じパターンを確認済み)。

- [ ] **Step 5: `cargo check --lib && cargo build --lib && cargo test --lib`を実行する**

Run: `cargo check --lib && cargo build --lib && cargo test --lib` (from `src-tauri/`)
Expected: 成功する。`AppState::new_for_test`(既存のテスト用コンストラクタ)は変更していないため、既存の全テストが引き続きSQLiteのインメモリDBで動くこと。

- [ ] **Step 6: `cargo tauri dev`向けにTSバインディングを再生成する**

Run: `cargo test generates_frontend_bindings` (from `src-tauri/`)
Expected: 成功し、`frontend/src/bindings/tauri.gen.ts`の`CacheBackendConfig`型に`{ type: "mySql", host: string, port: number, database: string, user: string }`(camelCase変換後の型名は実際の出力を確認すること — `MySql`のcamelCase化が`mySql`になるか`mysql`になるかは`serde(rename_all = "camelCase")`の実際の変換規則に従うため、生成された`tauri.gen.ts`を実際に読んで確認し、Task 4のフロントエンド実装で使う正確な文字列リテラルを決めること)が追加される。

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/domain/cache_backend.rs src-tauri/src/commands/cache_backend.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: CacheBackendConfigにMySql variantを追加し切替インフラを配線(Issue #115 Phase 3)"
```

---

### Task 4: フロントエンド設定UIへのMySQL追加

**Files:**
- Modify: `frontend/src/ui/settings/CacheBackendSettings.svelte`

**Interfaces:**
- Consumes: Task 3で生成された`frontend/src/bindings/tauri.gen.ts`の`CacheBackendConfig`型(`MySql` variant追加後)、既存の`commands.getCacheBackend()`/`commands.setCacheBackend(...)`(変更なし)

- [ ] **Step 1: 実際に生成された`CacheBackendConfig`型のタグ値を確認する**

`frontend/src/bindings/tauri.gen.ts`を開き、`CacheBackendConfig`型の定義を確認する。`type: "sqlite" | "postgres" | ???`の3つ目のタグ値の実際の文字列(`mySql`/`mysql`等、`serde`の`rename_all = "camelCase"`が`MySql`をどう変換するかは実際の出力を見て確定させる。推測で書かない)を確認してから次のStepに進む。

- [ ] **Step 2: `CacheBackendSettings.svelte`にMySQLの選択肢を追加する**

`frontend/src/ui/settings/CacheBackendSettings.svelte`を編集する。既存の`mode`/`host`/`port`/`database`/`user`/`password`の状態はPostgresと共用し(フォームの見た目は同じフィールド構成のため)、`save()`内で`mode`に応じて`type`タグを出し分ける。**Step 1で確認した実際のタグ文字列**を使うこと(以下は`"mysql"`と仮定した骨子。実際のタグ値が異なる場合は全て置き換える):

```svelte
<script lang="ts">
  import { commands } from "../../bindings/tauri.gen";
  import type { CacheBackendConfig } from "../../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let mode = $state<CacheBackendConfig["type"]>("sqlite");
  let host = $state("");
  let port = $state(5432);
  let database = $state("");
  let user = $state("");
  let password = $state("");
  let loading = $state(true);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  $effect(() => {
    void commands.getCacheBackend().then((r) => {
      loading = false;
      if (r.status !== "ok") return;
      mode = r.data.type;
      if (r.data.type === "postgres" || r.data.type === "mysql") {
        host = r.data.host;
        port = r.data.port;
        database = r.data.database;
        user = r.data.user;
      }
    });
  });

  // ポート番号の既定値をモードに合わせて切り替える(PostgreSQL: 5432, MySQL: 3306)。
  // ユーザーが既に値を入力済みの場合に上書きしないよう、既定値のいずれかと一致している
  // 場合だけ切り替える(手入力した値を消さないため)。
  const DEFAULT_PORTS: Record<string, number> = { postgres: 5432, mysql: 3306 };
  function onModeChange() {
    const knownDefaults = Object.values(DEFAULT_PORTS);
    if (knownDefaults.includes(port) && mode in DEFAULT_PORTS) {
      port = DEFAULT_PORTS[mode];
    }
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      let config: CacheBackendConfig;
      if (mode === "sqlite") {
        config = { type: "sqlite" };
      } else if (mode === "postgres") {
        config = { type: "postgres", host, port, database, user };
      } else {
        config = { type: "mysql", host, port, database, user };
      }
      const r = await commands.setCacheBackend(config, mode === "sqlite" ? null : password);
      if (r.status !== "ok") {
        err = r.error;
        return;
      }
      password = "";
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-2 mt-0 text-base font-semibold">ノートキャッシュのバックエンド</h3>
<p class="mb-3.5 mt-0 text-sm text-muted-foreground">
  切り替えは即座に反映されます(再起動不要)。接続に失敗した場合、切替前のバックエンドのまま維持されます。
</p>

{#if !loading}
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input type="radio" bind:group={mode} value="sqlite" onchange={onModeChange} />
    SQLite(ローカル、既定)
  </label>
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input type="radio" bind:group={mode} value="postgres" onchange={onModeChange} />
    PostgreSQL
  </label>
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input type="radio" bind:group={mode} value="mysql" onchange={onModeChange} />
    MySQL / MariaDB
  </label>

  {#if mode === "postgres" || mode === "mysql"}
    <div class="mt-2 flex flex-col gap-2.5">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ホスト</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={host} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ポート</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="1" max="65535" bind:value={port} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">データベース名</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={database} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ユーザー名</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={user} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">パスワード</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="password" bind:value={password} />
      </label>
    </div>
    <p class="mt-2 mb-0 text-xs text-muted-foreground">
      「データ」設定の「ノートキャッシュのサイズ上限(MB)」は、この{mode === "postgres" ? "PostgreSQL" : "MySQL/MariaDB"}バックエンドには
      適用されません(バイト単位でのサイズ管理は未対応です)。保持件数上限・保持日数上限は
      引き続き有効です。
    </p>
  {/if}
{/if}

<div class="mt-3 flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">切り替えました</span>{/if}
  <Button type="button" disabled={busy || loading} onclick={save}>{busy ? "接続確認中…" : "保存して切り替え"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive" role="alert">接続に失敗しました: {err}</p>{/if}
```

**注意(実装者向け)**: `save()`内の`if (mode === "postgres") { ... } else { ... }`のような`mode === "sqlite"`以外を全部`mysql`として扱う書き方は型安全ではない(将来4つ目のバックエンドが増えると壊れる)。上記コードは`if/else if/else`の3分岐にしてTypeScriptの型絞り込みが効くようにしてある。既存の`{r.data.type === "postgres"}`のような型ガードも同様に`|| r.data.type === "mysql"`を追加する形で拡張している。実際にビルドして`pnpm check`が型エラーなく通ることを必ず確認すること(Discriminated Unionの分岐が正しく網羅されているかはTypeScriptの型チェッカーが検出してくれる)。

- [ ] **Step 3: `pnpm check`を実行する**

Run: `cd frontend && pnpm check`
Expected: 型エラーなし。

- [ ] **Step 4: 動作確認(自動テストではなく手動、任意)**

Dockerで一時MySQLコンテナ(`docker run -d -e MYSQL_ROOT_PASSWORD=root -e MYSQL_DATABASE=testdb -p 3306:3306 mysql:8.0`)を用意できる環境であれば、`cargo tauri dev`(リポジトリルートから)で設定画面からMySQLへの切替が実際に動くか確認する。確認後は自分で起動した`cargo tauri dev`・Dockerコンテナの両方を停止すること。ヘッドレス環境で`cargo tauri dev`が起動できない場合はこのStepをスキップしてよい(`pnpm check`のパスが必須条件)。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/settings/CacheBackendSettings.svelte
git commit -m "feat: 設定UIにMySQL/MariaDB選択肢を追加(Issue #115 Phase 3)"
```
