# SQLiteチューニング(Issue #114) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `NoteCacheStore::load_cached`（起動時のカラム別直近ノート復元）を、ソートキーの非正規化とカバリングインデックスで高速化し、あわせて `cache.db` 接続に低リスクなPRAGMAチューニングを適用する。

**Architecture:** `column_note` テーブルに `note.created_at` を複製した `created_at` 列を追加し、`(column_id, created_at DESC, note_id DESC)` の複合インデックスを張る。`load_cached` のクエリを `column_note` 起点のJOINに変え、インデックスから直接ソート済みの上位N件を取り出せるようにする。既存DBのための非破壊マイグレーションを追加し、`open_cache` に `synchronous`/`temp_store`/`cache_size`/`mmap_size` のPRAGMAを設定する。

**Tech Stack:** Rust, `rusqlite`, SQLite (WAL mode)

## Global Constraints

- 対象は `cache.db`（`open_cache`）のみ。`open_settings`（レガシー移行専用）は変更しない。
- スキーマ変更は非破壊（既存DBはマイグレーションでバックフィル、破棄しても再取得で復元できる前提は維持）。
- `note.created_at` は作成後に変わらない値である前提で、`column_note.created_at` は挿入時に一度書けば良い（`INSERT OR IGNORE` で既存行は上書きしない）。
- 詳細仕様は `docs/superpowers/specs/2026-07-25-sqlite-tuning-design.md` を参照。

---

### Task 1: `column_note` へのソートキー非正規化とマイグレーション

**Files:**
- Modify: `src-tauri/src/store/db.rs`（`CACHE_SCHEMA` 定数、新規 `migrate_cache` 関数、`open_cache`）
- Test: `src-tauri/src/store/db.rs`（既存 `mod tests` 内に追加）

**Interfaces:**
- Consumes: なし（本タスクが起点）
- Produces: `column_note` テーブルに `created_at INTEGER NOT NULL DEFAULT 0` 列、`idx_cn_column_created` インデックス（`column_id, created_at DESC, note_id DESC`）。`fn migrate_cache(conn: &Connection) -> Result<()>`（Task 2/3では呼ばない。`open_cache` 内部でのみ使用）。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/db.rs` の `mod tests` に以下を追加する（`use super::*;` は既存のまま利用）:

```rust
#[test]
fn migrate_cache_backfills_created_at_from_note() {
    let conn = Connection::open_in_memory().unwrap();
    // 旧スキーマ（column_note に created_at 列が無い状態）を模倣
    conn.execute_batch(
        "CREATE TABLE note (id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
         CREATE TABLE column_note (
             column_id TEXT NOT NULL, note_id TEXT NOT NULL, received_at INTEGER NOT NULL,
             PRIMARY KEY (column_id, note_id)
         );
         INSERT INTO note (id, created_at) VALUES ('n1', 12345);
         INSERT INTO column_note (column_id, note_id, received_at) VALUES ('c1', 'n1', 999);",
    )
    .unwrap();

    migrate_cache(&conn).unwrap();

    let created_at: i64 = conn
        .query_row(
            "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(created_at, 12345);

    // 冪等: 再実行してもエラーにならず値は変わらない
    migrate_cache(&conn).unwrap();
    let created_at2: i64 = conn
        .query_row(
            "SELECT created_at FROM column_note WHERE column_id='c1' AND note_id='n1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(created_at2, 12345);

    // idx_cn_column_created が作成されていること
    let idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cn_column_created'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx_count, 1);
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test migrate_cache_backfills_created_at_from_note`
Expected: FAIL（`migrate_cache` が存在しないためコンパイルエラー）

- [ ] **Step 3: `CACHE_SCHEMA` に列を追加し、`migrate_cache` を実装する**

`CACHE_SCHEMA` 内の `column_note` 定義を変更する（`created_at` 列を追加。インデックスは追加しない — 既存DBでは列が無い状態でこの `CREATE INDEX` が走るとエラーになるため、インデックス作成は `migrate_cache` 側に寄せる）:

```rust
-- どのカラムにどのノートが流れたか（起動時の即時復元用）
CREATE TABLE IF NOT EXISTS column_note (
    column_id   TEXT NOT NULL,
    note_id     TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (column_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);
```

`migrate` 関数の直後（`fn column_exists` の前）に追加する:

```rust
/// `column_note` にソート用の `created_at` を非正規化し、`load_cached` が
/// カラム別に「新しい順」をインデックスだけで取り出せるようにする。
/// 既存DBには列が無いため、`note` から逆算してバックフィルしてからインデックスを張る。
/// 新規DBでは `column_exists` が true を返しバックフィルはスキップされ、インデックス作成のみ行う。
fn migrate_cache(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "column_note", "created_at")? {
        conn.execute_batch("ALTER TABLE column_note ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0")?;
        conn.execute_batch(
            "UPDATE column_note SET created_at = (
                SELECT created_at FROM note WHERE note.id = column_note.note_id
            )
            WHERE EXISTS (SELECT 1 FROM note WHERE note.id = column_note.note_id)",
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_cn_column_created \
         ON column_note(column_id, created_at DESC, note_id DESC)",
    )?;
    Ok(())
}
```

`open_cache` から呼び出す:

```rust
pub fn open_cache(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(CACHE_SCHEMA)?;
    migrate_cache(&conn)?;
    enable_incremental_vacuum(&conn)?;
    Ok(conn)
}
```

`open_cache_in_memory`（テスト用ヘルパー）にも同様に `migrate_cache(&conn)?;` を追加する:

```rust
#[cfg(test)]
pub fn open_cache_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(CACHE_SCHEMA)?;
    migrate_cache(&conn)?;
    enable_incremental_vacuum(&conn)?;
    Ok(conn)
}
```

- [ ] **Step 4: テストを実行してパスを確認する**

Run: `cd src-tauri && cargo test migrate_cache_backfills_created_at_from_note`
Expected: PASS

- [ ] **Step 5: 既存テストが壊れていないことを確認する**

Run: `cd src-tauri && cargo test --lib store::`
Expected: 全てPASS（`store/db.rs` と `store/note_cache.rs` の既存テストを含む）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/store/db.rs
git commit -m "feat: column_noteにcreated_atを非正規化しカバリングインデックスを追加"
```

---

### Task 2: `load_cached` をインデックス起点のクエリに変更

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`（`cache_notes` のINSERT文、`load_cached` のクエリ）

**Interfaces:**
- Consumes: Task 1 で追加された `column_note.created_at` 列、`idx_cn_column_created` インデックス
- Produces: なし（末端の変更。以降のタスクはこのファイルに依存しない）

- [ ] **Step 1: 失敗するテストは不要（既存テストを仕様として使う）**

`cache_roundtrip_preserves_note_and_order`（`src-tauri/src/store/note_cache.rs` 内、既存）が
本タスクの変更後も同じ順序（`created_at` 降順、同値は `id` 降順）を要求する回帰テストとして機能する。
先に現状で通ることを確認しておく:

Run: `cd src-tauri && cargo test cache_roundtrip_preserves_note_and_order`
Expected: PASS（変更前なので通る）

- [ ] **Step 2: `cache_notes` のINSERT文に `created_at` を追加する**

`src-tauri/src/store/note_cache.rs` の `cache_notes` 内、`column_note` へのINSERT部分を変更する:

```rust
    pub fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction()?;
        let now = now_epoch();
        for n in notes {
            upsert_note(&tx, n)?;
            tx.execute(
                "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![column_id, n.id, now, n.created_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 3: `load_cached` のクエリを `column_note` 起点に変更する**

```rust
    pub fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![column_id, limit], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for payload in rows {
            out.push(serde_json::from_str::<Note>(&payload?)?);
        }
        Ok(out)
    }
```

- [ ] **Step 4: 関連テストを実行してパスを確認する**

Run: `cd src-tauri && cargo test --lib store::note_cache::`
Expected: 全てPASS（`cache_roundtrip_preserves_note_and_order`、`upsert_replaces_and_relations_not_duplicated`、`column_isolation_and_clear`、`prune_*` など既存テストが無変更で通ること）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/note_cache.rs
git commit -m "perf: load_cachedをidx_cn_column_created経由のクエリに変更"
```

---

### Task 3: `open_cache` のPRAGMAチューニング

**Files:**
- Modify: `src-tauri/src/store/db.rs`（`open_cache`）
- Test: `src-tauri/src/store/db.rs`（既存 `mod tests` 内に追加）

**Interfaces:**
- Consumes: なし
- Produces: なし（末端の変更）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/db.rs` の `mod tests` に追加する。`open_cache` はファイルパスを取るため、
一時ファイルパスを使う（`uuid` は `db.rs` で既に依存として使われている）:

```rust
#[test]
fn open_cache_applies_pragma_tuning() {
    let path = std::env::temp_dir().join(format!("tsumugi_test_{}.db", uuid::Uuid::new_v4()));
    let conn = open_cache(&path).unwrap();

    let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
    assert_eq!(synchronous, 1); // NORMAL

    let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
    assert_eq!(temp_store, 2); // MEMORY

    let cache_size: i64 = conn.query_row("PRAGMA cache_size", [], |r| r.get(0)).unwrap();
    assert_eq!(cache_size, -20000);

    let mmap_size: i64 = conn.query_row("PRAGMA mmap_size", [], |r| r.get(0)).unwrap();
    assert_eq!(mmap_size, 67_108_864);

    drop(conn);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test open_cache_applies_pragma_tuning`
Expected: FAIL（PRAGMA未設定のためデフォルト値でアサーションが落ちる。例: `synchronous` はデフォルト `2`(FULL) のはずが `1` を期待して失敗）

- [ ] **Step 3: `open_cache` にPRAGMA設定を追加する**

```rust
pub fn open_cache(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -20_000i64)?;
    conn.pragma_update(None, "mmap_size", 67_108_864i64)?;
    conn.execute_batch(CACHE_SCHEMA)?;
    migrate_cache(&conn)?;
    enable_incremental_vacuum(&conn)?;
    Ok(conn)
}
```

- [ ] **Step 4: テストを実行してパスを確認する**

Run: `cd src-tauri && cargo test open_cache_applies_pragma_tuning`
Expected: PASS

- [ ] **Step 5: フルテストスイートを実行する**

Run: `cd src-tauri && cargo test`
Expected: 全てPASS（`#[ignore]` の実接続テストを除く）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/store/db.rs
git commit -m "perf: cache.dbにPRAGMAチューニングを適用"
```

---

## 完了確認

- [ ] `cd src-tauri && cargo test` が全てPASS
- [ ] `cd frontend && pnpm check`（TS側は変更していないが、bindings再生成に影響がないことの確認として実行）
