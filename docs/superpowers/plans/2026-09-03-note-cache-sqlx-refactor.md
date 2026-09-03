# note cache: 非同期化 + 側テーブルUPSERT化 (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** note cache(`store/db.rs`のキャッシュスキーマ, `store/note_cache.rs`, `store/user_ref.rs`)を`NoteCacheBackend`トレイト越しの非同期APIへ移行し、外部から見た挙動(呼び出し元API・クエリ結果)を一切変えずに、Phase 2(PostgreSQL対応・`sqlx`+`sea-query`導入)・Phase 3(MySQL対応)へ進めるための土台を作る。

**Architecture:** `rusqlite`(bundled)は`sqlx`のSQLiteドライバと同じネイティブライブラリ`sqlite3`にリンクするため同一Cargo依存グラフに共存できない(実装検証済み。設計書「DBアクセス手段」参照)。account/column設定側が今後も`rusqlite`を使い続ける以上、Phase 1では`sqlx`を導入せず、既存の`rusqlite::Connection`をそのまま使い続ける。非同期化は`tauri::async_runtime::spawn_blocking`(このコードベースで`commands/note.rs`/`commands/mute.rs`が既に使っている確立パターン)で同期のrusqlite呼び出しを包む方式で実現する。`sqlx`+`sea-query`はPhase 2(PostgreSQL、`libsqlite3-sys`非依存)で初めて導入する。

**Tech Stack:** Rust, `rusqlite`(既存、変更なし), `async-trait`, `tokio`/`tauri::async_runtime`(既存)。`sqlx`・`sea-query`はPhase 2以降(本計画のスコープ外)。

## Global Constraints

- 外部から見た挙動は不変であること。SQL文字列・クエリロジックは下記の意図的変更(UNIQUE制約追加、側テーブルのUPSERT化)を除き現状と同一
- 各タスクの最後に `cd src-tauri && cargo test` が green であることを確認してからコミットする
- コミットメッセージは日本語の1行のみ(このリポジトリの規約)
- `spawn_blocking`クロージャは`'static`である必要があるため、借用引数(`&str`/`&[Note]`/`&SqlWhere`等)は呼び出し前に所有値へ変換する(`to_string()`/`to_vec()`/`clone()`)。`SqlWhere`は`Clone`をderiveしていないため、フィールドごとに複製する: `crate::filter::sql::SqlWhere { sql: where_sql.sql.clone(), params: where_sql.params.clone() }`(`SqlParam`は`Clone`実装済み)
- `spawn_blocking`は`tokio::task::JoinError`を返しうる。`.await`の結果を`.map_err(|e| crate::error::Error::Db(format!("cache task join error: {e}")))`で`Error`へマッピングしてから、クロージャ内の`Result<T>`を`?`で展開する(`commands/note.rs::read_clipboard_image`と同型のパターン: `spawn_blocking(...).await.map_err(...)？？`)
- `std::sync::Mutex`のロックガードは`spawn_blocking`クロージャ内で取得・使用・破棄が完結する(`.await`をまたがない)ため、そのまま使ってよい(`tokio::sync::Mutex`への切替は不要)

---

### Task 1: `store/db.rs` + `store/note_cache.rs` — 側テーブルUNIQUE制約 + UPSERT化

**方針**: このタスクは既存の同期API(シグネチャ)を一切変えない、純粋な内部実装の改善。`migrate_cache(conn: &Connection)`・`upsert_note(conn: &Connection, n: &Note) -> Result<()>`とも、引数・戻り値の型は現状のまま。呼び出し元(`NoteCacheStore`の各メソッド)からは何も変わって見えない。async化・トレイト抽出はTask 2で行う。

**Files:**
- Modify: `src-tauri/src/store/db.rs`(`migrate_cache`にUNIQUE制約+dedupマイグレーションを追加)
- Modify: `src-tauri/src/store/note_cache.rs`(`upsert_note`の側テーブル書き込みをDELETE+INSERTからUPSERT+失効行クリーンアップへ変更)

**Interfaces:**
- Consumes: なし
- Produces: `migrate_cache`/`upsert_note`とも既存と同じシグネチャのまま、内部動作のみ変更(Task 2から利用される)

- [ ] **Step 1: `db.rs`に新しいテストを追加する(TDD)。既存テストは変更しない**

`src-tauri/src/store/db.rs`の`#[cfg(test)] mod tests`の末尾に以下のテストを追加する:

```rust
    /// Issue #115: 側テーブルに重複行があっても、UNIQUEインデックス作成前に
    /// 重複排除してからインデックスを張ること(既存の蓄積データを壊さずに移行できること)。
    #[test]
    fn migrate_cache_dedupes_side_tables_before_creating_unique_index() {
        let conn = open_cache_in_memory().unwrap();
        // 正規のパスでは起きないはずの重複行を素のSQLで作る(移行前の実データを模倣)。
        conn.execute(
            "INSERT INTO note (id, created_at, visibility, user_id, payload) VALUES ('n1', 100, 'home', 'u1', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ('n1', '👍', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ('n1', '👍', 1)",
            [],
        )
        .unwrap();

        // open_cache_in_memory は migrate_cache 込みなので、この時点で既にインデックスは張られているはず。
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_nr_unique'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1);

        // 重複行は1件に集約されていること。
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // 冪等: 再実行してもエラーにならない(UNIQUE違反にならない)。
        migrate_cache(&conn).unwrap();
    }
```

これは`open_cache_in_memory`が既に`migrate_cache`を通した状態で作られる前提なので、テスト内であえて重複行を素のSQLで挿入し、その後手動で`migrate_cache(&conn)`を再度呼んで冪等性を検証する形になる。

- [ ] **Step 2: テストを実行して失敗を確認する**

```bash
cd src-tauri && cargo test --lib store::db::tests::migrate_cache_dedupes_side_tables_before_creating_unique_index
```

Expected: FAIL(`idx_nr_unique`が存在しない、または重複行が1件に集約されていないため)

- [ ] **Step 3: `migrate_cache`にUNIQUE制約マイグレーションを追加する**

`src-tauri/src/store/db.rs`の`migrate_cache`関数の末尾(`idx_cn_column_created`のインデックス作成の後)に以下を追加する:

```rust
    // Issue #115: 側テーブルへのUNIQUE制約追加(重複排除してからインデックス作成)。
    // 既存の蓄積データ(note/user/column_note等)は一切触らない。単一プロセス+単一SQLite接続
    // (Mutexで直列化)前提の現行コードでは重複行はほぼ存在しないはずだが、念のため
    // インデックス作成前に重複排除する(Issue #115 spec「既存キャッシュDBへのUNIQUE制約追加について」)。
    add_unique_index_with_dedup(conn, "note_reaction", &["note_id", "emoji_key"], "idx_nr_unique")?;
    add_unique_index_with_dedup(conn, "note_tag", &["note_id", "tag"], "idx_nt_unique")?;
    add_unique_index_with_dedup(conn, "note_mention", &["note_id", "user_id"], "idx_nm_unique")?;
    add_unique_index_with_dedup(conn, "note_emoji", &["note_id", "emoji"], "idx_ne_unique")?;
    add_unique_index_with_dedup(
        conn,
        "note_file",
        &["note_id", "mime_type", "mime_category", "is_sensitive"],
        "idx_nf_unique",
    )?;
    Ok(())
}

/// `table`に`(cols...)`のUNIQUEインデックス`index_name`が無ければ、重複行を
/// (rowidが最小の1行だけ残して)削除してからインデックスを作成する。
/// 既にインデックスがあれば何もしない(起動のたびに全表走査しないため)。
fn add_unique_index_with_dedup(
    conn: &Connection,
    table: &str,
    cols: &[&str],
    index_name: &str,
) -> Result<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
        params![index_name],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(());
    }
    let col_list = cols.join(", ");
    conn.execute(
        &format!("DELETE FROM {table} WHERE rowid NOT IN (SELECT MIN(rowid) FROM {table} GROUP BY {col_list})"),
        [],
    )?;
    conn.execute(&format!("CREATE UNIQUE INDEX {index_name} ON {table}({col_list})"), [])?;
    Ok(())
}
```

(`migrate_cache`の元の末尾が`Ok(())`のみだった場合、上記コードの`Ok(())`と関数の閉じ`}`が重複しないよう、既存の`Ok(())`を書き換える形で統合すること。`add_unique_index_with_dedup`は`migrate_cache`とは別の新規private関数として追加する)

- [ ] **Step 4: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::db
```

Expected: PASS(既存6テスト + 新規1テストの計7テスト)

- [ ] **Step 5: `note_cache.rs`に新しいテストを追加する(TDD)。既存テストは変更しない**

`src-tauri/src/store/note_cache.rs`の`#[cfg(test)] mod tests`の末尾に以下のテストを追加する(リアクションが取り消された場合に、UPSERT化後も正しく`note_reaction`から消えることを検証する):

```rust
    /// Issue #115: 側テーブルをDELETE+INSERTからUPSERTに変更した後も、
    /// 「今のnoteに無くなった行(取り消されたリアクション等)」は正しく消えること。
    #[test]
    fn upsert_note_removes_stale_reaction_after_unreact() {
        let s = store();
        let mut n = note("n1", 100);
        n.reactions = HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).unwrap();

        // リアクションが取り消された(reactionsが空になった)状態で再受信
        n.reactions = HashMap::new();
        n.reaction_count = 0;
        n.my_reaction = None;
        s.update_note(&n).unwrap();

        let conn = s.conn.lock().unwrap();
        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rc, 0, "取り消されたリアクションの行はUPSERT化後も削除されること");
    }

    /// Issue #115: 同じリアクションを再受信してもcountが正しく更新される(UPSERTのON CONFLICT DO UPDATEが効いていること)。
    #[test]
    fn upsert_note_updates_reaction_count_on_upsert() {
        let s = store();
        let mut n = note("n1", 100);
        n.reactions = HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).unwrap();

        n.reactions = HashMap::from([("👍".into(), 5u32)]);
        s.update_note(&n).unwrap();

        let conn = s.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT count FROM note_reaction WHERE note_id='n1' AND emoji_key='👍'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }
```

既存の`upsert_replaces_and_relations_not_duplicated`テスト(同じノートを2回`cache_note`しても`note_reaction`が重複しないことを検証)はそのまま残る。UPSERT化後もこのテストの意味(重複しない)は成り立つので変更不要。

- [ ] **Step 6: テストを実行して失敗を確認する**

```bash
cd src-tauri && cargo test --lib store::note_cache::tests::upsert_note_removes_stale_reaction_after_unreact
cd src-tauri && cargo test --lib store::note_cache::tests::upsert_note_updates_reaction_count_on_upsert
```

Expected: `upsert_note_removes_stale_reaction_after_unreact`はPASS(現行のDELETE+INSERT方式でも成立する。UPSERT化後も壊れないことを確認する回帰テストとして先に追加する)。`upsert_note_updates_reaction_count_on_upsert`もPASS(DELETE+INSERT方式でも成立)。**この2つは現行実装でも通る想定**なので、Step 7実装後に再度流して「UPSERT化しても壊れていないこと」を確認する用途のテストである(RED/GREENの意味が薄いが、UPSERT化の回帰防止として重要なので先に書く)。

- [ ] **Step 7: `upsert_note`の側テーブル書き込みをUPSERT化する**

`src-tauri/src/store/note_cache.rs`の`upsert_note`関数内、以下の箇所:

```rust
    // 関連テーブルは入れ替え
    for table in ["note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        conn.execute(&format!("DELETE FROM {table} WHERE note_id = ?1"), params![n.id])?;
    }
    for (emoji, count) in &n.reactions {
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?1, ?2, ?3)",
            params![n.id, emoji, count],
        )?;
    }
    for tag in &n.tags {
        conn.execute("INSERT INTO note_tag (note_id, tag) VALUES (?1, ?2)", params![n.id, tag])?;
    }
    for uid in &n.mentions {
        conn.execute("INSERT INTO note_mention (note_id, user_id) VALUES (?1, ?2)", params![n.id, uid])?;
    }
    for e in n.emojis.keys() {
        conn.execute("INSERT INTO note_emoji (note_id, emoji) VALUES (?1, ?2)", params![n.id, e])?;
    }
    for f in &n.files {
        conn.execute(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?1, ?2, ?3, ?4)",
            params![n.id, f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64],
        )?;
    }
    Ok(())
}
```

を、以下に置き換える:

```rust
    // 関連テーブルはUPSERT(Task 1で追加したUNIQUE制約に基づく)。DELETE+INSERTではなく
    // ON CONFLICTで書き換えることで、複数プロセスからの同時書き込みでも一時的な重複行・
    // 空状態が起きないようにする(Phase 2以降の外部DB利用を見据えた変更、Issue #115)。
    for (emoji, count) in &n.reactions {
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?1, ?2, ?3)
             ON CONFLICT(note_id, emoji_key) DO UPDATE SET count = excluded.count",
            params![n.id, emoji, count],
        )?;
    }
    for tag in &n.tags {
        conn.execute(
            "INSERT INTO note_tag (note_id, tag) VALUES (?1, ?2) ON CONFLICT(note_id, tag) DO NOTHING",
            params![n.id, tag],
        )?;
    }
    for uid in &n.mentions {
        conn.execute(
            "INSERT INTO note_mention (note_id, user_id) VALUES (?1, ?2) ON CONFLICT(note_id, user_id) DO NOTHING",
            params![n.id, uid],
        )?;
    }
    for e in n.emojis.keys() {
        conn.execute(
            "INSERT INTO note_emoji (note_id, emoji) VALUES (?1, ?2) ON CONFLICT(note_id, emoji) DO NOTHING",
            params![n.id, e],
        )?;
    }
    for f in &n.files {
        conn.execute(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(note_id, mime_type, mime_category, is_sensitive) DO NOTHING",
            params![n.id, f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64],
        )?;
    }

    // 旧行(現在のnoteの内容に含まれなくなったreaction/tag/mention/emoji/file)を掃除する。
    // 例: リアクションが取り消された、タグが編集で消えた、等。json_eachはSQLiteのJSON1拡張
    // (rusqliteのbundled機能で有効)を使う。
    conn.execute(
        "DELETE FROM note_reaction WHERE note_id = ?1 AND emoji_key NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.reactions.keys().collect::<Vec<_>>())?],
    )?;
    conn.execute(
        "DELETE FROM note_tag WHERE note_id = ?1 AND tag NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.tags)?],
    )?;
    conn.execute(
        "DELETE FROM note_mention WHERE note_id = ?1 AND user_id NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.mentions)?],
    )?;
    conn.execute(
        "DELETE FROM note_emoji WHERE note_id = ?1 AND emoji NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.emojis.keys().collect::<Vec<_>>())?],
    )?;
    // note_fileはUNIQUEキーが複合(4列)でjson_eachのタプル比較ができないため、行ごとに比較する。
    let current_file_keys: Vec<String> = n
        .files
        .iter()
        .map(|f| format!("{}\u{0}{}\u{0}{}", f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64))
        .collect();
    let mut stmt = conn.prepare("SELECT rowid, mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = ?1")?;
    let existing_files: Vec<(i64, String, String, i64)> = stmt
        .query_map(params![n.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for (rowid, mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = format!("{mime_type}\u{0}{mime_category_val}\u{0}{is_sensitive}");
        if !current_file_keys.contains(&key) {
            conn.execute("DELETE FROM note_file WHERE rowid = ?1", params![rowid])?;
        }
    }
    Ok(())
}
```

- [ ] **Step 8: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::note_cache
cd src-tauri && cargo test --lib store::db
```

Expected: PASS(既存の全テスト + Step 5で追加した2テストが揃ってPASSする)

- [ ] **Step 9: クレート全体のテストを実行する**

```bash
cd src-tauri && cargo test
```

Expected: 全件PASS(このタスクは`migrate_cache`/`upsert_note`のシグネチャを変えていないため、他の呼び出し元は無変更のまま動く)

- [ ] **Step 10: コミット**

```bash
git add src-tauri/src/store/db.rs src-tauri/src/store/note_cache.rs
git commit -m "note cacheの側テーブルにUNIQUE制約を追加しDELETE+INSERTをUPSERTに変更"
```

---

### Task 2: `NoteCacheBackend`トレイト抽出 + `spawn_blocking`による非同期化 + 呼び出し元の`.await`追加

**方針**: このタスクだけがクレート全体を一括で切り替える(実装中は一時的にコンパイル不能になってよいが、タスクの最後には`cargo test`がgreenになること)。新規ファイル`store/sqlite_backend.rs`に`SqliteBackend`(`Arc<Mutex<Connection>>`を持つ)を作り、Task 1で更新済みの`note_cache.rs`の各メソッド本体を`spawn_blocking`クロージャで包んで移す。`note_cache.rs`はトレイト定義+薄い委譲ラッパーへ全面書き換えする。

**Files:**
- Create: `src-tauri/src/store/sqlite_backend.rs`
- Modify: `src-tauri/src/store/note_cache.rs`(全面書き換え: トレイト定義+薄い委譲ラッパー)
- Modify: `src-tauri/src/store/mod.rs`(`mod sqlite_backend;`登録)
- Modify: `src-tauri/src/commands/column.rs`(call site + テストヘルパー更新)
- Modify: `src-tauri/src/commands/mute.rs`(call site更新)
- Modify: `src-tauri/src/commands/note.rs`(call site更新)
- Modify: `src-tauri/src/stream/connection.rs`(call site + テスト更新)
- Modify: `src-tauri/Cargo.toml`(`async-trait`を追加)

**Interfaces:**
- Consumes: Task 1で更新済みの`upsert_note`/`migrate_cache`(内部動作)、既存の`db::open_cache`/`open_cache_in_memory`(シグネチャ変更なし、`rusqlite::Connection`を返す同期関数のまま)
- Produces:
  - `pub(crate) trait NoteCacheBackend: Send + Sync`(async-trait)
  - `pub(crate) struct SqliteBackend { conn: Arc<Mutex<Connection>> }`、`impl NoteCacheBackend for SqliteBackend`
  - `pub struct NoteCacheStore { backend: Box<dyn NoteCacheBackend> }`(薄い委譲ラッパー、全メソッドasync)

- [ ] **Step 1: 依存クレートを追加する**

```bash
cd src-tauri
cargo add async-trait
```

- [ ] **Step 2: `store/sqlite_backend.rs`を新規作成し、`SqliteBackend`とトレイトを定義する**

`src-tauri/src/store/mod.rs`に`mod sqlite_backend;`と`pub(crate) use sqlite_backend::SqliteBackend;`を追加する。

新規ファイル`src-tauri/src/store/sqlite_backend.rs`を以下の内容で作成する(トレイト定義は`note_cache.rs`側に置くため、ここでは`use crate::store::note_cache::NoteCacheBackend;`で参照する):

```rust
//! note cacheのSqliteBackend(Issue #115 Phase 1)。既存の`note_cache.rs`のロジックを
//! `NoteCacheBackend`トレイトの非同期メソッドとして提供する。rusqliteの同期呼び出しを
//! `tauri::async_runtime::spawn_blocking`で包む(rusqliteとsqlxのSQLiteドライバは
//! `libsqlite3-sys`のバージョン要求が競合し共存できないため、Phase 1ではsqlxを使わない。
//! 設計書「DBアクセス手段」参照)。

use crate::domain::Note;
use crate::error::{Error, Result};
use crate::store::note_cache::NoteCacheBackend;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub(crate) struct SqliteBackend {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteBackend {
    pub(crate) fn new(conn: Connection) -> Self {
        Self { conn: Arc::new(Mutex::new(conn)) }
    }

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }
}

/// `spawn_blocking`のJoinErrorを`Error`へマッピングする共通ヘルパー。
fn map_join_error(e: tokio::task::JoinError) -> Error {
    Error::Db(format!("cache task join error: {e}"))
}
```

- [ ] **Step 3: `note_cache.rs`を全面書き換えし、トレイト定義+`SqliteBackend`の各メソッド実装+薄い委譲ラッパーにする**

まず`src-tauri/src/store/note_cache.rs`のテストを、既存の同期テストと同じ検証内容を持つ非同期版として書き直す準備をする。既存の`#[cfg(test)] mod tests`はいったん全て`sqlite_backend.rs`側の新しい`#[cfg(test)] mod tests`に移植することになる(後述Step 5)ので、まず本体の書き換えから進める。

`note_cache.rs`の中身(構造体定義・`impl`ブロック・private関数群)を以下に置き換える。まずファイル冒頭の非DB系private関数(`now_epoch`/`visibility_str`/`mime_category`/`has_url`)と`upsert_note`/`resolve_payload_rows`/`self_heal_legacy_row`/`self_heal_node`/`delete_matching`/`db_size_bytes`/`shrink_to_size`はそのまま残す(Task 1で更新済みの`upsert_note`を含め、シグネチャは`&Connection`を取る同期関数のまま変更しない)。`NoteCacheStore`構造体・`impl NoteCacheStore { ... }`ブロックだけを以下のトレイト定義+`SqliteBackend`実装+薄いラッパーに置き換える:

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

/// note cacheの公開API。内部の`NoteCacheBackend`実装(`SqliteBackend`、将来的な
/// `PostgresBackend`/`MySqlBackend`)へ委譲する薄いラッパー。
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

- [ ] **Step 4: `SqliteBackend`に`NoteCacheBackend`を実装する(`spawn_blocking`で包む)**

`src-tauri/src/store/sqlite_backend.rs`に以下を追記する。各メソッドは「引数を所有値へ変換→`Arc`をクローン→`spawn_blocking`→中で既存の同期ロジック(`note_cache.rs`のprivate関数を呼ぶ、またはインラインで書く)を実行」という同じ形になる。代表として`cache_notes`/`load_cached`/`get_note`/`prune`/`search_cache`を示す。残りの`cache_note`/`load_cached_before`/`update_note`/`clear_column_notes`/`get_fetch_boundary`/`set_fetch_boundary`/`extend_fetch_boundary`/`clear_all_fetch_boundaries`/`note_count`/`notes_since`は、現行`NoteCacheStore`(旧実装、書き換え前の`note_cache.rs`)の対応メソッドの中身をそのまま`spawn_blocking`クロージャに移すだけなので、同じパターンを機械的に適用する:

```rust
#[async_trait::async_trait]
impl NoteCacheBackend for SqliteBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let notes = notes.to_vec();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            if notes.is_empty() {
                return Ok(());
            }
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            let now = crate::store::note_cache::now_epoch();
            for n in &notes {
                crate::store::note_cache::upsert_note(&tx, n)?;
                tx.execute(
                    "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![column_id, n.id, now, n.created_at],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
            let guard = conn.lock().unwrap();
            let mut stmt = guard.prepare(
                "SELECT n.id, n.payload FROM column_note cn
                 JOIN note n ON n.id = cn.note_id
                 WHERE cn.column_id = ?1
                 ORDER BY cn.created_at DESC, cn.note_id DESC
                 LIMIT ?2",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map(rusqlite::params![column_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            crate::store::note_cache::resolve_payload_rows(&guard, rows)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let conn = Arc::clone(&self.conn);
        let note_id = note_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Option<Note>> {
            let guard = conn.lock().unwrap();
            let row: Option<(String, String)> = guard
                .query_row("SELECT id, payload FROM note WHERE id = ?1", rusqlite::params![note_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            Ok(match row {
                Some((id, payload)) => {
                    crate::store::note_cache::resolve_payload_rows(&guard, vec![(id, payload)])?.into_iter().next()
                }
                None => None,
            })
        })
        .await
        .map_err(map_join_error)?
    }

    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || -> Result<usize> {
            let guard = conn.lock().unwrap();
            crate::store::note_cache::prune_sync(&guard, keep, max_age_days, max_size_mb)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        let conn = Arc::clone(&self.conn);
        // SqlWhereはCloneをderiveしていないためフィールドごとに複製する(Global Constraints参照)。
        let where_sql = crate::filter::sql::SqlWhere { sql: where_sql.sql.clone(), params: where_sql.params.clone() };
        let until_id = until_id.map(|s| s.to_string());
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
            let guard = conn.lock().unwrap();
            crate::store::note_cache::search_cache_sync(&guard, &where_sql, until_id.as_deref(), limit)
        })
        .await
        .map_err(map_join_error)?
    }

    // load_cached_before/update_note/clear_column_notes/get_fetch_boundary/set_fetch_boundary/
    // extend_fetch_boundary/clear_all_fetch_boundaries/note_count/notes_since も同じ形
    // (Arc::clone→引数を所有値化→spawn_blocking→中で対応する同期ロジックを実行)で実装する。
}
```

上記コードは`prune`/`search_cache`の中身を`note_cache.rs`側の`prune_sync`/`search_cache_sync`という新しい`pub(crate) fn`(引数`conn: &Connection`)に切り出す前提で書いている。旧`NoteCacheStore::prune`/`NoteCacheStore::search_cache`(書き換え前の`impl NoteCacheStore` ブロック、Task開始前の`note_cache.rs`)の中身(`self.conn.lock().unwrap()`を取得した後の部分)をそのまま`pub(crate) fn prune_sync(conn: &Connection, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize>` / `pub(crate) fn search_cache_sync(conn: &Connection, where_sql: &crate::filter::sql::SqlWhere, until_id: Option<&str>, limit: u32) -> Result<Vec<Note>>`として`note_cache.rs`に切り出し、`sqlite_backend.rs`から呼ぶ。他のメソッド(`load_cached_before`/`update_note`/`clear_column_notes`/`get_fetch_boundary`/`set_fetch_boundary`/`extend_fetch_boundary`/`clear_all_fetch_boundaries`/`note_count`/`notes_since`)は本体がごく短い(数行のSQL1〜2文)ので、`_sync`関数に切り出さず`spawn_blocking`クロージャに直接インライン展開してよい(旧`impl NoteCacheStore`の該当メソッドの中身をそのままクロージャの中へ貼り付ける)。

`note_cache.rs`側で`now_epoch`/`upsert_note`/`resolve_payload_rows`/`prune_sync`/`search_cache_sync`を`sqlite_backend.rs`から参照できるよう、`pub(crate)`にする(現状private `fn`のものはvisibilityを変更する)。

- [ ] **Step 5: `note_cache.rs`の既存テストを`sqlite_backend.rs`側へ移植する**

`note_cache.rs`の既存`#[cfg(test)] mod tests`(約30件、Task 1で追加した2件を含む)を`sqlite_backend.rs`側の新しい`#[cfg(test)] mod tests`として書き直す。変換ルール:
- `fn store() -> NoteCacheStore { NoteCacheStore::new(open_cache_in_memory().unwrap()) }` → `fn store() -> SqliteBackend { SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()) }`(`open_cache_in_memory`自体は同期のまま、`SqliteBackend::new`も同期なので`async fn`にする必要はない)
- 各`#[test] fn test_name() { let s = store(); s.cache_notes(...).unwrap(); ... }` → `#[tokio::test] async fn test_name() { let s = store(); s.cache_notes(...).await.unwrap(); ... }`(メソッド呼び出しすべてに`.await`を追加。`store()`自体は同期なので`.await`は不要)
- `s.conn.lock().unwrap()`で直接SQLを検証している箇所は`s.conn().lock().unwrap()`(Step 2で用意した`conn()`アクセサ経由)にする
- `insert_legacy_row`等のテストヘルパーはDBアクセスが同期のままなので変更不要(引数の型が`&Connection`のままなら`s.conn().lock().unwrap()`を渡す形にするだけ)

`note_cache.rs`本体には`#[cfg(test)] mod tests`を残さない(全てのテストロジックが移った後は不要。ただし`upsert_note`等の単体テスト、例えば`upsert_note_stores_stubbed_user_in_payload`のような「`Connection`を直接渡して`upsert_note`を呼ぶだけ」のテストは、`note_cache.rs`に残したままでもよい — 判断に迷ったら`sqlite_backend.rs`側に寄せる)。

- [ ] **Step 6: テストを実行して通過を確認する**

```bash
cd src-tauri && cargo test --lib store::sqlite_backend
cd src-tauri && cargo test --lib store::note_cache
```

Expected: PASS(移植した全テストがgreen)。まだ呼び出し元4ファイルを更新していないため、クレート全体の`cargo test`はこの時点でまだ失敗してよい。

- [ ] **Step 7: `lib.rs`/`state.rs`の初期化コードを更新する**

`src-tauri/src/lib.rs`の該当箇所(`db::open_cache(...)`の結果を`NoteCacheStore::new(...)`に渡している箇所):

```rust
// 変更前
let cache_conn =
    db::open_cache(&cache_dir.join("cache.db")).expect("failed to open cache db");
let cache = NoteCacheStore::new(cache_conn);
```

```rust
// 変更後
let cache_conn =
    db::open_cache(&cache_dir.join("cache.db")).expect("failed to open cache db");
let cache = NoteCacheStore::new(store::SqliteBackend::new(cache_conn));
```

(`db::open_cache`は同期のままなので`block_on`は不要。`SqliteBackend::new`も同期コンストラクタ)

`src-tauri/src/state.rs`の`new_for_test`:

```rust
    #[cfg(test)]
    pub(crate) fn new_for_test(settings: SettingsStore) -> Self {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ));
        Self::new_with_sound(
            Box::new(crate::session::MemoryStore::default()),
            settings,
            DraftStore::new_in_memory(),
            cache,
            SoundPlayer::new_for_test(),
        )
    }
```

(`new_for_test`自体は同期のまま。呼び出し元の`commands/draft.rs`/`commands/mute.rs`/`stream/connection.rs`のテストは変更不要)

- [ ] **Step 8: `commands/column.rs`のcall siteを更新する**

```bash
grep -n "state\.cache\." src-tauri/src/commands/column.rs
```

該当行(既に`async fn`内にある)に`.await`を追加する。`?`を使っている箇所は`.await?`に、`let _ = ...`パターンは`.await`のみ追加する。

`cache_with`テストヘルパーを更新する:

```rust
    fn cache_with(notes: &[Note]) -> NoteCacheStore {
        let store = NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ));
        tauri::async_runtime::block_on(store.cache_notes("col1", notes)).unwrap();
        store
    }
```

(`cache_with`自体は同期ヘルパーのままにしたいので、内部で`block_on`を使って`cache_notes`の`.await`を吸収する。呼び出し元のテスト関数を`async fn`化する必要はない)

- [ ] **Step 9: `commands/mute.rs`のcall siteを更新する**

```bash
grep -n "state\.cache\." src-tauri/src/commands/mute.rs
```

該当行に`.await`を追加する(`let _ = state.cache.clear_all_fetch_boundaries();` → `let _ = state.cache.clear_all_fetch_boundaries().await;`)。

- [ ] **Step 10: `commands/note.rs`のcall siteを更新する**

```bash
grep -n "state\.cache\." src-tauri/src/commands/note.rs
```

`state.cache.get_note(...)`/`state.cache.update_note(...)`の呼び出しに`.await`を追加する。`if let Ok(Some(mut note)) = state.cache.get_note(&note_id) {`は`if let Ok(Some(mut note)) = state.cache.get_note(&note_id).await {`になる。

- [ ] **Step 11: `stream/connection.rs`のcall siteを更新する**

```bash
grep -n "state\.cache\." src-tauri/src/stream/connection.rs
```

プロダクションコード側の呼び出しに`.await`を追加する。テストコード側(`#[test]`関数内)の呼び出しは、該当テスト関数を`#[tokio::test] async fn`化した上で`.await`を追加する(それ以外のテスト関数は変更不要)。

- [ ] **Step 12: 全体テストを実行する**

```bash
cd src-tauri && cargo test
```

Expected: PASS(全テスト)。コンパイルエラーが出た場合は`grep -rn "\.cache\.\w" src-tauri/src`で呼び出し元の漏れを再確認する。

- [ ] **Step 13: `cargo tauri dev`で実機動作を確認する**

リポジトリルートから`cargo tauri dev`を起動し、以下を手動確認する:
- カラムを開いてノートが表示される(`load_cached`/REST取得後の`cache_notes`)
- 新着ノートがStreaming経由でリアルタイム表示される(`cache_note`)
- リアクションをつけて表示に反映される(`update_note`)。取り消しても正しく反映される(Task 1のUPSERT化+失効行クリーンアップの確認)
- Backstage(設定画面)のキャッシュ件数表示が正しく出る(`note_count`)
- 検証後は自分で起動したdevサーバを終了する

- [ ] **Step 14: コミット**

```bash
git add -A
git commit -m "NoteCacheBackendトレイトを抽出しSqliteBackend(spawn_blocking方式)へ切り出し、呼び出し元をasync化"
```

---

## 完了条件

- `cd src-tauri && cargo test`が全件green
- `cd frontend && pnpm check`が影響を受けないこと(フロントエンドは今回のスコープ外、コマンドのRust側シグネチャ・TS bindingsは変わらないはず)を確認する
- `cargo tauri dev`での手動確認(Task 2 Step 13)が完了している
- Phase 2(PostgreSQL対応・`sqlx`+`sea-query`導入・設定UI)は別計画として、本Phase完了後にあらためて`writing-plans`で作成する
