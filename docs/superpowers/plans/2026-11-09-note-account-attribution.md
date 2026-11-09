# ノートキャッシュのアカウント帰属(`note_account`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートキャッシュに「どのアカウントの取得経路で得たか」を記録する `note_account` を導入し、Issue #557(MCPサーバー)がアカウント単位でキャッシュを絞れるようにする。

**Architecture:** `NoteCacheBackend` trait に帰属用メソッドを**追加**し(既存メソッドは変えない)、SQLite / PostgreSQL / MySQL の3 backend で実装する。書き込み元3か所(ストリーミング受信・REST取得・ギャップ埋め)でキャッシュ書き込みの直後に記録し、`from cache` を含むカラムの REST 取得・ギャップ埋めでは記録しない。既存データはアプリ層から `column_note` を複写して埋め直す。

**Tech Stack:** Rust(Tauri v2)、rusqlite、sqlx(PostgreSQL / MySQL)、sea-query、tokio、async-trait、testcontainers(実 DB テスト)。

**Spec:** `docs/superpowers/specs/2026-11-09-note-account-attribution-design.md`

## Global Constraints

- ブランチは `feat/note-account-attribution`(作成済み)。`main` へ直接コミットしない。
- コミットメッセージは件名1行のみ、Conventional Commits 形式(`type(scope): 件名`)。本文は書かない(Co-Authored-By トレーラーは別途付与される)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗したら止めて報告する(リトライしない)。
- `NoteCacheBackend` の既存メソッド(`cache_notes` / `cache_note` ほか)のシグネチャと挙動は変えない。trait へは**追加のみ**。
- キャッシュスキーマのマイグレーションは冪等で、マーカー用テーブルを持たない(`CREATE TABLE IF NOT EXISTS`、複写は `ON CONFLICT DO NOTHING` / MySQL は `INSERT IGNORE`)。
- MySQL の ID 列は `string_len(64)`(`text` は主キーにできない)。PostgreSQL は `text`。
- `cargo run` / `./target/debug/tsumugi` は実行しない。検証は `cargo test` のみ(CLAUDE.md)。
- 実 DB テスト: `cd src-tauri && cargo test --lib postgres_ -- --ignored` / `cargo test --lib mysql_ -- --ignored --test-threads=2`(Docker 必須)。強制終了で残ったコンテナは `docker rm -fv <id>` で消す(`-v` 必須)。
- 帰属の記録に失敗しても、取得処理(境界の延長を含む)は失敗させない。警告ログのみ(ログに `account_id` は出さない)。
- 記録が欠けた場合は「見えない」側に倒れること(フェイルクローズ)。

## Review Focus

- 存在しないノート ID を記録しようとしても孤児行ができない(prune で消えた後の記録を含む) → Task 1 のテスト。
- 同じノートを複数アカウントが取得した場合、両方に帰属する → Task 1 のテスト。
- 解析できない TQL を持つカラムは `from cache` 扱いで埋め直しから除外される(フェイルクローズ) → Task 4 のテスト。
- `from home, cache` の混在カラムは、REST/ギャップ埋めでは記録せず、ストリーミングでは記録する → Task 5 のテスト。
- 記録先(`note_account`)が壊れていても、取得処理が成功する → Task 4 のテスト(`record` がエラーを握りつぶす)。
- MySQL に大量の ID(プレースホルダー上限超え)を渡しても記録できる → Task 3 のテスト。

---

## File Structure

| ファイル | 役割 |
|---|---|
| `src-tauri/src/store/note_cache.rs` | trait にメソッド追加、`NoteCacheStore` の委譲、SQLite の prune 対象テーブルに追加 |
| `src-tauri/src/store/db.rs` | SQLite の `CACHE_SCHEMA` に `note_account` を追加 |
| `src-tauri/src/store/sqlite_backend.rs` | SQLite 実装とテスト |
| `src-tauri/src/store/postgres_backend.rs` | PostgreSQL の DDL・実装・prune 対象テーブル・実 DB テスト |
| `src-tauri/src/store/mysql_backend.rs` | MySQL の DDL・実装・prune 対象テーブル・実 DB テスト |
| `src-tauri/src/attribution.rs`(新規) | 帰属のアプリ層ロジック(`from cache` 判定、記録の薄いラッパー、埋め直し) |
| `src-tauri/src/commands/column.rs` | `FilteredFetch.attribute_to`、`cache_fetched`、`commit_gap_fill_writes` の変更 |
| `src-tauri/src/stream/connection.rs` | ストリーミング受信の記録 |
| `src-tauri/src/commands/account.rs` | アカウント削除時の `forget_account_notes` |
| `src-tauri/src/commands/cache_backend.rs` | バックエンド切り替え後の埋め直し |
| `src-tauri/src/lib.rs` | `mod attribution;` と起動時の埋め直し |

---

### Task 1: trait の追加 + SQLite 実装

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`(trait、`NoteCacheStore`、`delete_matching` の対象テーブル)
- Modify: `src-tauri/src/store/db.rs`(`CACHE_SCHEMA`)
- Modify: `src-tauri/src/store/sqlite_backend.rs`(実装 + テスト)

**Interfaces:**
- Produces(以降のタスクが使う):
  - `NoteCacheBackend::record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()>`
  - `NoteCacheBackend::backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()>`
  - `NoteCacheBackend::forget_account_notes(&self, account_id: &str) -> Result<()>`
  - `NoteCacheBackend::account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool>`
  - `NoteCacheStore` に同名・同シグネチャの委譲メソッド。
- この Task では trait にデフォルト実装(何もしない)を**一時的に**置く。Task 2・3 で PostgreSQL / MySQL が実装し、Task 3 の最後で削除する(実装し忘れた backend を静かに許さないため)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/sqlite_backend.rs` の `#[cfg(test)] mod tests` の末尾に追加する。

```rust
    #[tokio::test]
    async fn note_account_records_idempotently_and_ignores_unknown_notes() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        s.record_note_accounts("acc1", &["n1".to_string(), "ghost".to_string()]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap(); // 再実行しても変わらない

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        // note に存在しない ID は孤児行にしない
        assert!(!s.account_has_note("acc1", "ghost").await.unwrap());
        // 記録していない別アカウントには帰属しない
        assert!(!s.account_has_note("acc2", "n1").await.unwrap());
    }

    #[tokio::test]
    async fn note_account_allows_one_note_to_belong_to_several_accounts() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap();
        s.record_note_accounts("acc2", &["n1".to_string()]).await.unwrap();

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc2", "n1").await.unwrap());
    }

    #[tokio::test]
    async fn note_account_is_removed_together_with_the_note_by_prune() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string(), "n3".to_string()]).await.unwrap();

        s.prune(2, 0, 0).await.unwrap(); // 最古の n1 が消える

        assert!(!s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc1", "n3").await.unwrap());
    }

    #[tokio::test]
    async fn note_account_forget_removes_only_the_given_account() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap();
        s.record_note_accounts("acc2", &["n1".to_string()]).await.unwrap();

        s.forget_account_notes("acc1").await.unwrap();

        assert!(!s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc2", "n1").await.unwrap());
    }

    #[tokio::test]
    async fn note_account_backfill_copies_the_column_notes_idempotently() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        s.cache_notes("col2", &[note("n3", 300)]).await.unwrap();

        s.backfill_note_accounts("acc1", "col1").await.unwrap();
        s.backfill_note_accounts("acc1", "col1").await.unwrap(); // 再実行しても変わらない

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc1", "n2").await.unwrap());
        assert!(!s.account_has_note("acc1", "n3").await.unwrap()); // 別カラムは複写しない
    }

    #[tokio::test]
    async fn note_account_survives_clear_column_notes() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap();

        s.clear_column_notes("col1").await.unwrap();

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
    }
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認**

Run: `cd src-tauri && cargo test --lib note_account_ 2>&1 | tail -20`
Expected: FAIL — `no method named record_note_accounts found`

- [ ] **Step 3: trait にメソッドを追加(一時的なデフォルト実装つき)**

`src-tauri/src/store/note_cache.rs` の `pub(crate) trait NoteCacheBackend` の中、`search_cache` の宣言の直後に追加する。

```rust
    // --- アカウント帰属(Issue #557、docs/superpowers/specs/2026-11-09-note-account-attribution-design.md) ---
    // 一時的なデフォルト実装(何もしない)。PostgreSQL / MySQL の実装が揃ったら削除する。

    /// アカウントがこれらのノートを自分の取得経路で得たことを記録する(冪等)。
    /// `note` に存在しない ID は無視する(孤児行を作らない)。
    async fn record_note_accounts(&self, _account_id: &str, _note_ids: &[String]) -> Result<()> {
        Ok(())
    }
    /// `column_note` のあるカラムのノートを、アカウントの帰属として複写する(埋め直し用、冪等)。
    async fn backfill_note_accounts(&self, _account_id: &str, _column_id: &str) -> Result<()> {
        Ok(())
    }
    /// アカウント削除時に、そのアカウントの帰属を全て消す。
    async fn forget_account_notes(&self, _account_id: &str) -> Result<()> {
        Ok(())
    }
    /// このノートがそのアカウントに帰属しているか。
    async fn account_has_note(&self, _account_id: &str, _note_id: &str) -> Result<bool> {
        Ok(false)
    }
```

同じファイルの `impl NoteCacheStore` の末尾(`search_cache` の直後)に委譲メソッドを追加する。

```rust
    /// アカウントがこれらのノートを自分の取得経路で得たことを記録する(Issue #557)。
    pub async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()> {
        self.backend().record_note_accounts(account_id, note_ids).await
    }

    /// `column_note` のあるカラムのノートをアカウントの帰属として複写する(埋め直し用)。
    pub async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()> {
        self.backend().backfill_note_accounts(account_id, column_id).await
    }

    /// アカウント削除時に、そのアカウントの帰属を全て消す。
    pub async fn forget_account_notes(&self, account_id: &str) -> Result<()> {
        self.backend().forget_account_notes(account_id).await
    }

    /// このノートがそのアカウントに帰属しているか。
    pub async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool> {
        self.backend().account_has_note(account_id, note_id).await
    }
```

同じファイルの `delete_matching` の対象テーブル一覧に `note_account` を加える。

```rust
    for table in ["column_note", "note_account", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
```

- [ ] **Step 4: スキーマを追加**

`src-tauri/src/store/db.rs` の `CACHE_SCHEMA` で、`CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);` の直後に追加する。

```sql

-- アカウントがどのノートを自分の取得経路(ストリーミング/REST)で得たか(Issue #557)。
-- note は全アカウント共通なので、アカウント単位の絞り込みはこのテーブルで行う。
CREATE TABLE IF NOT EXISTS note_account (
    account_id TEXT NOT NULL,
    note_id    TEXT NOT NULL,
    PRIMARY KEY (account_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_na_note ON note_account(note_id);
```

- [ ] **Step 5: SQLite の実装を追加**

`src-tauri/src/store/sqlite_backend.rs` の `impl NoteCacheBackend for SqliteBackend` の中、`clear_column_notes` メソッドの直後に追加する。

```rust
    async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()> {
        if note_ids.is_empty() {
            return Ok(());
        }
        let conn = Arc::clone(&self.conn);
        let account_id = account_id.to_string();
        let note_ids = note_ids.to_vec();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            for id in &note_ids {
                // note に存在する ID だけを記録する(prune 済みのノートで孤児行を作らない)。
                tx.execute(
                    "INSERT OR IGNORE INTO note_account (account_id, note_id)
                     SELECT ?1, id FROM note WHERE id = ?2",
                    rusqlite::params![account_id, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let (account_id, column_id) = (account_id.to_string(), column_id.to_string());
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT OR IGNORE INTO note_account (account_id, note_id)
                 SELECT ?1, note_id FROM column_note WHERE column_id = ?2",
                rusqlite::params![account_id, column_id],
            )?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn forget_account_notes(&self, account_id: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let account_id = account_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute("DELETE FROM note_account WHERE account_id = ?1", rusqlite::params![account_id])?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool> {
        let conn = Arc::clone(&self.conn);
        let (account_id, note_id) = (account_id.to_string(), note_id.to_string());
        tauri::async_runtime::spawn_blocking(move || -> Result<bool> {
            let guard = conn.lock().unwrap();
            let found: bool = guard.query_row(
                "SELECT EXISTS(SELECT 1 FROM note_account WHERE account_id = ?1 AND note_id = ?2)",
                rusqlite::params![account_id, note_id],
                |r| r.get(0),
            )?;
            Ok(found)
        })
        .await
        .map_err(map_join_error)?
    }
```

- [ ] **Step 6: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib note_account_ 2>&1 | tail -20`
Expected: PASS(6 tests)

- [ ] **Step 7: 既存のキャッシュ系テストが壊れていないことを確認**

Run: `cd src-tauri && cargo test --lib store:: 2>&1 | tail -8`
Expected: `test result: ok.`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/store/note_cache.rs src-tauri/src/store/db.rs src-tauri/src/store/sqlite_backend.rs
git commit -m "feat(store): ノートキャッシュにアカウント帰属(note_account)を追加しSQLiteで実装 (#557)"
```

---

### Task 2: PostgreSQL 実装

**Files:**
- Modify: `src-tauri/src/store/postgres_backend.rs`(DDL、iden、実装、`delete_matching_ids` の対象テーブル、テスト)

**Interfaces:**
- Consumes: Task 1 の trait メソッド4つ(シグネチャは Task 1 の Interfaces のとおり)。
- Produces: `PostgresBackend` が4メソッドを実装する。

- [ ] **Step 1: 失敗するテストを書く**

`postgres_backend.rs` の `mod tests` の末尾に追加する(`backend()` と `note()` は既存のヘルパー)。

```rust
    #[tokio::test]
    #[ignore]
    async fn note_account_records_idempotently_and_ignores_unknown_notes() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        s.record_note_accounts("acc1", &["n1".to_string(), "ghost".to_string()]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap();

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(!s.account_has_note("acc1", "ghost").await.unwrap());
        assert!(!s.account_has_note("acc2", "n1").await.unwrap());
    }

    #[tokio::test]
    #[ignore]
    async fn note_account_is_removed_together_with_the_note_by_prune() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string(), "n3".to_string()]).await.unwrap();

        s.prune(2, 0, 0).await.unwrap();

        assert!(!s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc1", "n3").await.unwrap());
    }

    #[tokio::test]
    #[ignore]
    async fn note_account_forget_and_backfill_and_clear_column() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        s.cache_notes("col2", &[note("n3", 300)]).await.unwrap();

        s.backfill_note_accounts("acc1", "col1").await.unwrap();
        s.backfill_note_accounts("acc1", "col1").await.unwrap(); // 冪等
        s.backfill_note_accounts("acc2", "col1").await.unwrap();
        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(!s.account_has_note("acc1", "n3").await.unwrap());

        s.clear_column_notes("col1").await.unwrap();
        assert!(s.account_has_note("acc1", "n2").await.unwrap()); // カラム消去では帰属は消えない

        s.forget_account_notes("acc1").await.unwrap();
        assert!(!s.account_has_note("acc1", "n2").await.unwrap());
        assert!(s.account_has_note("acc2", "n2").await.unwrap());
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib postgres_backend::tests::note_account -- --ignored 2>&1 | tail -15`
Expected: FAIL — デフォルト実装は何も記録しないため `assertion failed`(`account_has_note` が常に false)

- [ ] **Step 3: DDL を追加**

`postgres_backend.rs` のスキーマ作成関数で、`idx_cn_column_created` の `pool.execute(...)` の直後(`let column_source_boundary = ...` の前)に追加する。

```rust
    let note_account = Table::create()
        .table(NoteAccountTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteAccountTable::AccountId).text().not_null())
        .col(ColumnDef::new(NoteAccountTable::NoteId).text().not_null())
        .primary_key(Index::create().col(NoteAccountTable::AccountId).col(NoteAccountTable::NoteId))
        .build(PostgresQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_account)).await?;

    let idx_na_note = Index::create()
        .if_not_exists()
        .name("idx_na_note")
        .table(NoteAccountTable::Table)
        .col(NoteAccountTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(idx_na_note)).await?;

```

`ColumnNoteTable` の定義の直後に iden を追加する。

```rust
#[derive(sea_query::Iden)]
enum NoteAccountTable {
    #[iden = "note_account"]
    Table,
    AccountId,
    NoteId,
}
```

- [ ] **Step 4: 実装を追加**

`impl NoteCacheBackend for PostgresBackend` の中、`clear_column_notes` の直後に追加する。

```rust
    async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()> {
        if note_ids.is_empty() {
            return Ok(());
        }
        // note に存在する ID だけを記録する(prune 済みのノートで孤児行を作らない)。
        sqlx::query(
            "INSERT INTO note_account (account_id, note_id) SELECT $1, id FROM note WHERE id = ANY($2)
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(note_ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO note_account (account_id, note_id) SELECT $1, note_id FROM column_note WHERE column_id = $2
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(column_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn forget_account_notes(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM note_account WHERE account_id = $1").bind(account_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool> {
        let (found,): (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM note_account WHERE account_id = $1 AND note_id = $2)")
                .bind(account_id)
                .bind(note_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(found)
    }
```

`delete_matching_ids` の対象テーブル一覧に追加する。

```rust
    for table in ["column_note", "note_account", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
```

- [ ] **Step 5: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib postgres_backend::tests::note_account -- --ignored 2>&1 | tail -15`
Expected: PASS(3 tests。Docker が必要)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/store/postgres_backend.rs
git commit -m "feat(store): note_accountをPostgreSQL backendで実装 (#557)"
```

---

### Task 3: MySQL 実装 + trait のデフォルト実装の削除

**Files:**
- Modify: `src-tauri/src/store/mysql_backend.rs`
- Modify: `src-tauri/src/store/note_cache.rs`(trait のデフォルト実装を削除)

**Interfaces:**
- Consumes: Task 1 の trait メソッド4つ。
- Produces: 全 backend が4メソッドを実装し、trait のデフォルト実装がなくなる。

- [ ] **Step 1: 失敗するテストを書く**

`mysql_backend.rs` の `mod tests` の末尾に追加する。

```rust
    #[tokio::test]
    #[ignore]
    async fn note_account_records_idempotently_and_ignores_unknown_notes() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        s.record_note_accounts("acc1", &["n1".to_string(), "ghost".to_string()]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string()]).await.unwrap();

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(!s.account_has_note("acc1", "ghost").await.unwrap());
        assert!(!s.account_has_note("acc2", "n1").await.unwrap());
    }

    /// プレースホルダー数の上限(65535)を超えうる大量の ID でも記録できる(チャンク分割)。
    #[tokio::test]
    #[ignore]
    async fn note_account_records_a_large_id_list_in_chunks() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        let mut ids: Vec<String> = (0..70_000).map(|i| format!("ghost{i}")).collect();
        ids.push("n1".to_string());
        ids.push("n2".to_string());

        s.record_note_accounts("acc1", &ids).await.unwrap();

        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc1", "n2").await.unwrap());
    }

    #[tokio::test]
    #[ignore]
    async fn note_account_is_removed_together_with_the_note_by_prune() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.record_note_accounts("acc1", &["n1".to_string(), "n3".to_string()]).await.unwrap();

        s.prune(2, 0, 0).await.unwrap();

        assert!(!s.account_has_note("acc1", "n1").await.unwrap());
        assert!(s.account_has_note("acc1", "n3").await.unwrap());
    }

    #[tokio::test]
    #[ignore]
    async fn note_account_forget_and_backfill_and_clear_column() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        s.cache_notes("col2", &[note("n3", 300)]).await.unwrap();

        s.backfill_note_accounts("acc1", "col1").await.unwrap();
        s.backfill_note_accounts("acc1", "col1").await.unwrap(); // 冪等
        s.backfill_note_accounts("acc2", "col1").await.unwrap();
        assert!(s.account_has_note("acc1", "n1").await.unwrap());
        assert!(!s.account_has_note("acc1", "n3").await.unwrap());

        s.clear_column_notes("col1").await.unwrap();
        assert!(s.account_has_note("acc1", "n2").await.unwrap());

        s.forget_account_notes("acc1").await.unwrap();
        assert!(!s.account_has_note("acc1", "n2").await.unwrap());
        assert!(s.account_has_note("acc2", "n2").await.unwrap());
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib mysql_backend::tests::note_account -- --ignored --test-threads=2 2>&1 | tail -15`
Expected: FAIL(デフォルト実装は何も記録しないため `assertion failed`)

- [ ] **Step 3: DDL を追加**

`mysql_backend.rs` のスキーマ作成関数で、`idx_cn_column_created` の `execute_index(...)` の直後(`let column_source_boundary = ...` の前)に追加する。

```rust
    let note_account = Table::create()
        .table(NoteAccountTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteAccountTable::AccountId).string_len(64).not_null())
        .col(ColumnDef::new(NoteAccountTable::NoteId).string_len(64).not_null())
        .primary_key(Index::create().col(NoteAccountTable::AccountId).col(NoteAccountTable::NoteId))
        .build(MysqlQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(note_account)).await?;

    let idx_na_note = Index::create()
        .if_not_exists()
        .name("idx_na_note")
        .table(NoteAccountTable::Table)
        .col(NoteAccountTable::NoteId)
        .build(MysqlQueryBuilder);
    execute_index(pool, idx_na_note.as_str()).await?;

```

`ColumnNoteTable` の定義の直後に iden を追加する。

```rust
#[derive(sea_query::Iden)]
enum NoteAccountTable {
    #[iden = "note_account"]
    Table, AccountId, NoteId,
}
```

- [ ] **Step 4: 実装を追加**

`impl NoteCacheBackend for MySqlBackend` の中、`clear_column_notes` の直後に追加する。

```rust
    async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()> {
        // MySQL のプレースホルダー上限(65535)を超えないよう、チャンクに分けて記録する。
        for chunk in note_ids.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            // note に存在する ID だけを記録する(prune 済みのノートで孤児行を作らない)。
            // `placeholders` は "?" の羅列のみで、ID は `.bind()` 経由でのみ渡すためSQLインジェクションの懸念はない。
            let sql = format!(
                "INSERT IGNORE INTO note_account (account_id, note_id) SELECT ?, id FROM note WHERE id IN ({placeholders})"
            );
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(account_id);
            for id in chunk {
                q = q.bind(id);
            }
            q.execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT IGNORE INTO note_account (account_id, note_id) SELECT ?, note_id FROM column_note WHERE column_id = ?",
        )
        .bind(account_id)
        .bind(column_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn forget_account_notes(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM note_account WHERE account_id = ?").bind(account_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool> {
        let (found,): (i64,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM note_account WHERE account_id = ? AND note_id = ?)")
                .bind(account_id)
                .bind(note_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(found != 0)
    }
```

`delete_matching_ids_chunk` の対象テーブル一覧に追加する。

```rust
    for table in ["column_note", "note_account", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
```

- [ ] **Step 5: trait のデフォルト実装を削除**

`note_cache.rs` の trait で、Task 1 で追加した4メソッドの本体(`{ Ok(()) }` / `{ Ok(false) }`)とコメント「一時的なデフォルト実装…」を削除し、宣言のみにする。

```rust
    // --- アカウント帰属(Issue #557、docs/superpowers/specs/2026-11-09-note-account-attribution-design.md) ---

    /// アカウントがこれらのノートを自分の取得経路で得たことを記録する(冪等)。
    /// `note` に存在しない ID は無視する(孤児行を作らない)。
    async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()>;
    /// `column_note` のあるカラムのノートを、アカウントの帰属として複写する(埋め直し用、冪等)。
    async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()>;
    /// アカウント削除時に、そのアカウントの帰属を全て消す。
    async fn forget_account_notes(&self, account_id: &str) -> Result<()>;
    /// このノートがそのアカウントに帰属しているか。
    async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool>;
```

- [ ] **Step 6: ビルドとテスト**

Run: `cd src-tauri && cargo build --tests 2>&1 | tail -15`
Expected: エラーなし(3 backend すべてが4メソッドを実装していること)

Run: `cd src-tauri && cargo test --lib mysql_backend::tests::note_account -- --ignored --test-threads=2 2>&1 | tail -15`
Expected: PASS(4 tests。Docker が必要)

Run: `cd src-tauri && cargo test --lib note_account_ 2>&1 | tail -8`
Expected: PASS(SQLite の 6 tests)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/store/mysql_backend.rs src-tauri/src/store/note_cache.rs
git commit -m "feat(store): note_accountをMySQL backendで実装しtraitのデフォルト実装を削除 (#557)"
```

---

### Task 4: アプリ層の `attribution` モジュール

**Files:**
- Create: `src-tauri/src/attribution.rs`
- Modify: `src-tauri/src/lib.rs`(`mod attribution;` を追加)

**Interfaces:**
- Consumes: `NoteCacheStore::record_note_accounts` / `backfill_note_accounts`(Task 1)、`SettingsStore::load_columns() -> Result<Vec<Column>>`。
- Produces(Task 5・6 が使う):
  - `pub(crate) fn column_includes_cache_source(column: &Column) -> bool`
  - `pub(crate) fn attribution_target(account_id: &str, use_cache: bool) -> Option<String>`
  - `pub(crate) async fn record(cache: &NoteCacheStore, account_id: &str, notes: &[Note])`(エラーは握りつぶして警告ログ)
  - `pub(crate) async fn backfill_all(settings: &SettingsStore, cache: &NoteCacheStore)`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/attribution.rs` を新規作成し、まずテストだけを書く(実装は次のステップ)。

```rust
//! ノートキャッシュのアカウント帰属のアプリ層ロジック(Issue #557)。
//! 設計: docs/superpowers/specs/2026-11-09-note-account-attribution-design.md

use crate::domain::{Column, ColumnKind, FilterQuery, Note};
use crate::filter::{ast, parser};
use crate::store::{NoteCacheStore, SettingsStore};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteBackend;

    fn column(id: &str, account_id: &str, kind: ColumnKind, filter: FilterQuery) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind,
            order: 0,
            filter,
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    fn tql(text: &str) -> Column {
        column("c", "acc1", ColumnKind::Tql, FilterQuery::Tql(text.into()))
    }

    fn mem_cache() -> NoteCacheStore {
        NoteCacheStore::new(SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()))
    }

    fn note(id: &str, created_at: i64) -> Note {
        Note {
            id: id.into(),
            created_at,
            text: None,
            cw: None,
            visibility: crate::domain::Visibility::Public,
            local_only: false,
            user: crate::domain::User {
                id: "author".into(),
                username: "alice".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: Default::default(),
                bio: None,
                banner_url: None,
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
            reply: None,
            renote_id: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: Default::default(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: Default::default(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn a_plain_column_does_not_include_the_cache_source() {
        assert!(!column_includes_cache_source(&column("c", "acc1", ColumnKind::Home, FilterQuery::Keywords(vec![]))));
    }

    #[test]
    fn a_tql_column_includes_the_cache_source_only_when_from_lists_cache() {
        assert!(!column_includes_cache_source(&tql("from home where reply")));
        assert!(column_includes_cache_source(&tql("from cache where reply")));
        assert!(column_includes_cache_source(&tql("from home, cache where reply")));
    }

    /// 解析できない TQL は、from cache を含むものとして扱う(埋め直しから除外=見えない側に倒す)。
    #[test]
    fn an_unparsable_tql_column_is_treated_as_including_the_cache_source() {
        assert!(column_includes_cache_source(&tql("from")));
        assert!(column_includes_cache_source(&column("c", "acc1", ColumnKind::Tql, FilterQuery::Keywords(vec![]))));
    }

    #[test]
    fn attribution_target_is_none_when_the_column_uses_the_cache() {
        assert_eq!(attribution_target("acc1", false).as_deref(), Some("acc1"));
        assert_eq!(attribution_target("acc1", true), None);
    }

    #[tokio::test]
    async fn record_attributes_the_notes_to_the_account() {
        let cache = mem_cache();
        cache.cache_notes("col1", &[note("n1", 1), note("n2", 2)]).await.unwrap();

        record(&cache, "acc1", &[note("n1", 1)]).await;

        assert!(cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(!cache.account_has_note("acc1", "n2").await.unwrap());
    }

    #[tokio::test]
    async fn record_with_no_notes_is_a_noop() {
        let cache = mem_cache();
        record(&cache, "acc1", &[]).await;
    }

    /// 記録先が壊れていても、panic せず呼び出し元へエラーを返さない(取得処理を止めない)。
    #[tokio::test]
    async fn record_swallows_backend_errors() {
        let backend = SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap());
        let conn = std::sync::Arc::clone(backend.conn());
        let cache = NoteCacheStore::new(backend);
        cache.cache_notes("col1", &[note("n1", 1)]).await.unwrap();
        conn.lock().unwrap().execute("DROP TABLE note_account", []).unwrap();

        record(&cache, "acc1", &[note("n1", 1)]).await; // ここで戻ってくること自体がテスト
    }

    #[tokio::test]
    async fn backfill_all_copies_plain_columns_and_skips_columns_that_use_the_cache() {
        let settings = SettingsStore::new_in_memory();
        settings.upsert_column(&column("plain", "acc1", ColumnKind::Home, FilterQuery::Keywords(vec![]))).unwrap();
        let mut mixed = tql("from home, cache where reply");
        mixed.id = "mixed".into();
        settings.upsert_column(&mixed).unwrap();
        let cache = mem_cache();
        cache.cache_notes("plain", &[note("n1", 1)]).await.unwrap();
        cache.cache_notes("mixed", &[note("n2", 2)]).await.unwrap();

        backfill_all(&settings, &cache).await;

        assert!(cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(!cache.account_has_note("acc1", "n2").await.unwrap()); // from cache を含むカラムは除外
    }
}
```

- [ ] **Step 2: モジュールを登録し、テストが失敗することを確認**

`src-tauri/src/lib.rs` の `mod api;` の直後に追加する。

```rust
mod attribution;
```

Run: `cd src-tauri && cargo test --lib attribution:: 2>&1 | tail -15`
Expected: FAIL — `cannot find function column_includes_cache_source` など

(`Note` / `User` の初期化は `src-tauri/src/domain/note.rs` のテストの `minimal_note` と同じ。フィールドがずれてコンパイルエラーになる場合は、そちらに合わせる。)

- [ ] **Step 3: 実装を書く**

`attribution.rs` の `use` 群の直後(`#[cfg(test)]` の前)に追加する。

```rust
/// このカラムが `from cache` を含むか。含むカラムの REST 取得結果にはグローバルなキャッシュ由来の
/// ノートが混ざり、そのアカウントが取得した証拠にならない。ネットワークは使わない純粋な判定で、
/// 解析できない TQL は「含む」として扱う(埋め直しから除外=見えない側に倒す)。
pub(crate) fn column_includes_cache_source(column: &Column) -> bool {
    if !matches!(column.kind, ColumnKind::Tql) {
        return false;
    }
    let FilterQuery::Tql(text) = &column.filter else {
        return true;
    };
    match parser::parse(text) {
        Ok(q) => q.sources.iter().any(|s| matches!(s, ast::Source::Cache)),
        Err(_) => true,
    }
}

/// REST 取得・ギャップ埋めの結果を帰属させるアカウント。`from cache` を含むカラムでは None。
pub(crate) fn attribution_target(account_id: &str, use_cache: bool) -> Option<String> {
    (!use_cache).then(|| account_id.to_string())
}

/// 取得経路の帰属を記録する。失敗しても取得処理は止めない(未記録は外部に出ない側に倒れるだけ)。
pub(crate) async fn record(cache: &NoteCacheStore, account_id: &str, notes: &[Note]) {
    if notes.is_empty() {
        return;
    }
    let ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();
    if let Err(e) = cache.record_note_accounts(account_id, &ids).await {
        log::warn!("failed to record the note attribution: {e}");
    }
}

/// 既存の `column_note` から、`from cache` を含まないカラムの分を帰属として複写する(冪等)。
/// 起動時と、キャッシュのバックエンドを切り替えた直後に呼ぶ。
pub(crate) async fn backfill_all(settings: &SettingsStore, cache: &NoteCacheStore) {
    let columns = match settings.load_columns() {
        Ok(columns) => columns,
        Err(e) => {
            log::warn!("skipping the note attribution backfill: failed to load the columns: {e}");
            return;
        }
    };
    for column in columns.iter().filter(|c| !column_includes_cache_source(c)) {
        if let Err(e) = cache.backfill_note_accounts(&column.account_id, &column.id).await {
            log::warn!("failed to backfill the note attribution of a column: {e}");
        }
    }
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib attribution:: 2>&1 | tail -15`
Expected: PASS(8 tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/attribution.rs src-tauri/src/lib.rs
git commit -m "feat(attribution): from cache判定・記録・埋め直しのアプリ層ロジックを追加 (#557)"
```

---

### Task 5: 書き込み経路への配線

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`FilteredFetch`、`collect_backfill_pages`、`fetch_and_filter_multi`、`cache_fetched`、`commit_gap_fill_writes` とその呼び出し元、テスト)
- Modify: `src-tauri/src/stream/connection.rs`(ストリーミング受信)

**Interfaces:**
- Consumes: Task 4 の `attribution::{attribution_target, record}`、Task 1 の `NoteCacheStore::account_has_note`。
- Produces: `commit_gap_fill_writes(fence, cache, column_id, epoch, gap, attribute_to: Option<&str>) -> bool`(末尾に引数を追加)。`FilteredFetch.attribute_to: Option<String>`。`stream::connection::cache_streamed_note`。

- [ ] **Step 1: 失敗するテストを書く(column.rs)**

`commands/column.rs` の `mod tests` の `cache_fetched_stores_the_cacheable_set_not_the_truncated_display_notes` の直後に追加する。

```rust
    #[tokio::test]
    async fn cache_fetched_records_the_attribution_only_when_attribute_to_is_set() {
        let store = cache_with(&[]).await;
        let mut fetch = FilteredFetch {
            notes: vec![note("n01", 1)],
            cacheable: vec![note("n01", 1)],
            source_outcomes: vec![],
            attribute_to: None,
        };

        cache_fetched(&store, "col1", &fetch).await.unwrap();
        assert!(!store.account_has_note("acc1", "n01").await.unwrap());

        fetch.attribute_to = Some("acc1".into());
        cache_fetched(&store, "col1", &fetch).await.unwrap();
        assert!(store.account_has_note("acc1", "n01").await.unwrap());
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_records_the_attribution_only_when_attribute_to_is_set() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");

        commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)]), None).await;
        assert!(!cache.account_has_note("acc1", "n600").await.unwrap());

        commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)]), Some("acc1")).await;
        assert!(cache.account_has_note("acc1", "n600").await.unwrap());
    }
```

あわせて、既存の呼び出しと構築箇所を新シグネチャに合わせる。

- `FilteredFetch { notes, cacheable, source_outcomes: vec![fetched("n02"), fetched("n01")] }`(`cache_fetched_stores_the_cacheable_set_...` の中)に `attribute_to: None` を足す。
- `fn fetch_of` の `FilteredFetch { ... source_outcomes: vec![], }` に `attribute_to: None,` を足す。
- `commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await` の既存4か所(`commit_gap_fill_writes_caches_notes_and_raises_boundaries_when_current` ほか)の末尾に `, None` を足す。

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認**

Run: `cd src-tauri && cargo test --lib commands::column::tests::cache_fetched_records 2>&1 | tail -15`
Expected: FAIL — `struct FilteredFetch has no field named attribute_to`

- [ ] **Step 3: `FilteredFetch` とその構築を変更**

`struct FilteredFetch` にフィールドを追加する。

```rust
    /// `resolved.kinds` と同じ並びの、ソースごとの取得結果。
    source_outcomes: Vec<SourceOutcome>,
    /// この取得結果を帰属させるアカウント(Issue #557)。`from cache` を含むカラムでは None
    /// (グローバルなキャッシュ由来のノートが混ざり、そのアカウントが取得した証拠にならないため)。
    attribute_to: Option<String>,
}
```

`collect_backfill_pages` の末尾を変更する。

```rust
    Ok(FilteredFetch { notes: display_acc, cacheable, source_outcomes: merged_outcomes, attribute_to: None })
```

`fetch_and_filter_multi` を変更する。

```rust
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    let mut fetch = collect_backfill_pages(until_id, resolved.use_cache, |until| async move {
        fetch_backfill_pass(state, account_id, resolved, until.as_deref()).await
    })
    .await?;
    fetch.attribute_to = crate::attribution::attribution_target(account_id, resolved.use_cache);
    Ok(fetch)
}
```

- [ ] **Step 4: `cache_fetched` を変更**

```rust
async fn cache_fetched(cache: &NoteCacheStore, column_id: &str, fetch: &FilteredFetch) -> Result<()> {
    cache.cache_notes(column_id, &fetch.cacheable).await?;
    if let Some(account_id) = fetch.attribute_to.as_deref() {
        crate::attribution::record(cache, account_id, &fetch.cacheable).await;
    }
    Ok(())
}
```

- [ ] **Step 5: `commit_gap_fill_writes` と呼び出し元を変更**

```rust
async fn commit_gap_fill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    gap: &GapFillResult,
    attribute_to: Option<&str>,
) -> bool {
    fence
        .write_if_current(column_id, epoch, |_boundaries_ok| async {
            apply_gap_fill_boundaries(cache, column_id, gap).await;
            if !gap.notes.is_empty() {
                let _ = cache.cache_notes(column_id, &gap.notes).await;
                if let Some(account_id) = attribute_to {
                    crate::attribution::record(cache, account_id, &gap.notes).await;
                }
            }
        })
        .await
        .is_some()
}
```

呼び出し元(非テスト)の2か所を変更する。

1つ目(`tauri::async_runtime::spawn(async move {` の中、`let gap_result = fill_gap(...)` の後):

```rust
                let attribute_to = crate::attribution::attribution_target(&account_id, resolved.use_cache);
                if !commit_gap_fill_writes(
                    &state.column_fence,
                    &state.cache,
                    &column_id,
                    &epoch,
                    &gap_result,
                    attribute_to.as_deref(),
                )
                .await
                {
                    return;
                }
```

2つ目(`gap_fill_on_reconnect` の中):

```rust
    let attribute_to = crate::attribution::attribution_target(&column.account_id, resolved.use_cache);
    if !commit_gap_fill_writes(
        &state.column_fence,
        &state.cache,
        &column.id,
        &epoch,
        &gap_result,
        attribute_to.as_deref(),
    )
    .await
    {
        return;
    }
```

- [ ] **Step 6: column.rs のテストが通ることを確認**

Run: `cd src-tauri && cargo test --lib commands::column:: 2>&1 | tail -10`
Expected: `test result: ok.`(既存テストを含めすべて PASS)

- [ ] **Step 7: ストリーミング受信の失敗するテストを書く(connection.rs)**

`stream/connection.rs` の `mod tests` に追加する(`state_with_account` / `minimal_note` は既存のヘルパー)。

```rust
    #[tokio::test]
    async fn cache_streamed_note_caches_the_note_and_attributes_it_to_the_account() {
        let state = state_with_account("acc1", "u-self");

        cache_streamed_note(&state, "col1", "acc1", &minimal_note("n1")).await;

        assert!(state.cache.get_note("n1").await.unwrap().is_some());
        assert!(state.cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(!state.cache.account_has_note("acc2", "n1").await.unwrap());
    }
```

Run: `cd src-tauri && cargo test --lib stream::connection::tests::cache_streamed_note 2>&1 | tail -10`
Expected: FAIL — `cannot find function cache_streamed_note`

- [ ] **Step 8: ストリーミング受信を変更**

`stream/connection.rs` の `handle_text` の手前に関数を追加する。

```rust
/// ストリーミングで受信したノートをキャッシュへ書き、そのアカウントの取得経路として記録する。
/// 受信したノートはそのアカウント自身の WebSocket から届いた事実なので、`from cache` を含む
/// カラムでも常に記録する(Issue #557)。
async fn cache_streamed_note(state: &AppState, column_id: &str, account_id: &str, note: &Note) {
    let _ = state.cache.cache_note(column_id, note).await;
    crate::attribution::record(&state.cache, account_id, std::slice::from_ref(note)).await;
}
```

`handle_text` の中の次の行を置き換える。

```rust
                let _ = state.cache.cache_note(&column_id, &normalized).await;
```

を

```rust
                cache_streamed_note(&state, &column_id, account_id, &normalized).await;
```

に変更する。

- [ ] **Step 9: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib stream:: 2>&1 | tail -8`
Expected: `test result: ok.`

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/stream/connection.rs
git commit -m "feat(cache): 取得経路ごとにノートのアカウント帰属を記録する (#557)"
```

---

### Task 6: アカウント削除・起動時・バックエンド切り替え

**Files:**
- Modify: `src-tauri/src/commands/account.rs`(`remove_account`)
- Modify: `src-tauri/src/commands/cache_backend.rs`(切り替え後の埋め直し)
- Modify: `src-tauri/src/lib.rs`(起動時の埋め直し)

**Interfaces:**
- Consumes: `NoteCacheStore::forget_account_notes`(Task 1)、`attribution::backfill_all`(Task 4)。

- [ ] **Step 1: アカウント削除で帰属を消す**

`commands/account.rs` の `remove_account` で、`state.settings.delete_account(&account_id)?;` の直前に追加する。

```rust
    // そのアカウントのノート帰属も消す(Issue #557)。失敗しても削除は止めない
    // (残っても、そのアカウント ID が再び使われない限り外部には出ない)。
    if let Err(e) = state.cache.forget_account_notes(&account_id).await {
        log::warn!("failed to forget the note attribution of the removed account: {e}");
    }
```

- [ ] **Step 2: 起動時に埋め直す**

`lib.rs` の `scheduler::spawn(app.handle().clone());` の直後に追加する。

```rust
            // 既存の column_note から、アカウントの帰属を埋め直す(Issue #557)。冪等なので毎起動で実行する。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = handle.try_state::<AppState>() {
                    attribution::backfill_all(&state.settings, &state.cache).await;
                }
            });
```

- [ ] **Step 3: バックエンド切り替え後に埋め直す**

`commands/cache_backend.rs` の `state.cache.swap_backend(new_backend);` の直後、`Ok(())` の前に追加する。

```rust
    // 切り替え先に既存のキャッシュがある場合に備えて、帰属を埋め直す(Issue #557)。
    crate::attribution::backfill_all(&state.settings, &state.cache).await;
```

- [ ] **Step 4: ビルドと全テスト**

Run: `cd src-tauri && cargo build --tests 2>&1 | tail -10`
Expected: エラーなし

Run: `cd src-tauri && cargo test 2>&1 | tail -12`
Expected: `test result: ok.`(`#[ignore]` の実 DB テストは除く)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/account.rs src-tauri/src/commands/cache_backend.rs src-tauri/src/lib.rs
git commit -m "feat(cache): アカウント削除・起動時・バックエンド切り替えで帰属を管理する (#557)"
```

---

### Task 7: ドキュメントと最終検証

**Files:**
- Modify: `CLAUDE.md`(`store/` の説明に1文追加)

- [ ] **Step 1: CLAUDE.md に追記**

`CLAUDE.md` の `store/` の項で、`Cache schema migrations must be idempotent ...` の文の直前に追加する。

```
ノートのアカウント帰属は `note_account(account_id, note_id)`(Issue #557): `note` は全アカウント共通なので、アカウント単位の絞り込み(MCP など)はこれで行う。ストリーミング受信・REST取得・ギャップ埋めの直後に `attribution::record` で記録し、`from cache` を含むカラムの REST取得・ギャップ埋めでは記録しない(グローバルなキャッシュ由来のノートが混ざるため)。記録が欠けると「見えない」側に倒れる設計で、既存データは起動時に `attribution::backfill_all` が `column_note` から埋め直す。
```

- [ ] **Step 2: 静的検査とテスト**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -15`
Expected: 警告・エラーなし(既存の警告は増やさない)

Run: `cd src-tauri && cargo test 2>&1 | tail -8`
Expected: `test result: ok.`

Run: `cd src-tauri && cargo test --lib postgres_ -- --ignored 2>&1 | tail -8`
Expected: `test result: ok.`(Docker 必須。残ったコンテナは `docker ps -a` で確認し `docker rm -fv <id>`)

Run: `cd src-tauri && cargo test --lib mysql_ -- --ignored --test-threads=2 2>&1 | tail -8`
Expected: `test result: ok.`

- [ ] **Step 3: 実データでの確認(コピーに対して、読み取り専用)**

ユーザーが `cargo tauri dev` を起動中の場合、ブランチの内容が実データに対して動く(`note_account` の追加と埋め直しのみで、既存テーブルは変更しない)。実 DB は直接開かず、コピーで確認する。

コピー先はセッションのスクラッチパッド(`<scratchpad>` は作業時のスクラッチパッドの絶対パスに置き換える)。

```bash
sqlite3 -readonly ~/.cache/com.onodai.tsumugi/cache.db ".backup '<scratchpad>/note-account-check.db'"
sqlite3 <scratchpad>/note-account-check.db "SELECT account_id, COUNT(*) FROM note_account GROUP BY account_id;"
sqlite3 <scratchpad>/note-account-check.db "SELECT COUNT(*) FROM note_account WHERE note_id NOT IN (SELECT id FROM note);"
```

Expected: 1つ目はアカウントごとの件数(アカウントのカラム数に応じて非ゼロ)、2つ目は `0`(孤児行なし)。

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: ノートのアカウント帰属(note_account)をCLAUDE.mdに追記 (#557)"
```
