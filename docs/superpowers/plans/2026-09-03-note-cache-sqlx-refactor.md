# note cache: rusqlite→sqlx非同期化 (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** note cache(`store/db.rs`のキャッシュスキーマ, `store/note_cache.rs`, `store/user_ref.rs`)を`rusqlite`(同期・`Mutex<Connection>`)から`sqlx::SqlitePool`(非同期)へ移行し、外部から見た挙動(呼び出し元API・クエリ結果)を一切変えずに、Phase 2(PostgreSQL対応)・Phase 3(MySQL対応)へ進めるための土台を作る。

**Architecture:** `store/db.rs::open_cache`/`open_cache_in_memory`が`rusqlite::Connection`ではなく`sqlx::SqlitePool`(単一プロセス内の直列化を保つため`max_connections(1)`)を返すようにし、`store/user_ref.rs`と`store/note_cache.rs`の全関数を`async fn`化してsqlxのバインドAPIへ載せ替える。呼び出し元4ファイル(`commands/column.rs`, `commands/mute.rs`, `commands/note.rs`, `stream/connection.rs`)は既にasync fn内から呼んでいるため`.await`を追加するだけで済む。トレイト抽出(`NoteCacheBackend`)による`SqliteBackend`/将来の`PostgresBackend`/`MySqlBackend`の切り替えは本Phase最後のタスクで行う。SQL文字列自体・スキーマ・クエリロジックは(下記の意図的な変更点を除き)現状と同一に保つ。

**Tech Stack:** Rust, `sqlx`(features: `sqlite`, `runtime-tokio`), `async-trait`, `tokio`(既存)。sea-query・PostgreSQL/MySQL対応はPhase 2以降(本計画のスコープ外)。

## Global Constraints

- 外部から見た挙動は不変であること。SQL文字列・クエリ結果は下記の意図的変更(UNIQUE制約追加、`delete_matching`のTEMP TABLE撤去)を除き現状と同一
- `sqlx`のバージョン・`SqliteConnectOptions`等の正確なAPI(メソッド名)は、実装時に`cargo doc --open`または https://docs.rs/sqlx で使用する固定バージョンのドキュメントを必ず確認すること。本計画のコード例はsqlxの一般的なAPI形状に基づくが、細部のメソッド名はバージョン間で変わりうる
- 各タスクの最後に `cd src-tauri && cargo test` が green であることを確認してからコミットする
- コミットメッセージは日本語の1行のみ(このリポジトリの規約)

---

### Task 1: `store/db.rs` — sqlxプール化 + UNIQUE制約マイグレーション

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/store/db.rs`(全体書き換え)

**Interfaces:**
- Consumes: なし(このタスクは他モジュールに依存しない)
- Produces:
  - `pub async fn open_cache(path: &Path) -> Result<sqlx::SqlitePool>`(旧: `Connection`を返していた)
  - `pub async fn open_cache_in_memory() -> Result<sqlx::SqlitePool>`(`#[cfg(test)]`のまま)
  - `open_settings`/`migrate`(account/column設定側)は**このタスクでは変更しない**(rusqliteのまま、`SettingsStore`が今も同期で使うため)

- [ ] **Step 1: 依存クレートを追加する**

```bash
cd src-tauri
cargo add sqlx --no-default-features --features sqlite,runtime-tokio
cargo add async-trait
```

`Cargo.toml`の`[dependencies]`に追記された行が、既存の`rusqlite = { version = "0.40.1", features = ["bundled"] }`の近くに来るよう手で並び替えてよい(機能的な影響はない)。`rusqlite`は`open_settings`側でまだ使うため削除しない。

- [ ] **Step 2: 既存の`db.rs`テストをsqlx/`#[tokio::test]`前提の期待値に書き換えてから実装する(TDD)**

`src-tauri/src/store/db.rs`の`#[cfg(test)] mod tests`をまるごと以下に置き換える(既存の6テスト+新規1テストの計7テスト。ロジックは既存と同じだが`async`化し、新規テストで側テーブルのUNIQUE制約+重複排除マイグレーションを検証する):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn migrates_old_column_def_to_groups() {
        // account/column設定はこのタスクでは触らないため rusqlite のまま検証する。
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE column_def (
                id TEXT PRIMARY KEY, account_id TEXT NOT NULL, kind TEXT NOT NULL,
                ord INTEGER NOT NULL, width INTEGER NOT NULL, filter TEXT NOT NULL,
                notify_sound INTEGER NOT NULL, notify_desktop INTEGER NOT NULL);
             INSERT INTO column_def VALUES('c1','a1','{}',2,360,'{}',0,0);",
        )
        .unwrap();
        conn.execute_batch(SETTINGS_SCHEMA).unwrap();
        migrate(&conn).unwrap();

        let gid: Option<String> = conn
            .query_row("SELECT group_id FROM column_def WHERE id='c1'", [], |r| r.get(0))
            .unwrap();
        let gid = gid.expect("group_id should be set");
        let (gord, gwidth): (i32, i32) = conn
            .query_row("SELECT ord, width FROM column_group WHERE id=?1", [&gid], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(gord, 2);
        assert_eq!(gwidth, 360);

        migrate(&conn).unwrap();
        let groups: i32 = conn
            .query_row("SELECT COUNT(*) FROM column_group", [], |r| r.get(0))
            .unwrap();
        assert_eq!(groups, 1);
    }

    #[tokio::test]
    async fn migrate_cache_backfills_created_at_from_note() {
        let pool = open_cache_in_memory_with_legacy_schema().await;
        sqlx::query(
            "CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
             CREATE TABLE user (id TEXT PRIMARY KEY, username TEXT NOT NULL);
             CREATE TABLE column_note (
                 column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                 PRIMARY KEY (column_id, note_id)
             );",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO note (id, created_at) VALUES ('n1', 12345)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO column_note (column_id, note_id, received_at) VALUES ('c1', 'n1', 999)")
            .execute(&pool)
            .await
            .unwrap();

        migrate_cache(&pool).await.unwrap();

        let created_at: i64 = sqlx::query_scalar(
            "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(created_at, 12345);

        // 冪等: 再実行してもエラーにならず値は変わらない
        migrate_cache(&pool).await.unwrap();
        let created_at2: i64 = sqlx::query_scalar(
            "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(created_at2, 12345);

        let idx_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cn_column_created'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(idx_count, 1);
    }

    #[tokio::test]
    async fn open_cache_applies_pragma_tuning() {
        let path = std::env::temp_dir().join(format!("tsumugi_test_{}.db", uuid::Uuid::new_v4()));
        let pool = open_cache(&path).await.unwrap();

        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&pool).await.unwrap();
        assert_eq!(synchronous, 1); // NORMAL

        let temp_store: i64 = sqlx::query_scalar("PRAGMA temp_store").fetch_one(&pool).await.unwrap();
        assert_eq!(temp_store, 2); // MEMORY

        let cache_size: i64 = sqlx::query_scalar("PRAGMA cache_size").fetch_one(&pool).await.unwrap();
        assert_eq!(cache_size, -20000);

        let mmap_size: i64 = sqlx::query_scalar("PRAGMA mmap_size").fetch_one(&pool).await.unwrap();
        assert_eq!(mmap_size, 67_108_864);

        drop(pool);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[tokio::test]
    async fn migrate_cache_adds_user_normalization_columns() {
        let pool = open_cache_in_memory_with_legacy_schema().await;
        sqlx::query(
            "CREATE TABLE user (
                id TEXT PRIMARY KEY, username TEXT NOT NULL, host TEXT, name TEXT,
                is_bot INTEGER NOT NULL DEFAULT 0, is_cat INTEGER NOT NULL DEFAULT 0,
                followers_count INTEGER NOT NULL DEFAULT 0,
                following_count INTEGER NOT NULL DEFAULT 0,
                notes_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
            CREATE TABLE column_note (
                column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
                PRIMARY KEY (column_id, note_id)
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        migrate_cache(&pool).await.unwrap();

        for col in [
            "avatar_url", "bio", "banner_url", "emojis",
            "instance_name", "instance_icon_url", "instance_theme_color",
        ] {
            assert!(column_exists(&pool, "user", col).await.unwrap(), "missing column: {col}");
        }
        migrate_cache(&pool).await.unwrap();
    }

    /// Issue #115: 側テーブルに重複行があっても、UNIQUEインデックス作成前に
    /// 重複排除してからインデックスを張ること(既存の蓄積データを壊さずに移行できること)。
    #[tokio::test]
    async fn migrate_cache_dedupes_side_tables_before_creating_unique_index() {
        let pool = open_cache_in_memory().await.unwrap();
        // 正規のパスでは起きないはずの重複行を素のSQLで作る(移行前の実データを模倣)。
        sqlx::query(
            "INSERT INTO note (id, created_at, visibility, user_id, payload) VALUES ('n1', 100, 'home', 'u1', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ('n1', '👍', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ('n1', '👍', 1)")
            .execute(&pool)
            .await
            .unwrap();

        // open_cache_in_memory は migrate_cache 込みなので、この時点で既にインデックスは張られているはず。
        let idx_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_nr_unique'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(idx_count, 1);

        // 重複行は1件に集約されていること。
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        // 冪等: 再実行してもエラーにならない(UNIQUE違反にならない)。
        migrate_cache(&pool).await.unwrap();
    }

    /// テスト専用: `migrate_cache`が期待する`note`/`column_note`テーブルより前段階の
    /// (=CACHE_SCHEMA適用前の)状態を作るためのヘルパー。CACHE_SCHEMAには依存しない空プール。
    async fn open_cache_in_memory_with_legacy_schema() -> sqlx::SqlitePool {
        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::from_str("sqlite::memory:").unwrap())
            .await
            .unwrap()
    }
}
```

- [ ] **Step 3: テストを実行して失敗を確認する**

```bash
cd src-tauri && cargo test --lib store::db
```

Expected: コンパイルエラー(`open_cache`/`migrate_cache`/`column_exists`が`sqlx::SqlitePool`を受け取らないため)。

- [ ] **Step 4: `db.rs`本体を書き換える**

`CACHE_SCHEMA`定数の末尾(`column_fetch_boundary`の`CREATE TABLE`の後)に、以下のUNIQUEインデックス作成文を**追加しない**こと(新規DBでも`migrate_cache`側の`add_unique_index_with_dedup`を毎回通す設計にして、新規/既存で分岐を持たないようにする)。`CACHE_SCHEMA`定数のテキスト自体は変更しない。

`open_settings`/`migrate`/`column_exists`(既存のrusqlite版、account/column設定用)はそのまま残す。以下を同じファイルに追記・置き換える形で実装する:

```rust
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;

/// ノートキャッシュDBを開き（無ければ作成し）、スキーマを適用する。
/// `max_connections(1)`: 旧`rusqlite`+`Mutex<Connection>`と同じ「プロセス内で常に単一接続に
/// 直列化する」挙動をPhase 1では維持する(挙動不変が目標のため、複数コネクションによる
/// 並行アクセスは今回スコープ外)。
pub async fn open_cache(path: &Path) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .pragma("temp_store", "2") // MEMORY
        .pragma("cache_size", "-20000")
        .pragma("mmap_size", "67108864");
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    sqlx::query(CACHE_SCHEMA).execute(&pool).await?;
    migrate_cache(&pool).await?;
    enable_incremental_vacuum(&pool).await?;
    Ok(pool)
}

async fn enable_incremental_vacuum(pool: &SqlitePool) -> Result<()> {
    let mode: i64 = sqlx::query_scalar("PRAGMA auto_vacuum").fetch_one(pool).await?;
    const INCREMENTAL: i64 = 2;
    if mode != INCREMENTAL {
        sqlx::query("PRAGMA auto_vacuum = INCREMENTAL").execute(pool).await?;
        sqlx::query("VACUUM").execute(pool).await?;
    }
    Ok(())
}

async fn migrate_cache(pool: &SqlitePool) -> Result<()> {
    if !column_exists(pool, "column_note", "created_at").await? {
        sqlx::query("ALTER TABLE column_note ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
        sqlx::query(
            "UPDATE column_note SET created_at = (
                SELECT created_at FROM note WHERE note.id = column_note.note_id
            )
            WHERE EXISTS (SELECT 1 FROM note WHERE note.id = column_note.note_id)",
        )
        .execute(pool)
        .await?;
    }
    if !column_exists(pool, "user", "instance_name").await? {
        sqlx::query(
            "ALTER TABLE user ADD COLUMN avatar_url TEXT;
             ALTER TABLE user ADD COLUMN bio TEXT;
             ALTER TABLE user ADD COLUMN banner_url TEXT;
             ALTER TABLE user ADD COLUMN emojis TEXT NOT NULL DEFAULT '{}';
             ALTER TABLE user ADD COLUMN instance_name TEXT;
             ALTER TABLE user ADD COLUMN instance_icon_url TEXT;
             ALTER TABLE user ADD COLUMN instance_theme_color TEXT;",
        )
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_cn_column_created \
         ON column_note(column_id, created_at DESC, note_id DESC)",
    )
    .execute(pool)
    .await?;

    // Issue #115: 側テーブルへのUNIQUE制約追加(重複排除してからインデックス作成)。
    // 既存の蓄積データ(note/user/column_note等)は一切触らない。
    add_unique_index_with_dedup(pool, "note_reaction", &["note_id", "emoji_key"], "idx_nr_unique").await?;
    add_unique_index_with_dedup(pool, "note_tag", &["note_id", "tag"], "idx_nt_unique").await?;
    add_unique_index_with_dedup(pool, "note_mention", &["note_id", "user_id"], "idx_nm_unique").await?;
    add_unique_index_with_dedup(pool, "note_emoji", &["note_id", "emoji"], "idx_ne_unique").await?;
    add_unique_index_with_dedup(
        pool,
        "note_file",
        &["note_id", "mime_type", "mime_category", "is_sensitive"],
        "idx_nf_unique",
    )
    .await?;
    Ok(())
}

/// `table`に`(cols...)`のUNIQUEインデックス`index_name`が無ければ、重複行を
/// (rowidが最小の1行だけ残して)削除してからインデックスを作成する。
/// 既にインデックスがあれば何もしない(起動のたびに全表走査しないため)。
async fn add_unique_index_with_dedup(
    pool: &SqlitePool,
    table: &str,
    cols: &[&str],
    index_name: &str,
) -> Result<()> {
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1")
            .bind(index_name)
            .fetch_one(pool)
            .await?;
    if exists > 0 {
        return Ok(());
    }
    let col_list = cols.join(", ");
    sqlx::query(&format!(
        "DELETE FROM {table} WHERE rowid NOT IN (SELECT MIN(rowid) FROM {table} GROUP BY {col_list})"
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!("CREATE UNIQUE INDEX {index_name} ON {table}({col_list})"))
        .execute(pool)
        .await?;
    Ok(())
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> Result<bool> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})")).fetch_all(pool).await?;
    for row in rows {
        let name: String = row.try_get("name")?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// インメモリキャッシュDB（テスト用）。
#[cfg(test)]
pub async fn open_cache_in_memory() -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?;
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    sqlx::query(CACHE_SCHEMA).execute(&pool).await?;
    migrate_cache(&pool).await?;
    enable_incremental_vacuum(&pool).await?;
    Ok(pool)
}
```

旧`column_exists`(rusqlite版、`migrate`が使う)は名前が衝突するため、こちらの新しいsqlx版は`column_exists`という同名にせず、rusqlite版と共存できるよう関数シグネチャの型(`&SqlitePool` vs `&rusqlite::Connection`)で区別する。Rustはシグネチャによるオーバーロードをサポートしないため、**rusqlite版の`column_exists`をそのまま残しつつ、sqlx版は同一モジュール内に共存できない**。sqlx版を`column_exists_cache`のような別名にするか、rusqlite版を`column_exists_settings`にリネームする。リネームは呼び出し元(`migrate`関数内)も合わせて更新すること。本計画では sqlx 版を追加する側なので **sqlx版を`column_exists`のまま、rusqlite版(既存)を`column_exists_settings`にリネーム**する方針とする。

- [ ] **Step 5: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::db
```

Expected: PASS(7件)

- [ ] **Step 6: コミット**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/store/db.rs
git commit -m "note cacheのDB接続をrusqliteからsqlxへ移行し側テーブルにUNIQUE制約を追加"
```

---

### Task 2: `store/user_ref.rs` — sqlx非同期化

**Files:**
- Modify: `src-tauri/src/store/user_ref.rs`(全体書き換え)

**Interfaces:**
- Consumes: `sqlx::SqliteConnection`(Task 1の`open_cache`/`open_cache_in_memory`が返す`SqlitePool`から`pool.acquire()`または`pool.begin()`で得る)
- Produces(すべて`pub(crate) async fn`、Task 3から呼ばれる):
  - `upsert_user(conn: &mut sqlx::SqliteConnection, user: &User) -> Result<()>`
  - `fill_user_from_snapshot(conn: &mut sqlx::SqliteConnection, user: &User) -> Result<()>`
  - `fetch_users_by_ids(conn: &mut sqlx::SqliteConnection, ids: &[String]) -> Result<HashMap<String, User>>`
  - `collect_users`/`stub_user_refs`/`is_legacy_full_user`/`has_legacy_full_user`/`collect_user_id_refs`/`hydrate_user_refs` は純粋なJSON操作でDB接続を取らないため**変更不要**(シグネチャそのまま)

- [ ] **Step 1: 既存テストを`#[tokio::test]`+ sqlxの型に書き換える(TDD)**

`user_ref.rs`の`#[cfg(test)] mod tests`内で`rusqlite::Connection`を使っている箇所(`upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`を呼ぶテスト)を以下のように書き換える。対象は`upsert_user_preserves_bio_when_later_write_has_none`, `upsert_user_overwrites_always_present_fields`, `upsert_user_preserves_instance_when_later_fetch_fails`, `fill_user_from_snapshot_does_not_clobber_fresher_live_data`, `fetch_users_by_ids_returns_stored_instance_info`, `fetch_users_by_ids_returns_empty_map_for_empty_input`, および`row`ヘルパー。それ以外(`collect_users_*`, `stub_user_refs_*`, `is_legacy_full_user_*`, `has_legacy_full_user_*`, `collect_user_id_refs_*`, `hydrate_user_refs_*`)はDBを使わないため変更不要。

```rust
    async fn row(pool: &sqlx::SqlitePool, id: &str) -> (Option<String>, Option<String>, Option<String>) {
        use sqlx::Row;
        let r = sqlx::query("SELECT bio, banner_url, instance_name FROM user WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        (r.try_get(0).unwrap(), r.try_get(1).unwrap(), r.try_get(2).unwrap())
    }

    #[tokio::test]
    async fn upsert_user_preserves_bio_when_later_write_has_none() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut full = user_lite("u1", "Alice");
        full.bio = Some("hello".into());
        upsert_user(&mut conn, &full).await.unwrap();

        let lite = user_lite("u1", "Alice (updated)");
        upsert_user(&mut conn, &lite).await.unwrap();

        let (bio, _, _) = row(&pool, "u1").await;
        assert_eq!(bio, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn upsert_user_overwrites_always_present_fields() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        upsert_user(&mut conn, &user_lite("u1", "Alice")).await.unwrap();
        upsert_user(&mut conn, &user_lite("u1", "Alice2")).await.unwrap();

        let name: String = sqlx::query_scalar("SELECT name FROM user WHERE id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Alice2");
    }

    #[tokio::test]
    async fn upsert_user_preserves_instance_when_later_fetch_fails() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&mut conn, &with_instance).await.unwrap();

        let mut failed_fetch = user_lite("u1", "Alice");
        failed_fetch.instance = None;
        upsert_user(&mut conn, &failed_fetch).await.unwrap();

        let (_, _, instance_name) = row(&pool, "u1").await;
        assert_eq!(instance_name, Some("Remote".to_string()));
    }

    #[tokio::test]
    async fn fill_user_from_snapshot_does_not_clobber_fresher_live_data() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut fresh = user_lite("u1", "Alice (new name)");
        fresh.emojis = HashMap::from([("wave".to_string(), "https://example.com/wave.png".to_string())]);
        upsert_user(&mut conn, &fresh).await.unwrap();

        let mut stale_snapshot = user_lite("u1", "Alice (old name)");
        stale_snapshot.emojis = HashMap::new();
        fill_user_from_snapshot(&mut conn, &stale_snapshot).await.unwrap();

        let name: String = sqlx::query_scalar("SELECT name FROM user WHERE id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let emojis_json: String = sqlx::query_scalar("SELECT emojis FROM user WHERE id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Alice (new name)", "古いスナップショットが直近のnameを上書きしてはいけない");
        assert!(emojis_json.contains("wave"), "古いスナップショットが直近のemojisを消してはいけない");
    }

    #[tokio::test]
    async fn fetch_users_by_ids_returns_stored_instance_info() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&mut conn, &with_instance).await.unwrap();
        upsert_user(&mut conn, &user_lite("u2", "Bob")).await.unwrap();

        let ids = vec!["u1".to_string(), "u2".to_string(), "u3".to_string()];
        let users = fetch_users_by_ids(&mut conn, &ids).await.unwrap();

        assert_eq!(users.len(), 2);
        let instance = users["u1"].instance.as_ref().unwrap();
        assert_eq!(instance.name.as_deref(), Some("Remote"));
        assert!(users["u2"].instance.is_none());
    }

    #[tokio::test]
    async fn fetch_users_by_ids_returns_empty_map_for_empty_input() {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let users = fetch_users_by_ids(&mut conn, &[]).await.unwrap();
        assert!(users.is_empty());
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

```bash
cd src-tauri && cargo test --lib store::user_ref
```

Expected: コンパイルエラー(本体側がまだ`rusqlite::Connection`を取るため)。

- [ ] **Step 3: `upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`を書き換える**

```rust
use sqlx::{Row, SqliteConnection};

pub(crate) async fn upsert_user(conn: &mut SqliteConnection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(id) DO UPDATE SET
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
            bio = COALESCE(excluded.bio, user.bio),
            banner_url = COALESCE(excluded.banner_url, user.banner_url),
            instance_name = COALESCE(excluded.instance_name, user.instance_name),
            instance_icon_url = COALESCE(excluded.instance_icon_url, user.instance_icon_url),
            instance_theme_color = COALESCE(excluded.instance_theme_color, user.instance_theme_color)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count)
    .bind(user.following_count)
    .bind(user.notes_count)
    .bind(emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(instance_name)
    .bind(instance_icon_url)
    .bind(instance_theme_color)
    .execute(conn)
    .await?;
    Ok(())
}

pub(crate) async fn fill_user_from_snapshot(conn: &mut SqliteConnection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(id) DO UPDATE SET
            username = COALESCE(user.username, excluded.username),
            host = COALESCE(user.host, excluded.host),
            name = COALESCE(user.name, excluded.name),
            avatar_url = COALESCE(user.avatar_url, excluded.avatar_url),
            is_bot = user.is_bot,
            is_cat = user.is_cat,
            followers_count = user.followers_count,
            following_count = user.following_count,
            notes_count = user.notes_count,
            emojis = COALESCE(NULLIF(user.emojis, '{}'), excluded.emojis),
            bio = COALESCE(user.bio, excluded.bio),
            banner_url = COALESCE(user.banner_url, excluded.banner_url),
            instance_name = COALESCE(user.instance_name, excluded.instance_name),
            instance_icon_url = COALESCE(user.instance_icon_url, excluded.instance_icon_url),
            instance_theme_color = COALESCE(user.instance_theme_color, excluded.instance_theme_color)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count)
    .bind(user.following_count)
    .bind(user.notes_count)
    .bind(emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(instance_name)
    .bind(instance_icon_url)
    .bind(instance_theme_color)
    .execute(conn)
    .await?;
    Ok(())
}

pub(crate) async fn fetch_users_by_ids(
    conn: &mut SqliteConnection,
    ids: &[String],
) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM user WHERE id IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(conn).await?;
    for r in rows {
        let emojis_json: String = r.try_get(10)?;
        let instance_name: Option<String> = r.try_get(13)?;
        let instance_icon_url: Option<String> = r.try_get(14)?;
        let instance_theme_color: Option<String> = r.try_get(15)?;
        let instance = if instance_name.is_some() || instance_icon_url.is_some() || instance_theme_color.is_some() {
            Some(InstanceInfo {
                name: instance_name,
                icon_url: instance_icon_url,
                theme_color: instance_theme_color,
            })
        } else {
            None
        };
        let user = User {
            id: r.try_get(0)?,
            username: r.try_get(1)?,
            host: r.try_get(2)?,
            name: r.try_get(3)?,
            avatar_url: r.try_get(4)?,
            is_bot: r.try_get(5)?,
            is_cat: r.try_get(6)?,
            followers_count: r.try_get(7)?,
            following_count: r.try_get(8)?,
            notes_count: r.try_get(9)?,
            emojis: serde_json::from_str(&emojis_json).unwrap_or_default(),
            bio: r.try_get(11)?,
            banner_url: r.try_get(12)?,
            instance,
        };
        out.insert(user.id.clone(), user);
    }
    Ok(out)
}
```

`is_bot`/`is_cat`は`sqlx`が SQLite の `INTEGER` 列と Rust の `bool` を直接相互変換できるため、旧`rusqlite`版にあった`as i64`キャストと`!= 0`変換は不要になる(削除する)。

- [ ] **Step 4: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::user_ref
```

Expected: PASS(全17テスト: DB操作6件が新規async化、非DB系11件は無変更のままPASS)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/user_ref.rs
git commit -m "user_refのDB操作をsqlx非同期に移行"
```

---

### Task 3: `store/note_cache.rs` — sqlx非同期化(本体)

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`(全体書き換え)

**Interfaces:**
- Consumes:
  - `crate::store::db::{open_cache, open_cache_in_memory}() -> Result<sqlx::SqlitePool>`(Task 1)
  - `crate::store::user_ref::{upsert_user, fill_user_from_snapshot, fetch_users_by_ids}`(すべて`async fn(&mut SqliteConnection, ...)`、Task 2)
- Produces: `NoteCacheStore`の全メソッドが`async fn`になる(シグネチャの引数・戻り値の型自体は現状と同じ、`self`の借用も`&self`のまま。呼び出し元は`.await`を追加するだけでよい)。このタスクではまだ`NoteCacheStore`自身が`sqlx::SqlitePool`を直接保持する(トレイト抽出はTask 4)

このタスクは対象範囲が広いため、以下のサブステップ群に分けて進める。**各サブステップの完了時点でファイル全体が再びコンパイルできる必要はない**(このタスク全体で1回`cargo test`が通ればよい)ため、実装時は一気に書き換えてから最後にテストを流す形でよい。ただしTDDの原則に沿い、テストは先に書き換えて「何を実装すべきか」を確定させてから本体を書く。

- [ ] **Step 1: `NoteCacheStore`の型定義とコンストラクタを書き換える**

```rust
use crate::domain::{Note, Visibility};
use crate::error::Result;
use crate::store::user_ref::{
    collect_user_id_refs, collect_users, fetch_users_by_ids, fill_user_from_snapshot,
    has_legacy_full_user, hydrate_user_refs, is_legacy_full_user, stub_user_refs, upsert_user,
};
use sqlx::{Row, SqliteConnection, SqlitePool};

pub struct NoteCacheStore {
    pool: SqlitePool,
}

impl NoteCacheStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
```

- [ ] **Step 2: `upsert_note`と関連テーブルUPSERT化(DELETE→INSERTからの置き換え)を書く**

```rust
/// note + user + 関連テーブルを upsert する。関連テーブルはTask 1で追加したUNIQUE制約に基づき
/// `INSERT ... ON CONFLICT DO UPDATE`で置き換える(複数プロセスからの同時書き込みでも
/// 一時的な重複行・空状態が起きないようにするため。設計書「複数端末の同時書き込みに関する整合性」参照)。
async fn upsert_note(conn: &mut SqliteConnection, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false) as i64;

    for user in collect_users(n) {
        upsert_user(conn, user).await?;
    }

    sqlx::query(
        "INSERT OR REPLACE INTO note (
            id, created_at, text, text_length, cw, visibility, local_only, user_id,
            reply_id, reply_user_id, renote_id, channel_id, via, lang,
            files_count, has_poll, has_link, is_pinned,
            reaction_count, renote_count, reply_count, my_reaction,
            is_renoted_by_me, is_favorited_by_me, payload
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25
        )",
    )
    .bind(&n.id)
    .bind(n.created_at)
    .bind(&n.text)
    .bind(text_length)
    .bind(&n.cw)
    .bind(visibility_str(n.visibility))
    .bind(n.local_only)
    .bind(&n.user.id)
    .bind(Option::<String>::None) // reply_user_id: Note には無いため NULL
    .bind(&n.reply_id)
    .bind(&n.renote_id)
    .bind(&n.channel_id)
    .bind(&n.via)
    .bind(&n.lang)
    .bind(n.files.len() as i64)
    .bind(n.poll.is_some())
    .bind(has_link != 0)
    .bind(n.is_pinned)
    .bind(n.reaction_count)
    .bind(n.renote_count)
    .bind(n.reply_count)
    .bind(&n.my_reaction)
    .bind(n.is_renoted_by_me)
    .bind(n.is_favorited_by_me)
    .bind(payload)
    .execute(&mut *conn)
    .await?;

    for (emoji, count) in &n.reactions {
        sqlx::query(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?1, ?2, ?3)
             ON CONFLICT(note_id, emoji_key) DO UPDATE SET count = excluded.count",
        )
        .bind(&n.id)
        .bind(emoji)
        .bind(*count)
        .execute(&mut *conn)
        .await?;
    }
    for tag in &n.tags {
        sqlx::query(
            "INSERT INTO note_tag (note_id, tag) VALUES (?1, ?2)
             ON CONFLICT(note_id, tag) DO NOTHING",
        )
        .bind(&n.id)
        .bind(tag)
        .execute(&mut *conn)
        .await?;
    }
    for uid in &n.mentions {
        sqlx::query(
            "INSERT INTO note_mention (note_id, user_id) VALUES (?1, ?2)
             ON CONFLICT(note_id, user_id) DO NOTHING",
        )
        .bind(&n.id)
        .bind(uid)
        .execute(&mut *conn)
        .await?;
    }
    for e in n.emojis.keys() {
        sqlx::query(
            "INSERT INTO note_emoji (note_id, emoji) VALUES (?1, ?2)
             ON CONFLICT(note_id, emoji) DO NOTHING",
        )
        .bind(&n.id)
        .bind(e)
        .execute(&mut *conn)
        .await?;
    }
    for f in &n.files {
        sqlx::query(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(note_id, mime_type, mime_category, is_sensitive) DO NOTHING",
        )
        .bind(&n.id)
        .bind(&f.mime_type)
        .bind(mime_category(&f.mime_type))
        .bind(f.is_sensitive)
        .execute(&mut *conn)
        .await?;
    }

    // 旧行(現在の note の内容に含まれなくなったreaction/tag/mention/emoji/file)を掃除する。
    // 例: リアクションが取り消された、タグが編集で消えた、等。
    sqlx::query(
        "DELETE FROM note_reaction WHERE note_id = ?1 AND emoji_key NOT IN (SELECT value FROM json_each(?2))",
    )
    .bind(&n.id)
    .bind(serde_json::to_string(&n.reactions.keys().collect::<Vec<_>>())?)
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM note_tag WHERE note_id = ?1 AND tag NOT IN (SELECT value FROM json_each(?2))")
        .bind(&n.id)
        .bind(serde_json::to_string(&n.tags)?)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "DELETE FROM note_mention WHERE note_id = ?1 AND user_id NOT IN (SELECT value FROM json_each(?2))",
    )
    .bind(&n.id)
    .bind(serde_json::to_string(&n.mentions)?)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM note_emoji WHERE note_id = ?1 AND emoji NOT IN (SELECT value FROM json_each(?2))",
    )
    .bind(&n.id)
    .bind(serde_json::to_string(&n.emojis.keys().collect::<Vec<_>>())?)
    .execute(&mut *conn)
    .await?;
    let current_file_keys: Vec<String> = n
        .files
        .iter()
        .map(|f| format!("{}\u{0}{}\u{0}{}", f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64))
        .collect();
    let existing_files: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT rowid, mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = ?1",
    )
    .bind(&n.id)
    .fetch_all(&mut *conn)
    .await?;
    for (rowid, mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = format!("{mime_type}\u{0}{mime_category_val}\u{0}{is_sensitive}");
        if !current_file_keys.contains(&key) {
            sqlx::query("DELETE FROM note_file WHERE rowid = ?1").bind(rowid).execute(&mut *conn).await?;
        }
    }
    Ok(())
}
```

`note_file`だけ`json_each`での`NOT IN`ではなく行ごとの比較にしているのは、UNIQUEキーが複合(4列)であり`json_each`一発でタプル比較できないため。`rowid`(SQLite組み込み)を使って個別削除する。

**既知の残課題(Phase 2/3で要再検討)**:
- 上記の「現在のnoteの内容に無くなった行を掃除するDELETE」は、UPSERT本体とは別のSQL文であるため、Phase 1の前提(`max_connections(1)`によるプロセス内直列化)が崩れる状況(=複数端末が外部DBに同時書き込みする場合)では、このDELETEと他トランザクションのINSERTが競合し、「他端末が今まさに追加した行を誤って消す」可能性がゼロではない。Phase 1は単一プロセス前提のため実害はないが、Phase 2でPostgres対応する際にこのクリーンアップ方式の安全性を再検討すること
- `json_each`はSQLiteのJSON1拡張に依存する。`sqlx`の`sqlite`featureがJSON1を有効化したビルドを使っているかは、実装時にStep 8のテスト実行で確認する(有効でなければテストが即座に失敗するため、リスクは低い)

- [ ] **Step 3: 読み取り系(`load_cached`/`load_cached_before`/`get_note`/`update_note`/`resolve_payload_rows`/自己修復)を書く**

```rust
impl NoteCacheStore {
    pub async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        let now = now_epoch();
        for n in notes {
            upsert_note(&mut tx, n).await?;
            sqlx::query(
                "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?1, ?2, ?3, ?4)",
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

    pub async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    pub async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?2",
        )
        .bind(column_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        resolve_payload_rows(&self.pool, rows).await
    }

    pub async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1 AND cn.note_id < ?2
             ORDER BY cn.note_id DESC
             LIMIT ?3",
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

    pub async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, payload FROM note WHERE id = ?1")
                .bind(note_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(match row {
            Some((id, payload)) => resolve_payload_rows(&self.pool, vec![(id, payload)]).await?.into_iter().next(),
            None => None,
        })
    }

    pub async fn update_note(&self, note: &Note) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        upsert_note(&mut conn, note).await
    }
}

async fn resolve_payload_rows(pool: &SqlitePool, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
    let mut conn = pool.acquire().await?;
    let mut values: Vec<(String, serde_json::Value)> = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(mut v) => {
                if has_legacy_full_user(&v) {
                    self_heal_legacy_row(&mut conn, &id, &mut v).await?;
                }
                values.push((id, v));
            }
            Err(e) => {
                log::warn!("skipping note cache row {id} with unparsable payload: {e}");
            }
        }
    }

    let mut ids = Vec::new();
    for (_, v) in &values {
        collect_user_id_refs(v, &mut ids);
    }
    ids.sort();
    ids.dedup();
    let users = fetch_users_by_ids(&mut conn, &ids).await?;

    let mut out = Vec::with_capacity(values.len());
    for (id, mut v) in values {
        if !hydrate_user_refs(&mut v, &users) {
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

async fn self_heal_legacy_row(conn: &mut SqliteConnection, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(conn, value).await?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        sqlx::query("UPDATE note SET payload = ?1 WHERE id = ?2")
            .bind(new_payload)
            .bind(note_id)
            .execute(conn)
            .await?;
    }
    Ok(())
}

fn self_heal_node<'a>(
    conn: &'a mut SqliteConnection,
    node: &'a mut serde_json::Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + 'a>> {
    Box::pin(async move {
        let mut changed = false;
        if let Some(user_value) = node.get("user").cloned() {
            if is_legacy_full_user(&user_value) {
                if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                    fill_user_from_snapshot(conn, &user).await?;
                    if let Some(id) = user_value.get("id").cloned() {
                        node["user"] = serde_json::json!({ "id": id });
                        changed = true;
                    }
                }
            }
        }
        if node.get("renote").map(|r| r.is_object()).unwrap_or(false) {
            changed |= self_heal_node(conn, &mut node["renote"]).await?;
        }
        Ok(changed)
    })
}
```

`self_heal_node`は元が再帰関数だが、Rustの`async fn`は無限サイズの型になるため直接の再帰ができない。`Pin<Box<dyn Future>>`で手動boxingして再帰させる(async再帰の標準的な回避策。`async_recursion`クレートを使ってもよいが、依存を増やさずこの形で書ける)。

- [ ] **Step 4: 境界CRUD・カウント系を書く**

```rust
impl NoteCacheStore {
    pub async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM column_note WHERE column_id = ?1").bind(column_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = ?1").bind(column_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let v: Option<String> = sqlx::query_scalar(
            "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = ?1",
        )
        .bind(column_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(v)
    }

    pub async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET
                oldest_fetched_id = MIN(oldest_fetched_id, excluded.oldest_fetched_id)",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        sqlx::query("DELETE FROM column_fetch_boundary").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn note_count(&self) -> Result<i32> {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(&self.pool).await?;
        Ok(count)
    }

    pub async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM note WHERE created_at >= ?1")
            .bind(since_epoch_secs)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}
```

- [ ] **Step 5: `prune`/`delete_matching`/`shrink_to_size`/`db_size_bytes`を書く(TEMP TABLE撤去)**

```rust
impl NoteCacheStore {
    pub async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        let mut deleted: i64 = 0;
        {
            let mut tx = self.pool.begin().await?;
            if max_age_days > 0 {
                let cutoff = now_epoch() - max_age_days as i64 * 86_400;
                let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM note WHERE created_at < ?1")
                    .bind(cutoff)
                    .fetch_all(&mut *tx)
                    .await?;
                deleted += delete_matching(&mut tx, &ids).await?;
            }
            if keep > 0 {
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(&mut *tx).await?;
                let overflow = total - keep as i64;
                if overflow > 0 {
                    let ids: Vec<String> = sqlx::query_scalar(
                        "SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT ?1",
                    )
                    .bind(overflow)
                    .fetch_all(&mut *tx)
                    .await?;
                    deleted += delete_matching(&mut tx, &ids).await?;
                }
            }
            tx.commit().await?;
        }
        if max_size_mb > 0 {
            deleted += shrink_to_size(&self.pool, max_size_mb as i64 * 1024 * 1024).await?;
        }
        Ok(deleted as usize)
    }
}

/// `note_ids`にマッチするノートと、その関連テーブル・column_noteを削除する
/// (FK制約は張っていないため手動カスケード)。削除によって影響を受けたカラムのbackfill境界
/// (column_fetch_boundary)も、生存している最古ノートIDまで引き上げる(Issue #228)。
///
/// Issue #115: 旧実装は`CREATE TEMP TABLE prune_ids AS ...`で対象IDを一時テーブルに
/// 溜めていたが、`sqlx::SqlitePool`は複数コネクションを持ちうる(このプールは`max_connections(1)`
/// だが、将来Postgres/MySQLでも同じロジックを使い回せるようにする)ため、対象IDを
/// Rust側の`Vec<String>`として先に確定させ、`IN (...)`にバインドする方式に変更する。
async fn delete_matching(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, note_ids: &[String]) -> Result<i64> {
    if note_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = note_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let affected_columns: Vec<String> = {
        let sql = format!("SELECT DISTINCT column_id FROM column_note WHERE note_id IN ({placeholders})");
        let mut q = sqlx::query_scalar(&sql);
        for id in note_ids {
            q = q.bind(id);
        }
        q.fetch_all(&mut **tx).await?
    };
    let max_deleted_by_column: std::collections::HashMap<String, String> = {
        let sql = format!(
            "SELECT column_id, MAX(note_id) FROM column_note WHERE note_id IN ({placeholders}) GROUP BY column_id"
        );
        let mut q = sqlx::query_as::<_, (String, String)>(&sql);
        for id in note_ids {
            q = q.bind(id);
        }
        q.fetch_all(&mut **tx).await?.into_iter().collect()
    };

    let del_sql = format!("DELETE FROM note WHERE id IN ({placeholders})");
    let mut q = sqlx::query(&del_sql);
    for id in note_ids {
        q = q.bind(id);
    }
    let deleted = q.execute(&mut **tx).await?.rows_affected() as i64;

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        let sql = format!("DELETE FROM {table} WHERE note_id IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for id in note_ids {
            q = q.bind(id);
        }
        q.execute(&mut **tx).await?;
    }

    for column_id in &affected_columns {
        let survivor: Option<String> =
            sqlx::query_scalar("SELECT MIN(note_id) FROM column_note WHERE column_id = ?1")
                .bind(column_id)
                .fetch_one(&mut **tx)
                .await?;
        match survivor {
            Some(oldest) => {
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                sqlx::query(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = ?2
                     WHERE column_id = ?1 AND oldest_fetched_id < ?2",
                )
                .bind(column_id)
                .bind(candidate)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = ?1")
                    .bind(column_id)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(deleted)
}

async fn db_size_bytes(pool: &SqlitePool) -> Result<i64> {
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count").fetch_one(pool).await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size").fetch_one(pool).await?;
    Ok(page_count * page_size)
}

async fn shrink_to_size(pool: &SqlitePool, budget_bytes: i64) -> Result<i64> {
    let mut deleted = 0i64;
    for _ in 0..3 {
        sqlx::query("PRAGMA incremental_vacuum").execute(pool).await?;
        let size = db_size_bytes(pool).await?;
        if size <= budget_bytes {
            break;
        }
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(pool).await?;
        if total == 0 {
            break;
        }
        let over_ratio = (size - budget_bytes) as f64 / size as f64;
        let to_delete = ((total as f64) * over_ratio).ceil() as i64;
        let to_delete = to_delete.clamp(1, total);
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT ?1")
                .bind(to_delete)
                .fetch_all(pool)
                .await?;
        let mut tx = pool.begin().await?;
        deleted += delete_matching(&mut tx, &ids).await?;
        tx.commit().await?;
    }
    Ok(deleted)
}
```

- [ ] **Step 6: `search_cache`を書く(設計書どおり、生SQL・`?`プレースホルダのまま変更しない)**

```rust
impl NoteCacheStore {
    pub async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        use crate::filter::sql::SqlParam;

        let mut sql = String::from(
            "SELECT n.id, n.payload FROM note n JOIN user u ON u.id = n.user_id WHERE (",
        );
        sql.push_str(&where_sql.sql);
        sql.push(')');
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
        let rows = query.fetch_all(&self.pool).await?;
        resolve_payload_rows(&self.pool, rows).await
    }
}
```

- [ ] **Step 7: `#[cfg(test)] mod tests`を`#[tokio::test]`化する**

既存の全テスト(約30件)を以下の変換ルールで書き換える:
- `fn store() -> NoteCacheStore` → `async fn store() -> NoteCacheStore { NoteCacheStore::new(open_cache_in_memory().await.unwrap()) }`
- 各`#[test] fn test_name() { let s = store(); ... }` → `#[tokio::test] async fn test_name() { let s = store().await; ... s.cache_notes(...).await.unwrap() ... }`(メソッド呼び出しすべてに`.await`を追加)
- テスト内で`s.conn.lock().unwrap()`を使って直接SQLを検証している箇所(例: `upsert_note_stores_stubbed_user_in_payload`, `upsert_replaces_and_relations_not_duplicated`, `normalized_columns_populated_for_nql`など)は、`s.pool`をpublicにはせず、`sqlx::query_scalar("...").bind(...).fetch_one(&s.pool).await.unwrap()`のように直接プールへ問い合わせる形に書き換える。そのため`NoteCacheStore`にテスト専用の`#[cfg(test)] pub(crate) fn pool(&self) -> &SqlitePool { &self.pool }`アクセサを追加してよい
- `insert_legacy_row`(旧形式行を素のSQLで作るテストヘルパー)も`async fn`化し、`sqlx::query(...).execute(&s.pool()).await.unwrap()`に置き換える

代表例(`upsert_note_stores_stubbed_user_in_payload`):

```rust
    #[tokio::test]
    async fn upsert_note_stores_stubbed_user_in_payload() {
        let s = store().await;
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        let raw_payload: String = sqlx::query_scalar("SELECT payload FROM note WHERE id = 'n1'")
            .fetch_one(s.pool())
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw_payload).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u1" }));
    }
```

残りのテストも同じ変換パターンを機械的に適用する。

- [ ] **Step 8: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::note_cache
```

Expected: PASS(全テスト。追加で`upsert_replaces_and_relations_not_duplicated`が「DELETE→INSERT」ではなく「UPSERT」に変わったことを検証する意味も持つようになる)

- [ ] **Step 9: コミット**

```bash
git add src-tauri/src/store/note_cache.rs
git commit -m "note_cacheをsqlx非同期に移行し側テーブルをUPSERT化・pruneをTEMP TABLE無しに変更"
```

---

### Task 4: `NoteCacheBackend`トレイト抽出 + 呼び出し元の`.await`追加

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`(トレイト抽出)
- Modify: `src-tauri/src/store/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/commands/mute.rs`
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/stream/connection.rs`
- Modify: `src-tauri/src/commands/draft.rs`(テストの`AppState::new_for_test`呼び出しに`.await`を追加)

**Interfaces:**
- Consumes: Task 3で完成した`NoteCacheStore`の全async メソッド
- Produces:
  - `pub(crate) trait NoteCacheBackend: Send + Sync`(Task 3の`NoteCacheStore`が持っていた全publicメソッドと同じシグネチャをasync traitメソッドとして持つ)
  - `pub(crate) struct SqliteBackend`(Task 3の`NoteCacheStore`の中身をリネームしたもの。`impl NoteCacheBackend for SqliteBackend`)
  - `pub struct NoteCacheStore { backend: Box<dyn NoteCacheBackend> }`(薄い委譲ラッパー)
  - `AppState::new_for_test`が`pub(crate) async fn`になる

- [ ] **Step 1: トレイトを定義し、`SqliteBackend`へリネームする**

`note_cache.rs`の先頭に`async-trait`を使ったトレイト定義を追加し、Task 3で書いた`impl NoteCacheStore { ... }`ブロックの型名を`SqliteBackend`にリネーム、各メソッド定義に`#[async_trait::async_trait] impl NoteCacheBackend for SqliteBackend { ... }`を被せる:

```rust
#[async_trait::async_trait]
pub(crate) trait NoteCacheBackend: Send + Sync {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()>;
    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()>;
    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>>;
    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>>;
    async fn get_note(&self, note_id: &str) -> Result<Option<Note>>;
    async fn update_note(&self, note: &Note) -> Result<()>;
    async fn clear_column_notes(&self, column_id: &str) -> Result<()>;
    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>>;
    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;
    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;
    async fn clear_all_fetch_boundaries(&self) -> Result<()>;
    async fn note_count(&self) -> Result<i32>;
    async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32>;
    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize>;
    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>>;
}

pub(crate) struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl NoteCacheBackend for SqliteBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        // Task 3で書いた本体をそのまま移す
        ...
    }
    // 以下、Task 3の各メソッドをそのまま移す(cache_note含む)
}
```

`NoteCacheStore`を薄い委譲ラッパーへ書き換える:

```rust
pub struct NoteCacheStore {
    backend: Box<dyn NoteCacheBackend>,
}

impl NoteCacheStore {
    pub fn new(backend: impl NoteCacheBackend + 'static) -> Self {
        Self { backend: Box::new(backend) }
    }

    pub async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        self.backend.cache_notes(column_id, notes).await
    }
    pub async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.backend.cache_note(column_id, note).await
    }
    pub async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        self.backend.load_cached(column_id, limit).await
    }
    pub async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        self.backend.load_cached_before(column_id, until_id, limit).await
    }
    pub async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        self.backend.get_note(note_id).await
    }
    pub async fn update_note(&self, note: &Note) -> Result<()> {
        self.backend.update_note(note).await
    }
    pub async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        self.backend.clear_column_notes(column_id).await
    }
    pub async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        self.backend.get_fetch_boundary(column_id).await
    }
    pub async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.backend.set_fetch_boundary(column_id, new_oldest_id).await
    }
    pub async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.backend.extend_fetch_boundary(column_id, new_oldest_id).await
    }
    pub async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        self.backend.clear_all_fetch_boundaries().await
    }
    pub async fn note_count(&self) -> Result<i32> {
        self.backend.note_count().await
    }
    pub async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        self.backend.notes_since(since_epoch_secs).await
    }
    pub async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        self.backend.prune(keep, max_age_days, max_size_mb).await
    }
    pub async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        self.backend.search_cache(where_sql, until_id, limit).await
    }
}
```

`#[cfg(test)] mod tests`内の`store()`ヘルパーは`NoteCacheStore::new(SqliteBackend::new(open_cache_in_memory().await.unwrap()))`に、`s.pool()`は`s.backend`の型を`SqliteBackend`と分かっている前提でテストからは使えなくなるため、`NoteCacheStore`にも`#[cfg(test)] pub(crate) fn sqlite_pool(&self) -> &SqlitePool`のようなdowncast用アクセサを用意するか、あるいはテストは`SqliteBackend`を直接使う(`NoteCacheStore`でラップしない)ように書き換える。後者の方が単純なので、`note_cache.rs`内の`#[cfg(test)] mod tests`は`SqliteBackend`を直接テスト対象にする(`NoteCacheStore`の委譲は別途、後述のcall-site統合テストで十分カバーされる)。

- [ ] **Step 2: `store/mod.rs`のエクスポートを確認・更新する**

```bash
grep -n "NoteCacheStore\|SqliteBackend\|NoteCacheBackend" src-tauri/src/store/mod.rs
```

`pub use note_cache::NoteCacheStore;`のみ公開されていることを確認する(`SqliteBackend`/`NoteCacheBackend`は`pub(crate)`のままで外部非公開)。

- [ ] **Step 3: `lib.rs`の初期化コードを更新する**

`src-tauri/src/lib.rs:233-235`付近:

```rust
// 変更前
let cache_conn =
    db::open_cache(&cache_dir.join("cache.db")).expect("failed to open cache db");
let cache = NoteCacheStore::new(cache_conn);
app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));
```

```rust
// 変更後
let cache_pool = tauri::async_runtime::block_on(db::open_cache(&cache_dir.join("cache.db")))
    .expect("failed to open cache db");
let cache = NoteCacheStore::new(store::SqliteBackend::new(cache_pool));
app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache));
```

このブロックは`tauri::Builder::setup`のクロージャ内(同期コンテキスト)にあるため、`tauri::async_runtime::block_on`でasync呼び出しをブロッキング実行する(Tauriのsetupフックが提供する標準パターン)。`store::SqliteBackend`を`lib.rs`から参照できるよう、`store/mod.rs`で`pub(crate) use note_cache::SqliteBackend;`を追加する。

- [ ] **Step 4: `state.rs`の`new_for_test`を`async fn`にする**

```rust
    #[cfg(test)]
    pub(crate) async fn new_for_test(settings: SettingsStore) -> Self {
        let pool = crate::store::db::open_cache_in_memory().await.unwrap();
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(pool));
        Self::new_with_sound(
            Box::new(crate::session::MemoryStore::default()),
            settings,
            DraftStore::new_in_memory(),
            cache,
            SoundPlayer::new_for_test(),
        )
    }
```

- [ ] **Step 5: `AppState::new_for_test`呼び出し元に`.await`を追加する**

```bash
grep -rn "AppState::new_for_test" src-tauri/src
```

出力された各呼び出し箇所(`commands/draft.rs`, `commands/mute.rs`, `stream/connection.rs`)について:
1. 呼び出し箇所を含む`#[test] fn`が`#[tokio::test] async fn`になっていることを確認し、なっていなければ変更する
2. `AppState::new_for_test(settings)` → `AppState::new_for_test(settings).await`に変更する

- [ ] **Step 6: `commands/column.rs`のcall siteを更新する**

`grep -n "state\.cache\." src-tauri/src/commands/column.rs`の各行に`.await`を追加する(対象行: 243, 290, 321, 365, 372, 401, 434, 447, 510, 755, 758, 970, 994, 1208)。これらは既にすべて`async fn`内にあるため、`?`演算子を使っている箇所は`.await?`に、`.ok()`/`let _ =`パターンは`.await`のみ追加する。

`cache_with`テストヘルパー(1310行目付近)を更新する:

```rust
    async fn cache_with(notes: &[Note]) -> NoteCacheStore {
        let store = NoteCacheStore::new(
            crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().await.unwrap()),
        );
        store.cache_notes("col1", notes).await.unwrap();
        store
    }
```

呼び出し元のテスト関数も`#[tokio::test] async fn`化し、`cache_with(...)`呼び出しに`.await`を追加する。

- [ ] **Step 7: `commands/mute.rs`のcall siteを更新する**

34行目: `let _ = state.cache.clear_all_fetch_boundaries();` → `let _ = state.cache.clear_all_fetch_boundaries().await;`

- [ ] **Step 8: `commands/note.rs`のcall siteを更新する**

74, 76, 91, 93行目の`state.cache.get_note(...)`/`state.cache.update_note(...)`に`.await`を追加する。`if let Ok(Some(mut note)) = state.cache.get_note(&note_id) {`のような`if let`パターンは`if let Ok(Some(mut note)) = state.cache.get_note(&note_id).await {`になる。

- [ ] **Step 9: `stream/connection.rs`のcall siteを更新する**

762, 923, 931行目(プロダクションコード)に`.await`を追加する。1204, 1207, 1212, 1222行目(テストコード)は該当テスト関数を`#[tokio::test] async fn`化した上で`.await`を追加する。

- [ ] **Step 10: 全体テストを実行する**

```bash
cd src-tauri && cargo test
```

Expected: PASS(全テスト)。コンパイルエラーが出た場合は、上記のcall site一覧に漏れがないか`grep -rn "\.cache\.\w" src-tauri/src`で再確認する。

- [ ] **Step 11: `cargo tauri dev`で実機動作を確認する**

リポジトリルートから`cargo tauri dev`を起動し、以下を手動確認する:
- カラムを開いてノートが表示される(`load_cached`/REST取得後の`cache_notes`)
- 新着ノートがStreaming経由でリアルタイム表示される(`cache_note`)
- リアクションをつけて表示に反映される(`update_note`)
- Backstage(設定画面)のキャッシュ件数表示が正しく出る(`note_count`)
- 検証後は自分で起動したdevサーバを終了する

- [ ] **Step 12: コミット**

```bash
git add -A
git commit -m "NoteCacheBackendトレイトを抽出しSqliteBackendへ切り出し、呼び出し元をasync化"
```

---

## 完了条件

- `cd src-tauri && cargo test`が全件green
- `cd frontend && pnpm check`が影響を受けないこと(フロントエンドは今回のスコープ外、コマンドのRust側シグネチャ・TS bindingsは変わらないはず)を確認する
- `cargo tauri dev`での手動確認(Task 4 Step 11)が完了している
- Phase 2(PostgreSQL対応・sea-query導入・設定UI)は別計画として、本Phase完了後にあらためて`writing-plans`で作成する
