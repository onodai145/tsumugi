# PostgresBackend + 設定UI (Issue #115 Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** note cache のバックエンドとして PostgreSQL を選択できるようにする(`NoteCacheBackend` の新規実装 `PostgresBackend` を追加し、設定画面から SQLite/PostgreSQL を再起動なしに切り替えられるようにする)。

**Architecture:** `store/postgres_backend.rs`(接続・DDL・`NoteCacheBackend` trait実装)+ `store/postgres_user_ref.rs`(user正規化テーブルの読み書き、`store/user_ref.rs` の Postgres 版)を新規追加する。`NoteCacheStore`(`store/note_cache.rs`)は `Box<dyn NoteCacheBackend>` から `std::sync::Mutex<Arc<dyn NoteCacheBackend>>` へ変更し、`swap_backend` で即時切替できるようにする。設定は `SettingsData.cache_backend`(JSON)+ keyring(パスワードのみ)に保存し、フロントエンドの設定UIから変更する。

**Tech Stack:** `sqlx`(Postgresドライバ、`tokio`ランタイム)、`sea-query`(DDLのみ)+`sea-query-binder`、`testcontainers`(統合テスト用の使い捨てPostgres)、既存: `rusqlite`(SqliteBackend、変更なし)、`keyring`(パスワード保存)、`tauri-specta`(コマンド/型のTS export)。

## Global Constraints

以下は `docs/superpowers/specs/2026-09-03-external-note-cache-db-design.md` の「Phase 2設計」節で確定した値。全タスクに共通して適用される。

- 追加する依存クレートと feature は正確に以下の通り(スパイクで動作確認済み。他のバージョン・feature構成を試さないこと):
  - `sqlx = { version = "0.8.6", default-features = false, features = ["postgres", "runtime-tokio", "tls-rustls"] }`
  - `sea-query = { version = "0.32.7", default-features = false, features = ["backend-postgres", "derive"] }`(**1.0.xではなく0.32.7**。`sea-query-binder 0.7.0`が`sea-query ^0.32.0`にピンされているため)
  - `sea-query-binder = { version = "0.7.0", default-features = false, features = ["sqlx-postgres", "runtime-tokio-rustls"] }`
- **`sqlx`/`sea-query-binder`の`chrono`/`json`featureは絶対に有効化しない**(有効化すると`sqlx-sqlite`パッケージが依存グラフに要求され、既存の`rusqlite`(`libsqlite3-sys ^0.38.1`)と`links`競合を起こしビルド不能になることを実証済み)。タイムスタンプは`BIGINT`(i64のunixミリ秒/秒、既存`Note.created_at`と同じ単位)、JSON payloadは`TEXT`列とし、アプリ側で`serde_json::to_string`/`from_str`により手動シリアライズする(既存の`SqliteBackend`と同じ規約)。
- **DDL(テーブル作成)は`sea-query`の`Table::create()`で書く**。CRUD文(INSERT/SELECT/UPDATE/DELETE)は既存の`SqliteBackend`/`note_cache.rs`と同じ「手書きSQL文字列 + `sqlx::query()`のバインド」方式で書く(sea-queryのフルエントビルダーは使わない。理由: 複雑な`ON CONFLICT ... DO UPDATE SET col = COALESCE(excluded.col, "user".col)`のような式は素のSQLの方が読みやすく、既存コードの「行→構造体マッピングは手書きスタイルを維持する」という方針とも一貫するため)。プレースホルダは`$1`,`$2`,...形式(Postgresネイティブ)。
- 接続は`sqlx::postgres::PgPool`(複数コネクションのプール)を使う。**`CREATE TEMP TABLE`は使わない**(プール内の別コネクションに割り当てられると「無い」扱いになるため)。複数文にまたがる処理は`sqlx::Transaction`(1つの獲得済みコネクション上で完結)を使うか、対象IDを先に`Vec<String>`として確定してから`WHERE id = ANY($1)`でバインドする。
- `NoteCacheBackend`トレイト(`store/note_cache.rs`)のシグネチャは変更しない(Phase 1で確定済み、全15メソッド)。
- エラー型は既存の`crate::error::{Error, Result}`を使う。`sqlx::Error`は`Error::Db(format!("{e}"))`へマッピングする(既存の`rusqlite::Error`→`Error::Db`と同じ扱いを踏襲。`error.rs`に`impl From<sqlx::Error> for Error`を追加してよい)。
- 既存の`SqliteBackend`(`store/sqlite_backend.rs`)・`filter/sql.rs`・`filter/eval.rs`・`store/user_ref.rs`は本計画では**変更しない**(新規ファイルのみで完結させる。Task 3のみ既存の`note_cache.rs`/`state.rs`/`lib.rs`/`store/settings.rs`に触れる)。

---

### Task 1: 依存クレート追加 + `PostgresBackend`接続・DDLの土台

**Files:**
- Modify: `src-tauri/Cargo.toml`(依存追加)
- Create: `src-tauri/src/store/postgres_backend.rs`
- Modify: `src-tauri/src/store/mod.rs`(新規モジュール宣言。既存の`pub(crate) mod sqlite_backend;`等と同じ並びに`pub(crate) mod postgres_backend;`を追加)
- Modify: `src-tauri/src/error.rs`(`sqlx::Error`→`Error`変換を追加)
- Modify: `src-tauri/Cargo.toml`の`[dev-dependencies]`(`testcontainers`追加)

**Interfaces:**
- Consumes: なし(このタスクは新規ファイルのみで完結する第一歩)
- Produces:
  - `pub(crate) struct PostgresBackend { pool: sqlx::PgPool }`
  - `pub(crate) struct PostgresConnectParams { pub host: String, pub port: u16, pub database: String, pub user: String, pub password: String }`(接続文字列組み立て用。Task 3の`CacheBackendConfig`から変換して渡される)
  - `impl PostgresBackend { pub(crate) async fn connect(params: &PostgresConnectParams) -> crate::error::Result<Self> }` — 接続確立 + `ensure_schema`実行
  - `pub(crate) async fn ensure_schema(pool: &sqlx::PgPool) -> crate::error::Result<()>` — DDL適用(冪等、`CREATE TABLE IF NOT EXISTS`相当)
  - `Error::Db(String)`は既存のバリアント(`error.rs`で確認済み。`impl From<sqlx::Error>`もこの形にマッピングする)

- [ ] **Step 1: 依存クレートを追加する**

```bash
cd src-tauri
cargo add sqlx --no-default-features --features postgres,runtime-tokio,tls-rustls
cargo add sea-query@0.32.7 --no-default-features --features backend-postgres,derive
cargo add sea-query-binder@0.7.0 --no-default-features --features sqlx-postgres,runtime-tokio-rustls
cargo add testcontainers --dev
cargo add testcontainers-modules --dev --features postgres
```

`Cargo.toml`を開き、上記4行が`default-features = false`かつGlobal Constraintsに書いたfeatureちょうどになっていることを目視確認する(`cargo add`が余計なfeatureを足していないか)。`chrono`/`json`featureが**どこにも**含まれていないことを確認する。

- [ ] **Step 2: ビルドが壊れていないことを確認する**

Run: `cargo check --lib` (from `src-tauri/`)
Expected: 成功する(既存コードは何も参照していないので警告も出ないはず)。もし`libsqlite3-sys`の`links`競合エラーが出たら、Step 1で入れたfeatureがGlobal Constraintsと一致しているか(特に`chrono`/`json`が紛れ込んでいないか)を再確認すること。

- [ ] **Step 3: `error.rs`に`sqlx::Error`の変換を追加する**

`src-tauri/src/error.rs`を開き、既存の`impl From<rusqlite::Error> for Error`(または同等の変換)を探し、その直後に追加する:

```rust
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Db(format!("postgres error: {e}"))
    }
}
```

- [ ] **Step 4: `store/mod.rs`にモジュール宣言を追加する**

`src-tauri/src/store/mod.rs`を開き、既存の`pub(crate) mod sqlite_backend;`(または類似の宣言)の直後に追加する:

```rust
pub(crate) mod postgres_backend;
```

- [ ] **Step 5: `PostgresBackend`とDDLを書く(失敗するテストから)**

`src-tauri/src/store/postgres_backend.rs`を新規作成する:

```rust
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
```

- [ ] **Step 6: テストを実行して失敗することを確認する(Dockerが無い/未起動の場合は`#[ignore]`なので実行されない)**

Run: `cargo test --lib -- --ignored ensure_schema_is_idempotent`
Expected: Dockerが使えるマシンでは PASS。Dockerが無い環境では「テストが見つからない/実行されない」旨のメッセージになる(`#[ignore]`なので通常の`cargo test`では実行されないことも確認: `cargo test --lib postgres_backend` で何も実行されない、または通常のテストのみ実行される)。まずコンパイルが通ることが最重要(`cargo check --lib`が通ればこのステップの主目的は達成)。

- [ ] **Step 7: 通常のビルド・既存テストが壊れていないことを最終確認する**

Run: `cargo build --lib && cargo test --lib` (from `src-tauri/`, `--ignored`は付けない)
Expected: 既存のテストがすべてPASSし、新規追加分は`#[ignore]`なのでスキップされる。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/store/postgres_backend.rs src-tauri/src/store/mod.rs src-tauri/src/error.rs
git commit -m "feat: PostgresBackendの接続とDDLを追加(Issue #115 Phase 2)"
```

---

### Task 2: `impl NoteCacheBackend for PostgresBackend`

**Files:**
- Create: `src-tauri/src/store/postgres_user_ref.rs`
- Modify: `src-tauri/src/store/postgres_backend.rs`(trait実装を追加)
- Modify: `src-tauri/src/store/mod.rs`(新規モジュール宣言)
- Modify: `src-tauri/src/store/user_ref.rs`(DB非依存の純粋関数を`pub(crate)`のまま、`postgres_user_ref.rs`からも呼べることを確認するだけ。**シグネチャ変更なし**)
- Modify: `src-tauri/src/filter/sql.rs`は変更しない(Global Constraints参照)。プレースホルダ/`REGEXP`変換は`postgres_backend.rs`内に閉じる

**Interfaces:**
- Consumes: Task 1の`PostgresBackend`(`pool`フィールド)、`PostgresConnectParams`
- Consumes(既存・変更なし): `crate::store::user_ref::{collect_users, stub_user_refs, is_legacy_full_user, has_legacy_full_user, collect_user_id_refs, hydrate_user_refs}`(すべて`&Connection`を取らない純粋関数、Postgres版でもそのまま再利用する)
- Produces:
  - `impl NoteCacheBackend for PostgresBackend`(15メソッド全実装)
  - `postgres_user_ref.rs`: `pub(crate) async fn upsert_user(pool: &sqlx::PgPool, user: &crate::domain::User) -> Result<()>`、`pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::PgPool, user: &crate::domain::User) -> Result<()>`、`pub(crate) async fn fetch_users_by_ids(pool: &sqlx::PgPool, ids: &[String]) -> Result<std::collections::HashMap<String, crate::domain::User>>`
  - `postgres_backend.rs`内(非公開): `fn to_postgres_sql(where_sql: &crate::filter::sql::SqlWhere) -> (String, Vec<crate::filter::sql::SqlParam>)`

`src-tauri/src/store/user_ref.rs`の該当関数のシグネチャ(参考。変更しない):
```rust
pub(crate) fn collect_users(note: &Note) -> Vec<&User>;
pub(crate) fn stub_user_refs(note_value: &mut serde_json::Value);
pub(crate) fn is_legacy_full_user(user_value: &serde_json::Value) -> bool;
pub(crate) fn has_legacy_full_user(note_value: &serde_json::Value) -> bool;
pub(crate) fn collect_user_id_refs(note_value: &serde_json::Value, out: &mut Vec<String>);
pub(crate) fn hydrate_user_refs(note_value: &mut serde_json::Value, users: &HashMap<String, User>) -> bool;
```

- [ ] **Step 1: `store/mod.rs`にモジュール宣言を追加する**

`pub(crate) mod postgres_backend;`の直後に追加:

```rust
pub(crate) mod postgres_user_ref;
```

- [ ] **Step 2: `postgres_user_ref.rs`を書く(user正規化テーブルの読み書き)**

`src-tauri/src/store/postgres_user_ref.rs`を新規作成する。ロジックは`store/user_ref.rs`の`upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`と等価(SQLのみ`$N`プレースホルダ・`sqlx`バインドへ変換):

```rust
//! `user`テーブル(正規化済みユーザー情報)への読み書き(Postgres版)。
//! ロジックは`store/user_ref.rs`の同名関数(SQLite版)と等価。
//! DB非依存の純粋関数(`stub_user_refs`等)は`user_ref.rs`のものをそのまま再利用する。

use crate::domain::{InstanceInfo, User};
use crate::error::Result;
use std::collections::HashMap;

pub(crate) async fn upsert_user(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (id) DO UPDATE SET
            username = excluded.username,
            host = excluded.host,
            name = excluded.name,
            avatar_url = excluded.avatar_url,
            is_bot = excluded.is_bot,
            is_cat = excluded.is_cat,
            followers_count = excluded.followers_count,
            following_count = excluded.following_count,
            notes_count = excluded.notes_count,
            emojis = excluded.emojis,
            bio = COALESCE(excluded.bio, \"user\".bio),
            banner_url = COALESCE(excluded.banner_url, \"user\".banner_url),
            instance_name = COALESCE(excluded.instance_name, \"user\".instance_name),
            instance_icon_url = COALESCE(excluded.instance_icon_url, \"user\".instance_icon_url),
            instance_theme_color = COALESCE(excluded.instance_theme_color, \"user\".instance_theme_color)",
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

/// 自己修復パス専用のupsert(`user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細は`user_ref.rs`のdocコメント参照)。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (id) DO UPDATE SET
            username = COALESCE(\"user\".username, excluded.username),
            host = COALESCE(\"user\".host, excluded.host),
            name = COALESCE(\"user\".name, excluded.name),
            avatar_url = COALESCE(\"user\".avatar_url, excluded.avatar_url),
            is_bot = \"user\".is_bot,
            is_cat = \"user\".is_cat,
            followers_count = \"user\".followers_count,
            following_count = \"user\".following_count,
            notes_count = \"user\".notes_count,
            emojis = COALESCE(NULLIF(\"user\".emojis, '{}'), excluded.emojis),
            bio = COALESCE(\"user\".bio, excluded.bio),
            banner_url = COALESCE(\"user\".banner_url, excluded.banner_url),
            instance_name = COALESCE(\"user\".instance_name, excluded.instance_name),
            instance_icon_url = COALESCE(\"user\".instance_icon_url, excluded.instance_icon_url),
            instance_theme_color = COALESCE(\"user\".instance_theme_color, excluded.instance_theme_color)",
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

pub(crate) async fn fetch_users_by_ids(pool: &sqlx::PgPool, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, bool, i64, i64, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM \"user\" WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

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
                id,
                username,
                host,
                name,
                avatar_url,
                is_bot,
                is_cat,
                followers_count: followers_count as u32,
                following_count: following_count as u32,
                notes_count: notes_count as u32,
                emojis,
                bio,
                banner_url,
                instance,
            },
        );
    }
    Ok(out)
}
```

`crate::domain::User`(`src-tauri/src/domain/user.rs`)を確認済み: `followers_count`/`following_count`/`notes_count`は`u32`。Postgres/sqlxに符号なし整数のネイティブ対応が無いため、書き込みは`as i64`、読み出しは`BIGINT`列を`i64`で受けてから`as u32`で戻す(上記コードの通り)。

- [ ] **Step 3: `postgres_user_ref.rs`のユニットテストを書く(testcontainers、`#[ignore]`)**

`postgres_user_ref.rs`末尾に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::User;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    async fn pool() -> sqlx::PgPool {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();
        crate::store::postgres_backend::ensure_schema(&pool).await.unwrap();
        // testcontainersのcontainerハンドルはpoolが生きている間ドロップされると
        // コンテナが止まるため、リークさせてテストプロセス終了まで保持する。
        std::mem::forget(container);
        pool
    }

    fn user(id: &str) -> User {
        User {
            id: id.into(),
            username: "alice".into(),
            host: None,
            name: Some("Alice".into()),
            avatar_url: None,
            is_bot: false,
            is_cat: false,
            followers_count: 5,
            following_count: 3,
            notes_count: 42,
            emojis: HashMap::new(),
            bio: None,
            banner_url: None,
            instance: None,
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

        u.bio = None; // ライブ書き込み(UserLiteのみ)を模す
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

`crate::domain::User`の実フィールドと一致しない場合は実際の定義に合わせて修正すること。

- [ ] **Step 4: Dockerが使える環境でテストを実行する(使えない場合はコンパイルのみ確認)**

Run: `cargo test --lib -- --ignored postgres_user_ref` (Dockerがあれば)、`cargo check --lib`(常に)
Expected: コンパイルが通り、Dockerがあれば全PASS。

- [ ] **Step 5: `PostgresBackend`に`NoteCacheBackend`を実装する(note本体+側テーブルのUPSERT)**

`postgres_backend.rs`の`impl PostgresBackend { ... }`ブロックの後に追加する。まず`cache_notes`/`cache_note`/`update_note`(内部で共有する`upsert_note`ヘルパーを使う):

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

/// `note`行 + 側テーブル(reaction/tag/mention/emoji/file)をUPSERTする。
/// `store/note_cache.rs::upsert_note`(SQLite版)と等価。1トランザクション内で実行する。
async fn upsert_note(pool: &sqlx::PgPool, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    crate::store::user_ref::stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false);

    // note行より先にuserをupsertする(SQLite版と同じ理由: user行が無いnote行が
    // 永久に読めなくなる事態を避ける)。
    for user in crate::store::user_ref::collect_users(n) {
        crate::store::postgres_user_ref::upsert_user(pool, user).await?;
    }

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO note (
            id, created_at, text, text_length, cw, visibility, local_only, user_id,
            reply_id, reply_user_id, renote_id, channel_id, via, lang,
            files_count, has_poll, has_link, is_pinned,
            reaction_count, renote_count, reply_count, my_reaction,
            is_renoted_by_me, is_favorited_by_me, payload
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)
        ON CONFLICT (id) DO UPDATE SET
            created_at = excluded.created_at, text = excluded.text, text_length = excluded.text_length,
            cw = excluded.cw, visibility = excluded.visibility, local_only = excluded.local_only,
            user_id = excluded.user_id, reply_id = excluded.reply_id, reply_user_id = excluded.reply_user_id,
            renote_id = excluded.renote_id, channel_id = excluded.channel_id, via = excluded.via,
            lang = excluded.lang, files_count = excluded.files_count, has_poll = excluded.has_poll,
            has_link = excluded.has_link, is_pinned = excluded.is_pinned,
            reaction_count = excluded.reaction_count, renote_count = excluded.renote_count,
            reply_count = excluded.reply_count, my_reaction = excluded.my_reaction,
            is_renoted_by_me = excluded.is_renoted_by_me, is_favorited_by_me = excluded.is_favorited_by_me,
            payload = excluded.payload",
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
    .execute(&mut *tx)
    .await?;

    for (emoji, count) in &n.reactions {
        sqlx::query(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ($1,$2,$3)
             ON CONFLICT (note_id, emoji_key) DO UPDATE SET count = excluded.count",
        )
        .bind(&n.id)
        .bind(emoji)
        .bind(*count as i64)
        .execute(&mut *tx)
        .await?;
    }
    for tag in &n.tags {
        sqlx::query(
            "INSERT INTO note_tag (note_id, tag) VALUES ($1,$2) ON CONFLICT (note_id, tag) DO NOTHING",
        )
        .bind(&n.id)
        .bind(tag)
        .execute(&mut *tx)
        .await?;
    }
    for uid in &n.mentions {
        sqlx::query(
            "INSERT INTO note_mention (note_id, user_id) VALUES ($1,$2) ON CONFLICT (note_id, user_id) DO NOTHING",
        )
        .bind(&n.id)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    }
    for e in n.emojis.keys() {
        sqlx::query(
            "INSERT INTO note_emoji (note_id, emoji) VALUES ($1,$2) ON CONFLICT (note_id, emoji) DO NOTHING",
        )
        .bind(&n.id)
        .bind(e)
        .execute(&mut *tx)
        .await?;
    }
    for f in &n.files {
        sqlx::query(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES ($1,$2,$3,$4)
             ON CONFLICT (note_id, mime_type, mime_category, is_sensitive) DO NOTHING",
        )
        .bind(&n.id)
        .bind(&f.mime_type)
        .bind(mime_category(&f.mime_type))
        .bind(f.is_sensitive)
        .execute(&mut *tx)
        .await?;
    }

    // 旧行の掃除(SQLiteのjson_eachの代わりにPostgresは`= ANY($N)`配列バインドを使う)。
    let reaction_keys: Vec<&String> = n.reactions.keys().collect();
    sqlx::query("DELETE FROM note_reaction WHERE note_id = $1 AND NOT (emoji_key = ANY($2))")
        .bind(&n.id)
        .bind(&reaction_keys)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM note_tag WHERE note_id = $1 AND NOT (tag = ANY($2))")
        .bind(&n.id)
        .bind(&n.tags)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM note_mention WHERE note_id = $1 AND NOT (user_id = ANY($2))")
        .bind(&n.id)
        .bind(&n.mentions)
        .execute(&mut *tx)
        .await?;
    let emoji_keys: Vec<&String> = n.emojis.keys().collect();
    sqlx::query("DELETE FROM note_emoji WHERE note_id = $1 AND NOT (emoji = ANY($2))")
        .bind(&n.id)
        .bind(&emoji_keys)
        .execute(&mut *tx)
        .await?;
    // note_fileは複合キーで`= ANY`が使えないため、SQLite版(rowidベース)と同様に
    // 既存行を取得してRust側で差分判定し、現存しないキーの組だけ個別にDELETEする。
    let current_file_keys: std::collections::HashSet<(String, String, bool)> = n
        .files
        .iter()
        .map(|f| (f.mime_type.clone(), mime_category(&f.mime_type).to_string(), f.is_sensitive))
        .collect();
    let existing_files: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = $1",
    )
    .bind(&n.id)
    .fetch_all(&mut *tx)
    .await?;
    for (mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = (mime_type.clone(), mime_category_val.clone(), is_sensitive);
        if !current_file_keys.contains(&key) {
            sqlx::query(
                "DELETE FROM note_file WHERE note_id = $1 AND mime_type = $2 AND mime_category = $3 AND is_sensitive = $4",
            )
            .bind(&n.id)
            .bind(&mime_type)
            .bind(&mime_category_val)
            .bind(is_sensitive)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 6: `resolve_payload_rows`のPostgres版を書く(payload復元・自己修復)**

`upsert_note`関数の後に追加する。`note_cache.rs::resolve_payload_rows`/`self_heal_legacy_row`/`self_heal_node`と等価:

```rust
async fn self_heal_node(pool: &sqlx::PgPool, node: &mut serde_json::Value) -> Result<bool> {
    let mut changed = false;
    if let Some(user_value) = node.get("user").cloned() {
        if crate::store::user_ref::is_legacy_full_user(&user_value) {
            if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                crate::store::postgres_user_ref::fill_user_from_snapshot(pool, &user).await?;
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

async fn self_heal_legacy_row(pool: &sqlx::PgPool, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(pool, value).await?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        sqlx::query("UPDATE note SET payload = $1 WHERE id = $2").bind(new_payload).bind(note_id).execute(pool).await?;
    }
    Ok(())
}

async fn resolve_payload_rows(pool: &sqlx::PgPool, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
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
    let users = crate::store::postgres_user_ref::fetch_users_by_ids(pool, &ids).await?;

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

(`self_heal_node`は再帰`async fn`のため`Box::pin(...)`で包む必要がある。素直に`async fn`を再帰させるとコンパイルエラーになる点に注意。)

- [ ] **Step 7: `NoteCacheBackend`トレイトの残り12メソッドを実装する**

`resolve_payload_rows`の後、`impl NoteCacheBackend for PostgresBackend`ブロックを追加する:

```rust
#[async_trait::async_trait]
impl NoteCacheBackend for PostgresBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let now = crate::store::note_cache::now_epoch();
        for n in notes {
            upsert_note(&self.pool, n).await?;
            sqlx::query(
                "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ($1,$2,$3,$4)
                 ON CONFLICT DO NOTHING",
            )
            .bind(column_id)
            .bind(&n.id)
            .bind(now)
            .bind(n.created_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = $1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT $2",
        )
        .bind(column_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        resolve_payload_rows(&self.pool, rows).await
    }

    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = $1 AND cn.note_id < $2
             ORDER BY cn.note_id DESC
             LIMIT $3",
        )
        .bind(column_id)
        .bind(until_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        let mut out = resolve_payload_rows(&self.pool, rows).await?;
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }

    async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, payload FROM note WHERE id = $1").bind(note_id).fetch_optional(&self.pool).await?;
        Ok(match row {
            Some(r) => resolve_payload_rows(&self.pool, vec![r]).await?.into_iter().next(),
            None => None,
        })
    }

    async fn update_note(&self, note: &Note) -> Result<()> {
        upsert_note(&self.pool, note).await
    }

    async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM column_note WHERE column_id = $1").bind(column_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = $1").bind(column_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let v: Option<(String,)> = sqlx::query_as(
            "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = $1",
        )
        .bind(column_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(v.map(|(s,)| s))
    }

    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES ($1,$2)
             ON CONFLICT (column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES ($1,$2)
             ON CONFLICT (column_id) DO UPDATE SET
                oldest_fetched_id = LEAST(column_fetch_boundary.oldest_fetched_id, excluded.oldest_fetched_id)",
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
            sqlx::query_as("SELECT COUNT(*) FROM note WHERE created_at >= $1").bind(since_epoch_secs as i64).fetch_one(&self.pool).await?;
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

`extend_fetch_boundary`はSQLiteの`MIN(a,b)`(2引数集約関数呼び出し可能な組み込み)をPostgresの`LEAST(a,b)`へ変えている点に注意(挙動は同じ「より古い方を残す」)。`cache_notes`のON CONFLICT句はSQLiteの`INSERT OR IGNORE`と等価にするため列指定なしの`ON CONFLICT DO NOTHING`にしている(`column_note`の主キーは`(column_id, note_id)`の複合キーなので、素の`ON CONFLICT DO NOTHING`で複合主キー全体をターゲットにできる)。

- [ ] **Step 8: `prune`(サイズ上限による間引き含む)を実装する**

SQLiteの`delete_matching`(TEMP TABLE使用)とincremental vacuum相当は使えないため、**対象IDを`Vec<String>`へ確定してから`= ANY($N)`でDELETEする方式**へ書き換える(Global Constraints参照)。`prune_impl`関数を追加する:

```rust
/// SQLite版`delete_matching`と等価だが、TEMP TABLEを使わずRust側で対象IDを
/// `Vec<String>`として確定してから`= ANY($1)`でDELETEする(プールの別コネクションに
/// 割り当てられてもTEMP TABLEが「無い」扱いになる問題を回避するため。Global Constraints参照)。
/// `tx`は呼び出し元が開始したトランザクション。
async fn delete_matching_ids(tx: &mut sqlx::PgTransaction<'_>, ids: &[String]) -> Result<i64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let deleted = sqlx::query("DELETE FROM note WHERE id = ANY($1)").bind(ids).execute(&mut **tx).await?.rows_affected() as i64;

    let affected_columns: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT column_id FROM column_note WHERE note_id = ANY($1)").bind(ids).fetch_all(&mut **tx).await?;
    let max_deleted_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_id, MAX(note_id) FROM column_note WHERE note_id = ANY($1) GROUP BY column_id",
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    let max_deleted_by_column: std::collections::HashMap<String, String> = max_deleted_rows.into_iter().collect();

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE note_id = ANY($1)")).bind(ids).execute(&mut **tx).await?;
    }

    for (column_id,) in &affected_columns {
        let survivor: Option<(String,)> =
            sqlx::query_as("SELECT MIN(note_id) FROM column_note WHERE column_id = $1").bind(column_id).fetch_optional(&mut **tx).await?;
        match survivor.map(|(s,)| s) {
            Some(oldest) => {
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                sqlx::query(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = $2 WHERE column_id = $1 AND oldest_fetched_id < $2",
                )
                .bind(column_id)
                .bind(&candidate)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = $1").bind(column_id).execute(&mut **tx).await?;
            }
        }
    }
    Ok(deleted)
}

async fn prune_impl(pool: &sqlx::PgPool, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
    let mut deleted: i64 = 0;
    let mut tx = pool.begin().await?;

    if max_age_days > 0 {
        let cutoff = crate::store::note_cache::now_epoch() - max_age_days as i64 * 86_400;
        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM note WHERE created_at < $1").bind(cutoff).fetch_all(&mut *tx).await?;
        let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
        deleted += delete_matching_ids(&mut tx, &ids).await?;
    }
    if keep > 0 {
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note").fetch_one(&mut *tx).await?;
        let overflow = total - keep as i64;
        if overflow > 0 {
            let ids: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT $1").bind(overflow).fetch_all(&mut *tx).await?;
            let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
            deleted += delete_matching_ids(&mut tx, &ids).await?;
        }
    }
    tx.commit().await?;

    if max_size_mb > 0 {
        // Postgresは対象DB全体のサイズ(`pg_database_size`)しか手軽に取れず、SQLiteのように
        // 単一ファイルの物理サイズやincremental vacuumで縮小、という概念がない(通常のテーブルは
        // 他のDBオブジェクトと同じデータベースに同居しうる。同一データベースを他用途と共有しない
        // 運用を前提とする)。ここでは「サイズが予算を超えている間、最古のノートを一定数ずつ追加削除する」
        // 素朴なループにする(SQLiteの`shrink_to_size`と挙動を完全一致させることは狙わない)。
        let budget_bytes = max_size_mb as i64 * 1024 * 1024;
        loop {
            let (size,): (i64,) = sqlx::query_as("SELECT pg_database_size(current_database())").fetch_one(pool).await?;
            if size <= budget_bytes {
                break;
            }
            let ids: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT 100").fetch_all(pool).await?;
            if ids.is_empty() {
                break;
            }
            let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
            let mut tx = pool.begin().await?;
            let n = delete_matching_ids(&mut tx, &ids).await?;
            tx.commit().await?;
            deleted += n;
        }
    }
    Ok(deleted as usize)
}
```

- [ ] **Step 9: `search_cache`(`to_postgres_sql`によるプレースホルダ/`REGEXP`変換)を実装する**

`prune_impl`の後に追加する:

```rust
/// `SqlWhere.sql`(`?`プレースホルダ、SQLite方言)をPostgres用に変換する。
/// `?`を出現順に`$1`,`$2`,...へ振り直し、` REGEXP `を` ~ `へ置換する。
/// 設計書「filter/sql.rs(TQLの扱い)」参照(build_where自体は変更しない)。
fn to_postgres_sql(where_sql: &crate::filter::sql::SqlWhere) -> String {
    let mut out = String::with_capacity(where_sql.sql.len());
    let mut n = 0usize;
    for ch in where_sql.sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out.replace(" REGEXP ", " ~ ")
}

async fn search_cache_impl(
    pool: &sqlx::PgPool,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::SqlParam;

    let converted_where = to_postgres_sql(where_sql);
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN \"user\" u ON u.id = n.user_id WHERE ({converted_where})");
    let mut next_placeholder = where_sql.params.len() + 1;
    if until_id.is_some() {
        sql.push_str(&format!(" AND n.id < ${next_placeholder}"));
        next_placeholder += 1;
    }
    sql.push_str(&format!(" ORDER BY n.created_at DESC, n.id DESC LIMIT ${next_placeholder}"));

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
    query = query.bind(limit as i64);

    let rows = query.fetch_all(pool).await?;
    resolve_payload_rows(pool, rows).await
}
```

- [ ] **Step 10: コンパイルを確認する**

Run: `cargo check --lib` (from `src-tauri/`)
Expected: 成功する。`async fn`の再帰(`self_heal_node`)を`Box::pin`で包み忘れているとここでエラーになるので、その場合はStep 7のコードを見直す。

- [ ] **Step 11: 統合テストを書く(testcontainers、`SqliteBackend`のテストと対応させる)**

`postgres_backend.rs`の`#[cfg(test)] mod tests`に追加する(Step 5で作った`tests`モジュールへ追記):

```rust
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
                emojis: std::collections::HashMap::new(), bio: None, banner_url: None, instance: None,
            },
            reply_id: None, renote_id: None, renote: None,
            files: vec![DriveFile { id: "f1".into(), mime_type: "image/png".into(), is_sensitive: false, url: "http://x/f1".into(), thumbnail_url: None, name: "f1.png".into() }],
            poll: None, tags: vec!["rust".into()], mentions: vec![],
            emojis: std::collections::HashMap::new(), channel_id: None, via: None, lang: None,
            reactions: std::collections::HashMap::from([("👍".into(), 3u32)]),
            reaction_count: 3, renote_count: 1, reply_count: 0,
            my_reaction: Some("👍".into()), is_renoted_by_me: false, is_favorited_by_me: false, is_pinned: false,
        }
    }

    async fn backend() -> PostgresBackend {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let params = PostgresConnectParams { host: "127.0.0.1".into(), port, database: "postgres".into(), user: "postgres".into(), password: "postgres".into() };
        let backend = PostgresBackend::connect(&params).await.unwrap();
        std::mem::forget(container);
        backend
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

    #[tokio::test]
    #[ignore]
    async fn search_cache_translates_regexp_to_postgres_tilde_operator() {
        use crate::filter::{parser, sql};
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("text match \"rust\"").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.len(), 1);
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_boundary_roundtrip_and_extend_only_moves_older() {
        let s = backend().await;
        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
        s.set_fetch_boundary("col1", "n500").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n500"));
        s.extend_fetch_boundary("col1", "n300").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
        s.extend_fetch_boundary("col1", "n800").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
    }
```

- [ ] **Step 12: Dockerが使える環境でテストを実行する**

Run: `cargo test --lib -- --ignored postgres_backend` (Dockerがあれば)
Expected: 全PASS。Dockerが無ければ`cargo check --lib`と`cargo build --lib`が通ることを確認する。

- [ ] **Step 13: 既存テストが壊れていないことを確認する**

Run: `cargo test --lib` (from `src-tauri/`, `--ignored`は付けない)
Expected: 既存のテストが全てPASS。

- [ ] **Step 14: コミット**

```bash
git add src-tauri/src/store/postgres_backend.rs src-tauri/src/store/postgres_user_ref.rs src-tauri/src/store/mod.rs
git commit -m "feat: PostgresBackendにNoteCacheBackendを実装(Issue #115 Phase 2)"
```

---

### Task 3: バックエンド切替インフラ(`NoteCacheStore`のMutex化・設定・keyring・コマンド)

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`(`NoteCacheStore`を`Mutex<Arc<dyn NoteCacheBackend>>`化、`swap_backend`追加)
- Create: `src-tauri/src/domain/cache_backend.rs`(`CacheBackendConfig`型定義。既存の`domain/mute.rs`/`domain/notify.rs`と同じ配置パターン)
- Modify: `src-tauri/src/domain/mod.rs`(`mod cache_backend;` + `pub use cache_backend::CacheBackendConfig;`を追加)
- Modify: `src-tauri/src/store/settings.rs`(`SettingsData`(非公開構造体)に`cache_backend`フィールド追加、`load_cache_backend`/`save_cache_backend`を`SettingsStore`に追加。既存の`load_mute`/`save_mute`と同じパターン)
- Create: `src-tauri/src/commands/cache_backend.rs`(切替用コマンド)
- Modify: `src-tauri/src/lib.rs`(`specta_builder()`へコマンド登録、コマンドモジュール宣言、**起動時のバックエンド構築ロジックもここ**(`run()`内、`let cache_conn = db::open_cache(...)`の周辺、`state.rs`ではない)
- Modify: `src-tauri/src/session/`配下(keyringアクセスの既存パターンを確認し、パスワード保存/読み出し関数を追加。既存のaccountトークン保存関数の実装を読んでから同じ`keyring::Entry`の使い方に合わせること)

**Interfaces:**
- Consumes: Task 2の`PostgresBackend::connect(&PostgresConnectParams) -> Result<Self>`、`PostgresConnectParams`
- Consumes(既存・変更なし): `NoteCacheBackend`トレイト、`SqliteBackend::new(Connection) -> Self`、`AppState { pub settings: SettingsStore, pub cache: NoteCacheStore, ... }`(`state.rs`で確認済み。`AppState::new(secrets, settings, drafts, cache)`は`lib.rs`の`run()`内で呼ばれ、`cache`は事前に`NoteCacheStore::new(SqliteBackend::new(cache_conn))`として構築されてから渡される。**`state.rs`自体はバックエンド構築ロジックを持たない**)
- Produces:
  - `NoteCacheStore::new(backend: impl NoteCacheBackend + 'static) -> Self`(シグネチャ不変、内部実装のみ変更)
  - `NoteCacheStore::new_from_arc(backend: Arc<dyn NoteCacheBackend>) -> Self`(新規)
  - `NoteCacheStore::swap_backend(&self, new_backend: Arc<dyn NoteCacheBackend>)`(新規、`Result`を返さない同期メソッド)
  - `domain/cache_backend.rs`: `#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)] #[serde(tag = "type", rename_all = "camelCase")] pub enum CacheBackendConfig { Sqlite, Postgres { host: String, port: u16, database: String, user: String } }`(パスワードは含まない。`Default`は手動`impl`で`Sqlite`を返す)
  - `SettingsStore::load_cache_backend(&self) -> Result<CacheBackendConfig>`、`SettingsStore::save_cache_backend(&self, cfg: &CacheBackendConfig) -> Result<()>`(`store/settings.rs`、`load_mute`/`save_mute`と同じパターン)
  - `#[tauri::command] #[specta::specta] pub async fn set_cache_backend(state: State<'_, AppState>, config: CacheBackendConfig, password: Option<String>) -> Result<(), String>`(`commands/cache_backend.rs`)
  - `#[tauri::command] #[specta::specta] pub async fn get_cache_backend(state: State<'_, AppState>) -> Result<CacheBackendConfig, String>`

- [ ] **Step 1: `NoteCacheStore`を`Mutex<Arc<dyn NoteCacheBackend>>`化する**

`src-tauri/src/store/note_cache.rs`の`NoteCacheStore`定義を以下に置き換える(スパイクで検証済みのパターン。設計書「バックエンド抽象化」節参照):

```rust
pub struct NoteCacheStore {
    backend: std::sync::Mutex<std::sync::Arc<dyn NoteCacheBackend>>,
}

impl NoteCacheStore {
    pub fn new(backend: impl NoteCacheBackend + 'static) -> Self {
        Self { backend: std::sync::Mutex::new(std::sync::Arc::new(backend)) }
    }

    /// バックエンドを即時差し替える。ロックは一瞬だけ保持し(`.await`をまたがない)、
    /// 差し替え中に進行中の呼び出しは古いバックエンド(cloneされた`Arc`)のまま
    /// 完走する(設計書「バックエンド抽象化」節参照)。
    pub fn swap_backend(&self, new_backend: std::sync::Arc<dyn NoteCacheBackend>) {
        *self.backend.lock().unwrap() = new_backend;
    }

    fn backend(&self) -> std::sync::Arc<dyn NoteCacheBackend> {
        std::sync::Arc::clone(&self.backend.lock().unwrap())
    }
}
```

既存の各委譲メソッド(`cache_notes`/`load_cached`/...)本体は変更しない**が**、`self.backend.method(...)`となっている箇所を`self.backend().method(...)`(メソッド呼び出しに変更)へ書き換える必要がある。例えば:

```rust
pub async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
    self.backend().cache_notes(column_id, notes).await
}
```

`note_cache.rs`内の全既存委譲メソッド(`cache_notes`/`cache_note`/`load_cached`/`load_cached_before`/`get_note`/`update_note`/`clear_column_notes`/`get_fetch_boundary`/`set_fetch_boundary`/`extend_fetch_boundary`/`clear_all_fetch_boundaries`/`note_count`/`notes_since`/`prune`/`search_cache`)を同じパターンで書き換える(`self.backend.` → `self.backend().`)。

- [ ] **Step 2: 既存テストが壊れていないことを確認する**

Run: `cargo test --lib` (from `src-tauri/`)
Expected: `NoteCacheStore`を経由する既存テスト(`note_cache.rs`・`sqlite_backend.rs`両方)が全てPASSする(`NoteCacheBackend`トレイトオブジェクトの型が`Box`→`Arc`に変わるだけで、公開APIのシグネチャは不変のため既存テストは無変更で通るはず)。

- [ ] **Step 3: `NoteCacheStore`単体の切替テストを追加する**

`note_cache.rs`の`#[cfg(test)] mod tests`(既存にあるはず。無ければ新規追加)に追加する:

```rust
    #[tokio::test]
    async fn swap_backend_switches_active_backend_for_subsequent_calls() {
        let backend_a = crate::store::sqlite_backend::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        );
        let store = NoteCacheStore::new(backend_a);
        store.cache_note("col1", &test_note("n1")).await.unwrap();
        assert_eq!(store.load_cached("col1", 10).await.unwrap().len(), 1);

        let backend_b = crate::store::sqlite_backend::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        );
        store.swap_backend(std::sync::Arc::new(backend_b));

        // 切替後は新しい(空の)バックエンドを見ている
        assert_eq!(store.load_cached("col1", 10).await.unwrap().len(), 0);
    }
```

このテストが使う`test_note`ヘルパーが`note_cache.rs`の既存テストモジュールに無ければ、`sqlite_backend.rs`の`note(id, created_at)`ヘルパーと同じ内容で追加すること(最小構成のNoteを1件作れればよい)。

- [ ] **Step 4: テストを実行して通ることを確認する**

Run: `cargo test --lib note_cache -- --nocapture`
Expected: 新規テストを含め全てPASS。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/note_cache.rs
git commit -m "refactor: NoteCacheStoreをMutex<Arc<dyn NoteCacheBackend>>化しswap_backendを追加(Issue #115 Phase 2)"
```

- [ ] **Step 6: `SettingsData`に`cache_backend`フィールドを追加する**

`src-tauri/src/store/settings.rs`の既存の`SettingsData`(`accounts`/`groups`/`columns`/`mute`/`notify`/`ui`/`pane_layout`が並ぶ、非公開の`struct SettingsData`)・`SettingsStore`の`load_mute`/`save_mute`(NG設定)を確認する。同じパターンで追加する。

まず`src-tauri/src/domain/cache_backend.rs`を新規作成する(`domain/mute.rs`と同じ配置・derive方針):

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// note cacheのバックエンド選択(Issue #115 Phase 2)。パスワードはここに含まず、
/// OS keyringへ別途保存する(`session`モジュール参照)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CacheBackendConfig {
    Sqlite,
    Postgres { host: String, port: u16, database: String, user: String },
}

impl Default for CacheBackendConfig {
    fn default() -> Self {
        CacheBackendConfig::Sqlite
    }
}
```

`src-tauri/src/domain/mod.rs`に追加する(`mod mute;`/`pub use mute::MuteConfig;`と同じ並びに):

```rust
mod cache_backend;
```
```rust
pub use cache_backend::CacheBackendConfig;
```

`src-tauri/src/store/settings.rs`の`use crate::domain::{...}`に`CacheBackendConfig`を追加し、`SettingsData`構造体に以下のフィールドを追加する(`mute`/`notify`と同じ並び):

```rust
    #[serde(default)]
    cache_backend: CacheBackendConfig,
```

`load_mute`/`save_mute`の直後に、同じパターンで追加する:

```rust
    // ---- note cacheバックエンド設定(Issue #115 Phase 2) ----

    pub fn load_cache_backend(&self) -> Result<CacheBackendConfig> {
        Ok(self.data.lock().unwrap().cache_backend.clone())
    }

    pub fn save_cache_backend(&self, cfg: &CacheBackendConfig) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.cache_backend = cfg.clone();
        self.save(&guard)
    }
```

- [ ] **Step 7: 設定の読み書きテストを追加する**

`settings.rs`の既存`#[cfg(test)] mod tests`に、`load_mute`/`save_mute`の既存テストと同じスタイルで追加する:

```rust
    #[test]
    fn cache_backend_defaults_to_sqlite_and_roundtrips_postgres() {
        let store = SettingsStore::new_in_memory();
        assert_eq!(store.load_cache_backend().unwrap(), CacheBackendConfig::Sqlite);

        let cfg = CacheBackendConfig::Postgres {
            host: "db.example".into(), port: 5432, database: "tsumugi".into(), user: "app".into(),
        };
        store.save_cache_backend(&cfg).unwrap();
        assert_eq!(store.load_cache_backend().unwrap(), cfg);
    }
```

`SettingsStore::new_in_memory()`は既存の他のテスト(`load_mute`のテスト等)が使っているコンストラクタをそのまま使う(既存のテストヘルパー名と完全に一致させること。もし別名なら合わせる)。

- [ ] **Step 8: テストを実行する**

Run: `cargo test --lib settings`
Expected: 全PASS。

- [ ] **Step 9: コミット**

```bash
git add src-tauri/src/domain/cache_backend.rs src-tauri/src/domain/mod.rs src-tauri/src/store/settings.rs
git commit -m "feat: SettingsStoreにcache_backend設定を追加(Issue #115 Phase 2)"
```

- [ ] **Step 10: keyringへのパスワード保存/読み出し関数を追加する**

`src-tauri/src/session/`配下で既存のaccountトークンがどう`keyring::Entry`に保存されているか(サービス名・ユーザー名の組み立て方、`set_password`/`get_password`/`delete_password`の呼び方)を確認し、同じファイル(または同じパターンの新規関数として`session/`配下の適切な既存ファイル)に以下を追加する。**関数名・配置場所はaccountトークン保存の既存実装を読んでから、そのモジュールの慣習に合わせること**(以下は最低限満たすべきインターフェース):

```rust
// service名は既存のaccountトークン保存で使っているものとは別の固定文字列にする
// (例: 既存が "tsumugi" ならこちらは "tsumugi-cache-backend" のように衝突しない名前にする)
pub(crate) fn save_cache_backend_password(password: &str) -> crate::error::Result<()>;
pub(crate) fn load_cache_backend_password() -> crate::error::Result<Option<String>>;
pub(crate) fn delete_cache_backend_password() -> crate::error::Result<()>;
```

実装は既存のaccountトークン保存関数(`keyring::Entry::new(service, user)` → `.set_password(...)`/`.get_password()`/`.delete_credential()`)とほぼ同じ形になるはず。既存関数の`Result`変換パターン(`keyring::Error`をどう`crate::error::Error`へマッピングしているか)も踏襲する。

- [ ] **Step 11: keyring読み書きのテストを追加する(既存のaccountトークンテストと同じ場所・同じスタイルで)**

既存のaccountトークンkeyringテストを参考に、`save_cache_backend_password`→`load_cache_backend_password`→`delete_cache_backend_password`の往復を検証するテストを追加する。CI環境でkeyringが使えない場合の既存の対処(`#[ignore]`にしているか、モックしているか)も確認し、同じ方針に揃える。

- [ ] **Step 12: テストを実行する**

Run: `cargo test --lib` (該当モジュールのテストが通ることを確認。CI都合で`#[ignore]`なら`--ignored`でローカル確認)

- [ ] **Step 13: コミット**

```bash
git add src-tauri/src/session/
git commit -m "feat: cache backendのパスワードをkeyringへ保存する関数を追加(Issue #115 Phase 2)"
```

- [ ] **Step 14: `AppState`に`cache_dir`を保持させ、切替コマンドを実装する**

`set_cache_backend`が`Sqlite`へ切り替える際、既存のキャッシュDBファイル(`cache_dir.join("cache.db")`)を再度開く必要があるが、現在の`AppState`(`src-tauri/src/state.rs`)はそのパスを保持していない。まず`AppState`に`cache_dir: std::path::PathBuf`フィールドを追加する:

`state.rs`の`AppState`構造体定義に追加する:

```rust
    pub cache_dir: std::path::PathBuf,
```

`AppState::new`/`new_with_sound`の引数に`cache_dir: std::path::PathBuf`を追加し(既存の`secrets, settings, drafts, cache`の並びに追加)、`Self { ... }`の初期化子に`cache_dir,`を追加する。`new_for_test`(`#[cfg(test)]`)は`cache_dir: std::env::temp_dir()`のようなダミー値を渡す(テストは常にSQLiteインメモリを使い、`set_cache_backend`のSqlite再オープン分岐を通らないため実際に読まれることはない)。

`src-tauri/src/lib.rs`の`run()`内、`app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));`の呼び出しに`cache_dir.clone()`(既存の`cache_dir`変数、`cache_dir.join("cache.db")`で使っているのと同じもの)を追加する。

次に`src-tauri/src/commands/cache_backend.rs`を新規作成する。**接続失敗時の挙動(重要)**: 起動時は失敗してもアプリを止めずSQLiteへフォールバックする(Phase 1から踏襲)が、**ユーザーが設定画面から明示的に切り替えた場合は、接続確認に失敗したらSQLiteへ無言で戻さず、エラーをそのままフロントエンドへ返し、現在のバックエンド(切替前のもの)を維持する**。設定画面が「Postgresに切り替わった」と誤表示したまま実体はフォールバックしている、という不整合を避けるため:

```rust
//! バックエンド切替コマンド(Issue #115 Phase 2)。設定画面からの手動切替を扱う。
//! 起動時のフォールバック(lib.rs::run()内、接続失敗時は無言でSQLiteへ)とは扱いが異なる:
//! ここでの接続失敗はフロントエンドへそのままエラーを返し、切替前のバックエンドを維持する
//! (設定画面が実体と食い違う表示になることを防ぐため)。

use crate::domain::CacheBackendConfig;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn get_cache_backend(state: State<'_, AppState>) -> Result<CacheBackendConfig, String> {
    state.settings.load_cache_backend().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_cache_backend(
    state: State<'_, AppState>,
    config: CacheBackendConfig,
    password: Option<String>,
) -> Result<(), String> {
    let new_backend: std::sync::Arc<dyn crate::store::note_cache::NoteCacheBackend> = match &config {
        CacheBackendConfig::Sqlite => {
            let conn = crate::store::db::open_cache(&state.cache_dir.join("cache.db")).map_err(|e| e.to_string())?;
            std::sync::Arc::new(crate::store::SqliteBackend::new(conn))
        }
        CacheBackendConfig::Postgres { host, port, database, user } => {
            let password = password.ok_or("password is required for Postgres backend")?;
            let params = crate::store::postgres_backend::PostgresConnectParams {
                host: host.clone(),
                port: *port,
                database: database.clone(),
                user: user.clone(),
                password: password.clone(),
            };
            // 接続確認に失敗したらここでErrを返す(切替前のバックエンドはまだ差し替えていない)。
            let backend = crate::store::postgres_backend::PostgresBackend::connect(&params)
                .await
                .map_err(|e| format!("failed to connect to Postgres: {e}"))?;
            crate::session::save_cache_backend_password(&password).map_err(|e| e.to_string())?;
            std::sync::Arc::new(backend)
        }
    };

    // ここまで来て初めて実際に差し替える(接続確認済みのバックエンドのみをswapする)。
    state.cache.swap_backend(new_backend);
    state.settings.save_cache_backend(&config).map_err(|e| e.to_string())?;
    Ok(())
}
```

`crate::session::save_cache_backend_password`はStep 10で実際に配置したモジュールパスに合わせる。

- [ ] **Step 15: `lib.rs`にコマンドを登録する**

`src-tauri/src/commands/mod.rs`を開き、既存の`pub mod account;`等の並びに追加する:

```rust
pub mod cache_backend;
```

`src-tauri/src/lib.rs`を開き、`specta_builder()`内の既存コマンド一覧(`commands::column::search_cache_notes,`等が並んでいる箇所)に追加する:

```rust
            commands::cache_backend::get_cache_backend,
            commands::cache_backend::set_cache_backend,
```

- [ ] **Step 16: 起動時の接続失敗フォールバックを実装する**

`src-tauri/src/lib.rs`の`run()`内、以下の既存コード(`let cache_conn = db::open_cache(&cache_dir.join("cache.db")).expect(...)`から`app.manage(AppState::new(...))`まで)を書き換える:

```rust
// 変更前(現行コード、lib.rs run()内):
let cache_conn =
    db::open_cache(&cache_dir.join("cache.db")).expect("failed to open cache db");
let cache = NoteCacheStore::new(store::SqliteBackend::new(cache_conn));
app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));
```

これを以下へ置き換える:

```rust
let cache_conn =
    db::open_cache(&cache_dir.join("cache.db")).expect("failed to open cache db");
let configured_backend = settings.load_cache_backend().unwrap_or_default();
let cache_backend: std::sync::Arc<dyn store::note_cache::NoteCacheBackend> = match configured_backend {
    domain::CacheBackendConfig::Sqlite => {
        std::sync::Arc::new(store::SqliteBackend::new(cache_conn))
    }
    domain::CacheBackendConfig::Postgres { host, port, database, user } => {
        match session::load_cache_backend_password() {
            Ok(Some(password)) => {
                let params = store::postgres_backend::PostgresConnectParams {
                    host, port, database, user, password,
                };
                match tauri::async_runtime::block_on(store::postgres_backend::PostgresBackend::connect(&params)) {
                    Ok(backend) => std::sync::Arc::new(backend),
                    Err(e) => {
                        log::error!(
                            "failed to connect to configured Postgres cache backend at startup, \
                             falling back to SQLite cache: {e}"
                        );
                        std::sync::Arc::new(store::SqliteBackend::new(cache_conn))
                    }
                }
            }
            _ => {
                log::error!(
                    "Postgres cache backend configured but no password found in keyring, \
                     falling back to SQLite cache"
                );
                std::sync::Arc::new(store::SqliteBackend::new(cache_conn))
            }
        }
    }
};
let cache = NoteCacheStore::new_from_arc(cache_backend);
app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));
```

上記の`cache_conn`(`rusqlite::Connection`、`Copy`ではない)は3箇所の分岐に登場するが、実行時に到達するのはそのうち1箇所だけなので(`match`の各アームは互いに排他)、Rustの借用チェッカー上も問題なくコンパイルできる(if/elseの各分岐で同じ値をmoveするのと同じ扱い)。

`NoteCacheStore::new`は`impl NoteCacheBackend + 'static`(所有権を取る値)を受け取るが、ここでは既に`Arc<dyn NoteCacheBackend>`を持っているため、`note_cache.rs`に`pub fn new_from_arc(backend: std::sync::Arc<dyn NoteCacheBackend>) -> Self`を追加すること(Step 1の`NoteCacheStore`定義に併せて追加):

```rust
    pub fn new_from_arc(backend: std::sync::Arc<dyn NoteCacheBackend>) -> Self {
        Self { backend: std::sync::Mutex::new(backend) }
    }
```

`run()`は同期関数(`pub fn run()`、`async fn`ではない)なので、`tauri::async_runtime::block_on`で起動時1回だけ同期的に待つ(既存の`setup`クロージャ内も同期のまま)。

- [ ] **Step 17: 既存テストと`cargo check`を確認する**

Run: `cargo check --lib && cargo test --lib` (from `src-tauri/`)
Expected: 成功する。`AppState::new_for_test`(既存のテスト用コンストラクタ)がある場合、そちらは常にSQLiteバックエンドを使うようにし、Postgres分岐を通らないことを確認する(既存の全テストが引き続きSQLiteのインメモリDBで動くこと)。

- [ ] **Step 18: `cargo tauri dev`向けにTSバインディングを再生成する**

Run: `cargo test generates_frontend_bindings` (from `src-tauri/`。CLAUDE.mdに記載の通りこのテストが`frontend/src/bindings/tauri.gen.ts`を再生成する)
Expected: 成功し、`frontend/src/bindings/tauri.gen.ts`に`getCacheBackend`/`setCacheBackend`と`CacheBackendConfig`型が追加される。

- [ ] **Step 19: コミット**

```bash
git add src-tauri/src/commands/cache_backend.rs src-tauri/src/lib.rs src-tauri/src/state.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: バックエンド切替コマンドと起動時フォールバックを実装(Issue #115 Phase 2)"
```

---

### Task 4: フロントエンド設定UI

**Files:**
- Create: `frontend/src/ui/settings/CacheBackendSettings.svelte`
- Modify: `frontend/src/ui/settings/`配下の設定画面の親コンポーネント(既存の設定セクション一覧にこのセクションを追加する箇所を探す。`docs/design/style-guide.md`のフォームコンポーネント規約に従うこと)

**Interfaces:**
- Consumes: Task 3で生成された`frontend/src/bindings/tauri.gen.ts`の`getCacheBackend()`/`setCacheBackend(config, password)`関数、`CacheBackendConfig`型

- [ ] **Step 1: 既存の設定セクションの実装パターンを確認する**

`frontend/src/ui/settings/MuteSection.svelte`と`AboutSection.svelte`を読み、以下を確認する: (a) Svelte 5 runes(`$state`/`$effect`)の使い方、(b) `tauri.gen.ts`の`commands`名前空間経由のコマンド呼び出しパターン(`import { commands } from "../../bindings/tauri.gen"; commands.xxx(...)`、個別関数の直接importではない)、(c) `busy`/`err`のtry/catchパターン、(d) `docs/design/style-guide.md`のフォーム要素(input/select/button)のクラス・サイズ規約。

- [ ] **Step 2: `CacheBackendSettings.svelte`を作る**

`MuteSection.svelte`と同じSvelte 5 runes・`commands`名前空間パターンで書く:

```svelte
<script lang="ts">
  import { commands } from "../../bindings/tauri.gen";
  import type { CacheBackendConfig } from "../../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let mode = $state<"Sqlite" | "Postgres">("Sqlite");
  let host = $state("");
  let port = $state(5432);
  let database = $state("");
  let user = $state("");
  let password = $state("");
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  $effect(() => {
    void commands.getCacheBackend().then((cfg) => {
      mode = cfg.type;
      if (cfg.type === "Postgres") {
        host = cfg.host;
        port = cfg.port;
        database = cfg.database;
        user = cfg.user;
      }
    });
  });

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const config: CacheBackendConfig =
        mode === "Sqlite" ? { type: "Sqlite" } : { type: "Postgres", host, port, database, user };
      await commands.setCacheBackend(config, mode === "Postgres" ? password : null);
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

<label class="flex items-center gap-2">
  <input type="radio" bind:group={mode} value="Sqlite" />
  SQLite(ローカル、既定)
</label>
<label class="flex items-center gap-2">
  <input type="radio" bind:group={mode} value="Postgres" />
  PostgreSQL
</label>

{#if mode === "Postgres"}
  <div class="mt-2 flex flex-col gap-2">
    <label>ホスト <input type="text" bind:value={host} /></label>
    <label>ポート <input type="number" bind:value={port} /></label>
    <label>データベース名 <input type="text" bind:value={database} /></label>
    <label>ユーザー名 <input type="text" bind:value={user} /></label>
    <label>パスワード <input type="password" bind:value={password} /></label>
  </div>
{/if}

{#if err}
  <p class="text-sm text-destructive" role="alert">接続に失敗しました: {err}</p>
{/if}
{#if saved}
  <p class="text-sm text-muted-foreground">切り替えました。</p>
{/if}

<Button onclick={save} disabled={busy}>{busy ? "接続確認中…" : "保存して切り替え"}</Button>
```

`Button`コンポーネントのprops名(`onclick` vs `on:click`)・`class`のトークン(`text-destructive`等)は`MuteSection.svelte`の実際の記述と完全に一致させること(推測で書いた値は実際のコンポーネント定義を見て修正する)。

- [ ] **Step 3: 親の設定画面へセクションを追加する**

Step 1で確認した設定画面の親コンポーネントに`<CacheBackendSettings />`を追加する(既存の他セクションと同じ並びのimport・配置)。

- [ ] **Step 4: `pnpm check`を実行する**

Run: `cd frontend && pnpm check`
Expected: 型エラーなし(`tauri.gen.ts`の`CacheBackendConfig`型・関数シグネチャと一致していること)。

- [ ] **Step 5: 動作確認(自動テストではなく手動)**

`cargo tauri dev`(リポジトリルートから)で起動し、設定画面でSQLite→(Dockerで用意した)Postgres→SQLiteの切替がエラー無く行えること、Postgresの接続情報を誤らせた場合にエラーメッセージが表示されバックエンドが切り替わらないことを目視確認する。確認後は自分で起動した`cargo tauri dev`を停止すること。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/settings/CacheBackendSettings.svelte frontend/src/ui/settings/
git commit -m "feat: キャッシュバックエンド切替の設定UIを追加(Issue #115 Phase 2)"
```
