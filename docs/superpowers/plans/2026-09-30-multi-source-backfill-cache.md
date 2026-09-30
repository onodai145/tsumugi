# TQL複数ソースカラムのbackfillキャッシュ優先化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** TQLで複数ソースを `from` に並べたカラムでも、`fetch_backfill` がキャッシュ優先で応答できるようにする（Issue #238）。

**Architecture:** backfill境界をカラム単位のスカラーからソース単位のテーブル `column_source_boundary(column_id, source_idx, oldest_fetched_id)` に移す。取得側は各ソースの取得結果（最古ID／枯渇／失敗）から境界の更新内容を純関数で決め、`truncate` 前のフィルタ通過分を全件キャッシュする。読み出し側は全ソースの境界が揃っているときだけ `max(b_i)` を有効境界として既存の判定に渡す。

**Tech Stack:** Rust（rusqlite / sqlx / sea-query / tauri）。Postgres/MySQL の実DBテストは testcontainers（Docker）。

**Spec:** `docs/superpowers/specs/2026-09-30-multi-source-backfill-cache-design.md`（先に読むこと）

## Global Constraints

- ブランチは作成済みの `feat/238-multi-source-backfill-cache`。`main` では編集・コミットしない。
- コミットメッセージは件名1行のみ（本文・箇条書き禁止）。`git commit -m "件名" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` の形でトレーラーだけ別に付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めてユーザーに報告し、リトライしない。
- `./target/debug/tsumugi` や `cargo run` は実行しない。アプリの起動確認が必要なときはリポジトリルートから `cargo tauri dev` を Xvfb（`WAYLAND_DISPLAY` も unset）+ `dbus-run-session` 越しに使い、自分で起動したプロセスは終了前に PID 指定で kill する（`pkill`/`killall` 禁止）。
- キャッシュスキーマのマイグレーションは冪等で、マーカーテーブルを作らない。「旧構造がまだ存在する」を未移行マーカーにし、`ON CONFLICT DO NOTHING`（MySQL は `INSERT IGNORE`）でコピーしてから `DROP` する。
- Tauri コマンドの署名・イベント・`domain/` の型は変えない（`frontend/src/bindings/tauri.gen.ts` は変化しないはず。`cargo test` の `generates_frontend_bindings` が通ること）。
- `oldest_fetched_id = ""`（空文字）は「そのソースは先頭まで枯渇済み」を表す。ID比較は既存と同じ文字列の辞書順。
- コメント・テスト名は既存コードに合わせて日本語。既存の1行あたりの長さ・コメント密度に合わせる。
- Rustテスト: `cd src-tauri && cargo test`。Postgres/MySQL の実DBテストは `cargo test --lib postgres_ -- --ignored` / `cargo test --lib mysql_ -- --ignored --test-threads=2`（Docker必須。各テストがコンテナを `mem::forget` でリークするので、終わったら `docker ps` で残った `mysql:8.1` / `postgres` コンテナをIDで `docker rm -f <id>`）。

## Review Focus

- **truncate起因の欠落（最優先）:** 2ソースが交互に並び全件フィルタを通るとき、`column_note` に境界以上のノートが全部入っていること。`notes`（20件）だけをキャッシュすると欠落する。→ Task 2 の回帰テスト。
- **ソース1つの取得失敗:** 初回取得で1ソースが失敗したカラムは、そのソースの境界が無いため有効境界が `None` になり、API経由のままであること（他ソースの境界だけで「完全」と主張しない）。→ Task 2 の `effective_boundary` / `plan_boundary_initial` テスト。
- **不連続な `until_id`:** 1ソースだけ `until_id < b_i` のとき、そのソースの境界は延長されず、他ソースは延長されること。→ Task 2 の `plan_boundary_extend` テスト。
- **prune後の境界:** 全ソース行（`""` の行を含む）が生存最古ID（連合ノートでは削除最大ID）まで引き上がり、全滅時は全行が消えること。→ Task 1 の prune テスト。
- **マイグレーション:** 旧 `column_fetch_boundary` の行が `source_idx=0` へ移り旧テーブルが消え、再実行や新テーブル既存行との衝突で値が壊れないこと。→ Task 1 の移行テスト（SQLite / Postgres / MySQL）。
- **`from cache` 併用カラム:** 常に API 経由（境界を読まない・書かない）であること。→ Task 2 の `backfill_cache_eligible` テスト。

---

## File Structure

| ファイル | 役割 |
|---|---|
| `src-tauri/src/store/db.rs` | SQLite スキーマ（新テーブル）と旧テーブルからの移行 |
| `src-tauri/src/store/note_cache.rs` | `NoteCacheBackend` トレイト、`NoteCacheStore` ラッパー、SQLite の prune 連動 |
| `src-tauri/src/store/sqlite_backend.rs` | SQLite バックエンド実装とテスト |
| `src-tauri/src/store/postgres_backend.rs` | Postgres バックエンド実装・スキーマ・移行・実DBテスト |
| `src-tauri/src/store/mysql_backend.rs` | MySQL バックエンド実装・スキーマ・移行・実DBテスト |
| `src-tauri/src/commands/column.rs` | 取得結果の型、境界更新の純関数、`fetch_backfill` / `open_stream_and_fetch` の配線 |
| `src-tauri/src/state.rs` | `BackfillOutcome::FallbackBoundaryUnset` のコメント更新のみ |

Task 1 は store 層だけで完結して既存の `column.rs` を壊さない（`NoteCacheStore` に旧3関数の暫定シムを残す）。Task 2 が `column.rs` を新APIへ移し、シムを削除する。

---

### Task 1: ソース単位の境界ストア（トレイト・3バックエンド・スキーマ・移行・prune）

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`（トレイト、`NoteCacheStore` ラッパー、`delete_matching`）
- Modify: `src-tauri/src/store/db.rs`（`CACHE_SCHEMA`、`migrate_cache`、テスト）
- Modify: `src-tauri/src/store/sqlite_backend.rs`
- Modify: `src-tauri/src/store/postgres_backend.rs`
- Modify: `src-tauri/src/store/mysql_backend.rs`

**Interfaces:**
- Consumes: なし。
- Produces（Task 2 が使う）:
  - `NoteCacheStore::get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>>`（`source_idx` 昇順。行が無ければ空）
  - `NoteCacheStore::replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>`
  - `NoteCacheStore::extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>`
  - 暫定シム（Task 2 で削除）: `NoteCacheStore::{get_fetch_boundary, set_fetch_boundary, extend_fetch_boundary}` は `source_idx=0` だけを見る従来シグネチャのまま。

`cargo test` はクレート全体をコンパイルするので、トレイトを変えたらこの Task の中で3バックエンドすべてを実装し終えてからでないとテストが走らない。ステップはその順に並べてある。

- [ ] **Step 1: テストの下ごしらえ（旧APIテストの機械的な書き換えと、新テストの追加）**

3バックエンドのテストは旧 `set_fetch_boundary` / `get_fetch_boundary` / `extend_fetch_boundary` を直接呼んでいる。各テストモジュールに次のヘルパーを追加し、旧呼び出しを機械的に置き換える。

`src-tauri/src/store/sqlite_backend.rs` の `mod tests` 内（`fn store()` の直後）に追加:

```rust
    fn b(idx: u32, id: &str) -> (u32, String) {
        (idx, id.to_string())
    }

    /// source_idx=0 だけの境界を置き換える(単一ソース時代のテストを新APIへ移すためのヘルパー)。
    async fn set0(s: &SqliteBackend, column_id: &str, id: &str) {
        s.replace_fetch_boundaries(column_id, &[b(0, id)]).await.unwrap();
    }

    async fn extend0(s: &SqliteBackend, column_id: &str, id: &str) {
        s.extend_fetch_boundaries(column_id, &[b(0, id)]).await.unwrap();
    }

    async fn b0(s: &SqliteBackend, column_id: &str) -> Option<String> {
        s.get_fetch_boundaries(column_id).await.unwrap().into_iter().find(|(i, _)| *i == 0).map(|(_, v)| v)
    }
```

`postgres_backend.rs` / `mysql_backend.rs` の `mod tests` 内（`backend()` ヘルパーの直後）にも、型を `PostgresBackend` / `MySqlBackend` に変えた同じ4関数（`b`, `set0`, `extend0`, `b0`）を追加する。

その後、3ファイルそれぞれに次の `sed` を1回ずつ実行する（実装本体には旧名の呼び出し `x.set_fetch_boundary(` 形式が存在しないので、テストだけが書き換わる。`fn set_fetch_boundary(&self` のような定義は正規表現に一致しない）:

```bash
cd src-tauri/src/store
for f in sqlite_backend.rs postgres_backend.rs mysql_backend.rs; do
  sed -i -E \
    -e 's/([a-z_]+)\.set_fetch_boundary\(([^)]+)\)\.await\.unwrap\(\)/set0(\&\1, \2).await/g' \
    -e 's/([a-z_]+)\.extend_fetch_boundary\(([^)]+)\)\.await\.unwrap\(\)/extend0(\&\1, \2).await/g' \
    -e 's/([a-z_]+)\.get_fetch_boundary\(([^)]+)\)\.await\.unwrap\(\)/b0(\&\1, \2).await/g' \
    "$f"
done
grep -n "_fetch_boundary(" sqlite_backend.rs postgres_backend.rs mysql_backend.rs
```

最後の `grep` に残るのは `async fn get_fetch_boundary` / `set_fetch_boundary` / `extend_fetch_boundary` の**定義**行だけ（Step 3〜6 で置き換える）。テスト内の呼び出しが残っていたら手で直す。

次に、`sqlite_backend.rs` の `mod tests` 末尾（`insert_legacy_row` の直前）に新しいテストを追加する:

```rust
    #[tokio::test]
    async fn fetch_boundaries_are_kept_per_source_and_per_column() {
        let s = store();
        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());

        s.replace_fetch_boundaries("col1", &[b(1, "n200"), b(0, "n100")]).await.unwrap();
        s.replace_fetch_boundaries("col2", &[b(0, "m1")]).await.unwrap();

        // source_idx 昇順で返り、別カラムには影響しない
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n100"), b(1, "n200")]);
        assert_eq!(s.get_fetch_boundaries("col2").await.unwrap(), vec![b(0, "m1")]);
    }

    #[tokio::test]
    async fn replace_fetch_boundaries_drops_sources_missing_from_entries() {
        let s = store();
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "n2")]).await.unwrap();

        s.replace_fetch_boundaries("col1", &[b(0, "n5")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n5")]);

        s.replace_fetch_boundaries("col1", &[]).await.unwrap();
        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn extend_fetch_boundaries_moves_each_source_older_only_and_inserts_absent() {
        let s = store();
        s.replace_fetch_boundaries("col1", &[b(0, "n500"), b(1, "n500")]).await.unwrap();

        s.extend_fetch_boundaries("col1", &[b(0, "n300"), b(1, "n800"), b(2, "n100")]).await.unwrap();

        // 0: 古い方へ延長 / 1: 新しい値は無視 / 2: 行が無ければ挿入
        assert_eq!(
            s.get_fetch_boundaries("col1").await.unwrap(),
            vec![b(0, "n300"), b(1, "n500"), b(2, "n100")]
        );
    }

    #[tokio::test]
    async fn extend_fetch_boundaries_accepts_empty_string_as_exhausted() {
        let s = store();
        s.replace_fetch_boundaries("col1", &[b(0, "n500")]).await.unwrap();

        s.extend_fetch_boundaries("col1", &[b(0, "")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "")]);

        // 枯渇済み("")は、後から通常のIDで延長しても戻らない
        s.extend_fetch_boundaries("col1", &[b(0, "n1")]).await.unwrap();
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "")]);
    }

    #[tokio::test]
    async fn clear_column_notes_removes_every_source_row_of_the_column_only() {
        let s = store();
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "n2")]).await.unwrap();
        s.replace_fetch_boundaries("col2", &[b(0, "m1")]).await.unwrap();

        s.clear_column_notes("col1").await.unwrap();

        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
        assert_eq!(s.get_fetch_boundaries("col2").await.unwrap(), vec![b(0, "m1")]);
    }

    #[tokio::test]
    async fn clear_all_fetch_boundaries_removes_rows_of_every_source_and_column() {
        let s = store();
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "n2")]).await.unwrap();
        s.replace_fetch_boundaries("col2", &[b(0, "m1")]).await.unwrap();

        s.clear_all_fetch_boundaries().await.unwrap();

        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
        assert!(s.get_fetch_boundaries("col2").await.unwrap().is_empty());
    }

    /// prune は column_note がソースの帰属を持たないため、そのカラムの全ソース行を
    /// 一律に生存最古IDまで引き上げる(枯渇済みの "" も含む)。
    #[tokio::test]
    async fn prune_raises_every_source_boundary_of_the_column() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, ""), b(2, "n2")]).await.unwrap();

        let deleted = s.prune(2, 0, 0).await.unwrap(); // 最古の n1 が削除される
        assert_eq!(deleted, 1);

        assert_eq!(
            s.get_fetch_boundaries("col1").await.unwrap(),
            vec![b(0, "n2"), b(1, "n2"), b(2, "n2")]
        );
    }

    #[tokio::test]
    async fn prune_removes_every_source_row_when_column_fully_pruned() {
        let s = store();
        let now = crate::store::note_cache::now_epoch();
        let one_day = 86_400;
        s.cache_notes("col1", &[note("old", now - 40 * one_day)]).await.unwrap();
        s.replace_fetch_boundaries("col1", &[b(0, "old"), b(1, "")]).await.unwrap();

        let deleted = s.prune(0, 30, 0).await.unwrap();
        assert_eq!(deleted, 1);

        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }
```

`src-tauri/src/store/db.rs` の `mod tests` 末尾に移行テストを追加する:

```rust
    #[test]
    fn migrate_fetch_boundary_moves_legacy_rows_to_source_zero_and_is_idempotent() {
        let conn = open_cache_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE column_fetch_boundary (column_id TEXT PRIMARY KEY, oldest_fetched_id TEXT NOT NULL);
             INSERT INTO column_fetch_boundary VALUES ('c1', 'n100'), ('c2', 'n200');",
        )
        .unwrap();

        migrate_fetch_boundary(&conn).unwrap();

        let rows: Vec<(String, i64, String)> = conn
            .prepare("SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary ORDER BY column_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![("c1".to_string(), 0, "n100".to_string()), ("c2".to_string(), 0, "n200".to_string())]
        );
        let legacy: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='column_fetch_boundary'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(legacy, 0, "旧テーブルは DROP される");

        // 旧テーブルが無い状態での再実行は何もしない(冪等)
        migrate_fetch_boundary(&conn).unwrap();
    }

    #[test]
    fn migrate_fetch_boundary_does_not_overwrite_rows_already_in_new_table() {
        let conn = open_cache_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO column_source_boundary VALUES ('c1', 0, 'n50');
             CREATE TABLE column_fetch_boundary (column_id TEXT PRIMARY KEY, oldest_fetched_id TEXT NOT NULL);
             INSERT INTO column_fetch_boundary VALUES ('c1', 'n100');",
        )
        .unwrap();

        migrate_fetch_boundary(&conn).unwrap();

        let v: String = conn
            .query_row("SELECT oldest_fetched_id FROM column_source_boundary WHERE column_id='c1' AND source_idx=0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "n50", "中断後の再実行で、移行後に更新された値を旧値で潰さない");
    }
```

Postgres の `mod tests` 末尾（`#[ignore]` テストの並びの最後）に追加:

```rust
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

        // prune: 全ソース行が生存最古IDまで引き上がる
        s.replace_fetch_boundaries("col1", &[b(0, "n1"), b(1, "")]).await.unwrap();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        assert_eq!(s.prune(2, 0, 0).await.unwrap(), 1);
        assert_eq!(s.get_fetch_boundaries("col1").await.unwrap(), vec![b(0, "n2"), b(1, "n2")]);

        s.clear_column_notes("col1").await.unwrap();
        assert!(s.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    /// 旧 `column_fetch_boundary` を持つ既存インストールが `ensure_schema` で
    /// `column_source_boundary` (source_idx=0) へ移行され、旧テーブルが消え、再実行しても壊れないこと。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_migrates_legacy_fetch_boundary_table() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();

        sqlx::query("CREATE TABLE column_fetch_boundary (column_id TEXT PRIMARY KEY, oldest_fetched_id TEXT NOT NULL)")
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
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'column_fetch_boundary'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(legacy, 0, "旧テーブルは DROP される");

        // 移行後に更新した値は ensure_schema を再実行しても戻らない
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
```

MySQL の `mod tests` 末尾にも同じ2テストを追加する。差分は `backend()` が `MySqlBackend`、`Postgres` → `Mysql`、接続文字列が `mysql://root@127.0.0.1:{port}/test`、ポートが `3306`、`MySqlPoolOptions`、旧テーブル作成が `` CREATE TABLE column_fetch_boundary (column_id VARCHAR(64) PRIMARY KEY, oldest_fetched_id TEXT NOT NULL) ``、`information_schema.tables` の条件が `table_name = 'column_fetch_boundary' AND table_schema = 'test'`、行の型が `(String, i32, String)`（`INT` は `i32`）である点だけ:

```rust
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
```

- [ ] **Step 2: テストがコンパイルエラーで失敗することを確認**

Run: `cd src-tauri && cargo test --lib store:: 2>&1 | tail -30`
Expected: FAIL（`no method named replace_fetch_boundaries` / `migrate_fetch_boundary` not found などのコンパイルエラー）。

- [ ] **Step 3: トレイトと `NoteCacheStore` ラッパー、SQLite の prune 連動を変更（`note_cache.rs`）**

`NoteCacheBackend` トレイトの旧3関数を新3関数に置き換える:

```rust
    async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>>;
    async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>;
    async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>;
```

（`clear_all_fetch_boundaries` はそのまま残す。）

`NoteCacheStore` の `get_fetch_boundary` / `set_fetch_boundary` / `extend_fetch_boundary` の3メソッド（doc comment 込み）を、次の新3メソッドと暫定シムに置き換える:

```rust
    /// カラムの全ソース境界を (source_idx, oldest_fetched_id) で返す(source_idx昇順)。
    /// 行が無ければ空。`""` は「そのソースは先頭まで枯渇済み」(Issue #238)。
    pub async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>> {
        self.backend().get_fetch_boundaries(column_id).await
    }

    /// カラムの境界を entries で置き換える(初回REST取得時に使う)。既存行は全削除してから挿入する。
    /// entries に含まれないソースは未確定になる。
    pub async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        self.backend().replace_fetch_boundaries(column_id, entries).await
    }

    /// 各 (source_idx, id) について境界を古い方向へのみ延長する(単調性を保証)。
    /// 行が無ければ挿入し、既存値の方が古ければ何もしない。
    pub async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        self.backend().extend_fetch_boundaries(column_id, entries).await
    }

    // ---- 暫定シム: Task 2 で column.rs を新APIへ移したら削除する ----
    pub async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        Ok(self.get_fetch_boundaries(column_id).await?.into_iter().find(|(i, _)| *i == 0).map(|(_, b)| b))
    }

    pub async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.replace_fetch_boundaries(column_id, &[(0, new_oldest_id.to_string())]).await
    }

    pub async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.extend_fetch_boundaries(column_id, &[(0, new_oldest_id.to_string())]).await
    }
```

`delete_matching`（同ファイル）の SQL とコメントを新テーブル名へ:

```rust
                conn.execute(
                    "UPDATE column_source_boundary SET oldest_fetched_id = ?2
                     WHERE column_id = ?1 AND oldest_fetched_id < ?2",
                    params![column_id, candidate],
                )?;
```

```rust
                conn.execute(
                    "DELETE FROM column_source_boundary WHERE column_id = ?1",
                    params![column_id],
                )?;
```

doc comment の `column_fetch_boundary` も `column_source_boundary` に直し、「カラムの全ソース行に同じ引き上げを適用する（`column_note` はソースの帰属を持たないため）」の一文を足す。

- [ ] **Step 4: スキーマと移行（`db.rs`）**

`CACHE_SCHEMA` の旧テーブル定義（`-- カラムごとの「これより新しいノートは…` のコメントと `CREATE TABLE IF NOT EXISTS column_fetch_boundary (...)`）を次に置き換える:

```sql
-- ソースごとの「これより新しいノートはAPI取得済みで完全」境界（Issue #228 / #238）。
-- source_idx は TQL の from 節内の位置。oldest_fetched_id = '' はそのソースが先頭まで枯渇済み。
CREATE TABLE IF NOT EXISTS column_source_boundary (
    column_id         TEXT NOT NULL,
    source_idx        INTEGER NOT NULL,
    oldest_fetched_id TEXT NOT NULL,
    PRIMARY KEY (column_id, source_idx)
);
```

`migrate_cache` の `migrate_instance_table(conn)?;` の直後に呼び出しを追加:

```rust
    migrate_fetch_boundary(conn)?;
```

`migrate_instance_table` の後ろに関数を追加:

```rust
/// Issue #238: 単一ソース境界 `column_fetch_boundary` を `column_source_boundary`
/// (source_idx=0)へ移す(一度きり)。旧テーブルが残っていることを「未移行」のマーカーとして使う。
/// #228 の仕様で境界が付くのは単一ソースのカラムだけなので、source_idx=0 への写像は正確。
/// コピーは競合時に何もしない(`ON CONFLICT DO NOTHING`)ため、途中で中断して再実行しても
/// 移行後に更新された値を旧値で潰さない。SQLite の `INSERT ... SELECT ... ON CONFLICT` は
/// 構文解析の曖昧さを避けるため `WHERE true` が必要。
fn migrate_fetch_boundary(conn: &Connection) -> Result<()> {
    let legacy: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='column_fetch_boundary'",
        [],
        |r| r.get(0),
    )?;
    if legacy == 0 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id)
         SELECT column_id, 0, oldest_fetched_id FROM column_fetch_boundary WHERE true
         ON CONFLICT(column_id, source_idx) DO NOTHING;
         DROP TABLE column_fetch_boundary;",
    )?;
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 5: SQLite バックエンド実装（`sqlite_backend.rs`）**

`clear_column_notes` の2つ目の `DELETE` のテーブル名を `column_source_boundary` に、`clear_all_fetch_boundaries` の `DELETE FROM column_fetch_boundary` を `DELETE FROM column_source_boundary` に変える。旧3関数（`get_fetch_boundary` / `set_fetch_boundary` / `extend_fetch_boundary`）を次の3関数に置き換える:

```rust
    async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<(u32, String)>> {
            let guard = conn.lock().unwrap();
            let mut stmt = guard.prepare(
                "SELECT source_idx, oldest_fetched_id FROM column_source_boundary
                 WHERE column_id = ?1 ORDER BY source_idx",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![column_id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let entries = entries.to_vec();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            tx.execute("DELETE FROM column_source_boundary WHERE column_id = ?1", rusqlite::params![column_id])?;
            for (idx, id) in &entries {
                tx.execute(
                    "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![column_id, idx, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let entries = entries.to_vec();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            for (idx, id) in &entries {
                tx.execute(
                    "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES (?1, ?2, ?3)
                     ON CONFLICT(column_id, source_idx) DO UPDATE SET
                        oldest_fetched_id = MIN(oldest_fetched_id, excluded.oldest_fetched_id)",
                    rusqlite::params![column_id, idx, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }
```

`use rusqlite::{Connection, OptionalExtension};` の `OptionalExtension` が他で使われていなければ未使用警告になるので、その場合だけ import から外す（`grep -n "optional()" src/store/sqlite_backend.rs` で確認）。

- [ ] **Step 6: Postgres バックエンド（`postgres_backend.rs`）**

(a) `ensure_schema` の `column_fetch_boundary` テーブル作成（`let column_fetch_boundary = Table::create()...` と `pool.execute(...)` の2文）を次に置き換え、直後（`Ok(())` の前）に移行呼び出しを足す:

```rust
    let column_source_boundary = Table::create()
        .table(ColumnSourceBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnSourceBoundaryTable::ColumnId).text().not_null())
        .col(ColumnDef::new(ColumnSourceBoundaryTable::SourceIdx).integer().not_null())
        .col(ColumnDef::new(ColumnSourceBoundaryTable::OldestFetchedId).text().not_null())
        .primary_key(
            Index::create()
                .col(ColumnSourceBoundaryTable::ColumnId)
                .col(ColumnSourceBoundaryTable::SourceIdx),
        )
        .build(PostgresQueryBuilder);
    pool.execute(sqlx::AssertSqlSafe(column_source_boundary)).await?;

    migrate_fetch_boundary(pool).await?;
```

(b) `ColumnFetchBoundaryTable` の `Iden` enum を置き換える:

```rust
#[derive(sea_query::Iden)]
enum ColumnSourceBoundaryTable {
    #[iden = "column_source_boundary"]
    Table,
    ColumnId,
    SourceIdx,
    OldestFetchedId,
}
```

(c) `migrate_instance_columns` の直後に移行関数を追加:

```rust
/// Issue #238: 単一ソース境界 `column_fetch_boundary` を `column_source_boundary`
/// (source_idx=0)へ移す(一度きり)。旧テーブルが残っていることを「未移行」のマーカーとして使う。
/// コピーは競合時に何もしないので、再実行しても移行後に更新された値を旧値で潰さない。
/// コピーとDROPは1トランザクション。
async fn migrate_fetch_boundary(pool: &sqlx::PgPool) -> Result<()> {
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables
         WHERE table_schema = current_schema() AND table_name = 'column_fetch_boundary'",
    )
    .fetch_one(pool)
    .await?;
    if legacy == 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id)
         SELECT column_id, 0, oldest_fetched_id FROM column_fetch_boundary
         ON CONFLICT (column_id, source_idx) DO NOTHING",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE column_fetch_boundary").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
```

(d) `impl NoteCacheBackend for PostgresBackend` の `clear_column_notes` 内 `DELETE FROM column_fetch_boundary WHERE column_id = $1` と `clear_all_fetch_boundaries` 内の `DELETE FROM column_fetch_boundary`、`delete_matching_ids` 内の `UPDATE column_fetch_boundary ...` / `DELETE FROM column_fetch_boundary ...` のテーブル名を `column_source_boundary` に変える（UPDATE/DELETE の条件はそのまま。複数ソース行に効く）。

(e) 旧3関数を次に置き換える:

```rust
    async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>> {
        let rows: Vec<(i32, String)> = sqlx::query_as(
            "SELECT source_idx, oldest_fetched_id FROM column_source_boundary
             WHERE column_id = $1 ORDER BY source_idx",
        )
        .bind(column_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(i, b)| (i as u32, b)).collect())
    }

    async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM column_source_boundary WHERE column_id = $1")
            .bind(column_id)
            .execute(&mut *tx)
            .await?;
        for (idx, id) in entries {
            sqlx::query(
                "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES ($1,$2,$3)",
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

    async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        // LEAST(...)による大小比較も、load_cached_beforeと同様Postgresのデフォルトテキスト
        // 照合順序がSQLiteのバイナリバイト比較と一致することに依存している。
        let mut tx = self.pool.begin().await?;
        for (idx, id) in entries {
            sqlx::query(
                "INSERT INTO column_source_boundary (column_id, source_idx, oldest_fetched_id) VALUES ($1,$2,$3)
                 ON CONFLICT (column_id, source_idx) DO UPDATE SET
                    oldest_fetched_id = LEAST(column_source_boundary.oldest_fetched_id, excluded.oldest_fetched_id)",
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
```

- [ ] **Step 7: MySQL バックエンド（`mysql_backend.rs`）**

(a) `ensure_schema` の `column_fetch_boundary` 作成部を置き換え、直後に移行呼び出し:

```rust
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
```

(b) `Iden` enum:

```rust
#[derive(sea_query::Iden)]
enum ColumnSourceBoundaryTable {
    #[iden = "column_source_boundary"]
    Table, ColumnId, SourceIdx, OldestFetchedId,
}
```

(c) `migrate_instance_columns` の直後に:

```rust
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
```

(d) `clear_column_notes` / `clear_all_fetch_boundaries` / `delete_matching_ids_with_chunk_size` 内の `column_fetch_boundary` を `column_source_boundary` に置換（条件はそのまま）。

(e) 旧3関数を次に置き換える:

```rust
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
```

- [ ] **Step 8: SQLite / 全体テストを実行して通ることを確認**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -30`
Expected: PASS（`#[ignore]` の実DBテストは実行されない。警告が新規に出ていないこと）。

- [ ] **Step 9: Postgres / MySQL の実DBテストを実行して通ることを確認（Docker 必須）**

Run: `cd src-tauri && cargo test --lib postgres_ -- --ignored 2>&1 | tail -30`
Expected: PASS（`fetch_boundaries_multi_source_roundtrip_extend_and_prune` と `ensure_schema_migrates_legacy_fetch_boundary_table` を含む）。

Run: `cd src-tauri && cargo test --lib mysql_ -- --ignored --test-threads=2 2>&1 | tail -30`
Expected: PASS。

実行後、`docker ps` で残った `mysql:8.1` / `postgres` コンテナをID指定で `docker rm -f <id>` する。

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/store/
git commit -m "feat: backfill境界をソース単位のテーブルへ移す (#238)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 取得側と読み出し側の配線（`commands/column.rs`）

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/store/note_cache.rs`（暫定シムの削除のみ）
- Modify: `src-tauri/src/state.rs`（コメントのみ）

**Interfaces:**
- Consumes（Task 1）: `NoteCacheStore::{get_fetch_boundaries, replace_fetch_boundaries, extend_fetch_boundaries}`。
- Produces（同ファイル内で使う、非公開）:
  - `enum SourceOutcome { Fetched(String), Exhausted, Failed }`
  - `fn source_outcome_from_page(raw: &[Note]) -> SourceOutcome`
  - `fn plan_boundary_initial(outcomes: &[SourceOutcome]) -> Vec<(u32, String)>`
  - `fn plan_boundary_extend(prev: &HashMap<u32, String>, until_id: &str, outcomes: &[SourceOutcome]) -> Vec<(u32, String)>`
  - `fn effective_boundary(boundaries: &HashMap<u32, String>, source_count: usize) -> Option<String>`
  - `fn backfill_cache_eligible(resolved: &ResolvedSources) -> bool`
  - `fn split_display_and_cacheable(filtered: Vec<Note>, use_cache: bool) -> (Vec<Note>, Vec<Note>)`
  - `struct FilteredFetch { notes, cacheable, source_outcomes }`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `mod tests` 末尾（最後の `}` の直前、`cache_backfill_page_some_when_within_boundary_and_enough_notes` の後）に追加する:

```rust
    fn fetched(id: &str) -> SourceOutcome {
        SourceOutcome::Fetched(id.to_string())
    }

    fn bmap(entries: &[(u32, &str)]) -> std::collections::HashMap<u32, String> {
        entries.iter().map(|(i, b)| (*i, b.to_string())).collect()
    }

    #[test]
    fn source_outcome_from_page_uses_min_id_and_flags_empty_as_exhausted() {
        // 最古は created_at ではなく id の辞書順の最小(境界比較が id 基準のため)
        let raw = vec![note("n3", 10), note("n1", 30), note("n2", 20)];
        assert_eq!(source_outcome_from_page(&raw), fetched("n1"));
        assert_eq!(source_outcome_from_page(&[]), SourceOutcome::Exhausted);
    }

    #[test]
    fn plan_boundary_initial_skips_failed_sources_and_maps_exhausted_to_empty_string() {
        let outcomes = [fetched("n5"), SourceOutcome::Failed, SourceOutcome::Exhausted];
        assert_eq!(plan_boundary_initial(&outcomes), vec![(0, "n5".to_string()), (2, String::new())]);
        assert!(plan_boundary_initial(&[SourceOutcome::Failed]).is_empty());
        assert!(plan_boundary_initial(&[]).is_empty());
    }

    #[test]
    fn plan_boundary_extend_extends_only_contiguous_sources() {
        // source0: 境界n500, source1: 境界n300。until_id=n400 なので source0 は不連続(400<500)、
        // source1 は連続(400>=300)。連続なソースだけ延長する。
        let prev = bmap(&[(0, "n500"), (1, "n300")]);
        let outcomes = [fetched("n380"), fetched("n350")];
        assert_eq!(plan_boundary_extend(&prev, "n400", &outcomes), vec![(1, "n350".to_string())]);
    }

    #[test]
    fn plan_boundary_extend_skips_failed_and_unknown_sources_and_extends_exhausted_to_empty() {
        let prev = bmap(&[(0, "n100"), (2, "n100")]); // source1 は境界未確定
        let outcomes = [SourceOutcome::Failed, fetched("n50"), SourceOutcome::Exhausted];
        // 0: 失敗 / 1: 境界が無く連続性を検証できない / 2: 枯渇 → ""
        assert_eq!(plan_boundary_extend(&prev, "n200", &outcomes), vec![(2, String::new())]);
    }

    #[test]
    fn plan_boundary_extend_treats_until_id_equal_to_boundary_as_contiguous() {
        let prev = bmap(&[(0, "n300")]);
        assert_eq!(plan_boundary_extend(&prev, "n300", &[fetched("n250")]), vec![(0, "n250".to_string())]);
    }

    #[test]
    fn effective_boundary_is_max_and_requires_every_source() {
        assert_eq!(effective_boundary(&bmap(&[(0, "n100"), (1, "n300")]), 2), Some("n300".to_string()));
        // 枯渇済み("")は有効境界を塞がない
        assert_eq!(effective_boundary(&bmap(&[(0, ""), (1, "n300")]), 2), Some("n300".to_string()));
        assert_eq!(effective_boundary(&bmap(&[(0, ""), (1, "")]), 2), Some(String::new()));
        // 1ソースでも境界が無ければ未確定
        assert_eq!(effective_boundary(&bmap(&[(0, "n100")]), 2), None);
        assert_eq!(effective_boundary(&bmap(&[(1, "n100")]), 2), None);
        // 余分な行(ソース数より大きい idx)は無視する
        assert_eq!(effective_boundary(&bmap(&[(0, "n100"), (5, "n900")]), 1), Some("n100".to_string()));
        assert_eq!(effective_boundary(&bmap(&[]), 0), None);
    }

    #[test]
    fn backfill_cache_eligible_requires_api_sources_and_no_cache_source() {
        let mk = |kinds: Vec<ColumnKind>, use_cache: bool| ResolvedSources {
            kinds,
            use_cache,
            filter: CompiledFilter::PassAll,
        };
        assert!(backfill_cache_eligible(&mk(vec![ColumnKind::Home], false)));
        assert!(backfill_cache_eligible(&mk(vec![ColumnKind::Home, ColumnKind::Local], false)));
        assert!(!backfill_cache_eligible(&mk(vec![ColumnKind::Home, ColumnKind::Local], true)));
        assert!(!backfill_cache_eligible(&mk(vec![], true)));
    }

    #[test]
    fn split_display_and_cacheable_dedupes_sorts_and_truncates_only_the_display() {
        let filtered: Vec<Note> = (1..=25).map(|i| note(&format!("n{i:02}"), i as i64)).collect();
        let mut with_dup = filtered.clone();
        with_dup.push(note("n25", 25)); // 複数ソースに同じノートが跨る場合

        let (display, cacheable) = split_display_and_cacheable(with_dup.clone(), false);
        assert_eq!(display.len(), INITIAL_LIMIT as usize);
        assert_eq!(display[0].id, "n25"); // created_at 降順
        assert_eq!(cacheable.len(), 25); // 重複除去済み・truncateしない

        // `from cache` を含むカラムは従来どおり truncate 後だけをキャッシュする
        let (display, cacheable) = split_display_and_cacheable(with_dup, true);
        assert_eq!(cacheable.len(), display.len());
    }

    /// truncate 起因の欠落の回帰テスト: 2ソースが交互に並び全件フィルタを通るとき、
    /// 初回取得後の境界 E 以上のノートが column_note に全部入っていなければならない。
    /// 画面へ返す20件だけをキャッシュすると E 以上のノートが欠落する。
    #[tokio::test]
    async fn caching_the_full_filtered_set_keeps_every_note_at_or_above_the_boundary() {
        // source A: 偶数 n02..n40、source B: 奇数 n01..n39(各20件、id と created_at は同順)
        let mut all: Vec<Note> = (1..=40).map(|i| note(&format!("n{i:02}"), i as i64)).collect();
        all.reverse();
        let outcomes = [fetched("n02"), fetched("n01")];
        let boundaries = bmap(&plan_boundary_initial(&outcomes).iter().map(|(i, b)| (*i, b.as_str())).collect::<Vec<_>>());
        let e = effective_boundary(&boundaries, 2).unwrap();
        assert_eq!(e, "n02");

        let (display, cacheable) = split_display_and_cacheable(all, false);
        assert_eq!(display.len(), 20);

        // 全件キャッシュ: E 以上(n02..n40 の39件)がすべて取り出せる
        let store = cache_with(&cacheable).await;
        let got = store.load_cached_before("col1", "n99", 100).await.unwrap();
        let at_or_above: Vec<&str> = got.iter().map(|n| n.id.as_str()).filter(|id| *id >= e.as_str()).collect();
        assert_eq!(at_or_above.len(), 39);

        // 対比: 画面へ返す20件だけをキャッシュすると、境界 E が完全と主張する範囲に欠落が出る
        let store = cache_with(&display).await;
        let got = store.load_cached_before("col1", "n99", 100).await.unwrap();
        assert!(got.iter().filter(|n| n.id.as_str() >= e.as_str()).count() < 39);
    }
```

- [ ] **Step 2: テストがコンパイルエラーで失敗することを確認**

Run: `cd src-tauri && cargo test --lib commands::column 2>&1 | tail -30`
Expected: FAIL（`SourceOutcome` / `plan_boundary_initial` などが未定義のコンパイルエラー）。

- [ ] **Step 3: 型・純関数を追加し `FilteredFetch` と `fetch_and_filter_multi` を書き換える**

`FilteredFetch` の定義とそのdoc comment（`/// \`fetch_and_filter_multi\` の戻り値。…` から `struct FilteredFetch { … }` まで）を次に置き換える:

```rust
/// 1ソースのREST取得結果。backfill境界の更新内容を決めるのに使う(Issue #238)。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceOutcome {
    /// 取得成功。生APIレスポンスの最古ID(id辞書順の最小)。フィルタ適用前の値で、
    /// フィルタで末尾が弾かれても「実際にはもっと深くAPIを見ている」事実を取り逃さない。
    Fetched(String),
    /// 取得成功だが0件(それより古いノートは無い)。境界は `""`(枯渇済み)にする。
    Exhausted,
    /// 取得失敗、またはREST取得できない種別。境界は更新しない。
    Failed,
}

/// `fetch_and_filter_multi` の戻り値。
struct FilteredFetch {
    /// 画面へ返す分(重複除去・created_at降順・INITIAL_LIMIT件へtruncate済み)。
    notes: Vec<Note>,
    /// キャッシュする分。境界は「ソースごとのRESTページ全体が column_note に入っている」ことを
    /// 前提に進めるため、複数ソースではtruncate前の重複除去済みフィルタ通過分を全件入れる。
    /// `from cache` を含むカラムは境界の対象外なので従来どおりtruncate後(`notes`と同内容)。
    cacheable: Vec<Note>,
    /// `resolved.kinds` と同じ並びの、ソースごとの取得結果。
    source_outcomes: Vec<SourceOutcome>,
}

/// 1ページ分の生レスポンスから `SourceOutcome` を決める。
fn source_outcome_from_page(raw: &[Note]) -> SourceOutcome {
    // 境界の比較は全て id の辞書順で行うため、ここも id 基準で最古を選ぶ(Issue #228)。
    match raw.iter().map(|n| n.id.as_str()).min() {
        Some(id) => SourceOutcome::Fetched(id.to_string()),
        None => SourceOutcome::Exhausted,
    }
}

/// 初回取得(`until_id` 無し)で境界へ書く内容。失敗したソースは行を作らない(未確定のまま)。
fn plan_boundary_initial(outcomes: &[SourceOutcome]) -> Vec<(u32, String)> {
    outcomes
        .iter()
        .enumerate()
        .filter_map(|(i, o)| match o {
            SourceOutcome::Fetched(id) => Some((i as u32, id.clone())),
            SourceOutcome::Exhausted => Some((i as u32, String::new())),
            SourceOutcome::Failed => None,
        })
        .collect()
}

/// `fetch_backfill` のAPI取得後に境界へ延長する内容。ソースごとに、既存の境界があり
/// `until_id >= b_i`(=今回の取得範囲が既存の完全範囲と連続)なときだけ延長する。
/// 不連続なソース(例: fillRemainingGap が gap marker の targetId まで遡った場合)や
/// 境界未確定のソースは、間の未検証の隙間を「完全」と誤認しないよう更新しない。
fn plan_boundary_extend(
    prev: &std::collections::HashMap<u32, String>,
    until_id: &str,
    outcomes: &[SourceOutcome],
) -> Vec<(u32, String)> {
    plan_boundary_initial(outcomes)
        .into_iter()
        .filter(|(i, _)| prev.get(i).is_some_and(|b| until_id >= b.as_str()))
        .collect()
}

/// 全ソースの境界が揃っているときだけ、カラム全体で完全な範囲 `id > E` の `E = max(b_i)` を返す。
/// 1ソースでも境界が無ければ None(未確定)。`""`(枯渇済み)は他ソースの境界より小さいので塞がない。
fn effective_boundary(boundaries: &std::collections::HashMap<u32, String>, source_count: usize) -> Option<String> {
    if source_count == 0 {
        return None;
    }
    let mut max: Option<&String> = None;
    for i in 0..source_count as u32 {
        let b = boundaries.get(&i)?;
        max = Some(match max {
            Some(m) if m >= b => m,
            _ => b,
        });
    }
    max.cloned()
}

/// backfill のキャッシュ優先経路の対象か。`from cache` を含むカラムは、`search_cache` が
/// グローバルな note テーブルを読み `column_note` だけでは API 経路と同じ結果を再現できないため対象外。
fn backfill_cache_eligible(resolved: &ResolvedSources) -> bool {
    !resolved.use_cache && !resolved.kinds.is_empty()
}

/// 重複除去・created_at降順ソート済みのフィルタ通過ノートから、画面へ返す分と
/// キャッシュする分を決める。詳細は `FilteredFetch::cacheable` を参照。
fn split_display_and_cacheable(mut filtered: Vec<Note>, use_cache: bool) -> (Vec<Note>, Vec<Note>) {
    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let display: Vec<Note> = filtered.iter().take(INITIAL_LIMIT as usize).cloned().collect();
    let cacheable = if use_cache { display.clone() } else { filtered };
    (display, cacheable)
}
```

`fetch_and_filter_multi` 全体（doc comment 込み、`async fn fetch_and_filter_multi(` から閉じ括弧まで）を次に置き換える:

```rust
/// 解決済みソース群から REST 初期/過去ページを取得し、id重複除去+created_at降順マージの上、
/// フィルタ/ミュートを適用する。`cache` ソースが含まれる場合はローカルSQLite検索も合成する。
/// 個別ソースの取得失敗は他ソースの結果を活かすため無視する（TQL§複数ソースは OR 合成のため）。
/// ただし失敗は `source_outcomes` に `Failed` として残し、backfill境界を進めないようにする。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    let mut all: Vec<Note> = Vec::new();
    let mut source_outcomes: Vec<SourceOutcome> = Vec::with_capacity(resolved.kinds.len());

    if !resolved.kinds.is_empty() {
        let client = state.client_for(account_id)?;
        for k in &resolved.kinds {
            let outcome = match k.rest_request(INITIAL_LIMIT, until_id) {
                None => SourceOutcome::Failed,
                Some((endpoint, body)) => match fetch_notes(&client, endpoint, &body).await {
                    Ok(raw) => {
                        let outcome = source_outcome_from_page(&raw);
                        all.extend(raw);
                        outcome
                    }
                    Err(_) => SourceOutcome::Failed,
                },
            };
            source_outcomes.push(outcome);
        }
    }

    if resolved.use_cache {
        let sql_ctx = sql::SqlCtx {
            my_ids: state.eval_context().my_user_ids.into_iter().collect(),
            following_ids: None,
        };
        let expr = match &resolved.filter {
            CompiledFilter::Tql(e) => Some(e),
            _ => None,
        };
        let where_sql = match expr {
            Some(e) => sql::build_where(e, &sql_ctx).map_err(Error::Invalid)?,
            None => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
        };
        if let Ok(cached) = state.cache.search_cache(&where_sql, until_id, INITIAL_LIMIT).await {
            all.extend(cached);
        }
    }

    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let filtered: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
                && !state.is_word_muted(account_id, n)
        })
        .collect();

    let (notes, cacheable) = split_display_and_cacheable(filtered, resolved.use_cache);
    Ok(FilteredFetch { notes, cacheable, source_outcomes })
}
```

- [ ] **Step 4: `open_stream_and_fetch` を新APIへ**

```rust
    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let fetch = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    state.cache.cache_notes(&column.id, &fetch.cacheable).await?;
    if backfill_cache_eligible(&resolved) {
        let entries = plan_boundary_initial(&fetch.source_outcomes);
        let _ = state.cache.replace_fetch_boundaries(&column.id, &entries).await;
    }
    open_streams_only(app, state, column, &resolved, host, token);
    Ok((fetch.notes, vec![]))
```

（元の `if resolved.kinds.len() == 1 && !resolved.use_cache { if let Some(oldest) = &fetch.raw_oldest_id { … set_fetch_boundary … } }` を上の `if backfill_cache_eligible(...)` ブロックに置き換える。）

- [ ] **Step 5: `fetch_backfill` を新APIへ**

doc comment と関数全体を次に置き換える:

```rust
/// 過去ページ（上スクロール）。`from cache` を含まないカラムは、要求範囲が全ソースの
/// backfill境界(`max(b_i)`)より新しければキャッシュのみで応答する(Issue #228 / #238)。
/// いずれかのソースの境界が未確定・範囲外・件数不足なら通常どおりAPIへ。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = backfill_cache_eligible(&resolved);
    let boundaries: std::collections::HashMap<u32, String> = if cache_eligible {
        state
            .cache
            .get_fetch_boundaries(&column.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    if cache_eligible {
        let effective = effective_boundary(&boundaries, resolved.kinds.len());
        let mut cached = match &effective {
            Some(e) if until_id.as_str() > e.as_str() => state
                .cache
                .load_cached_before(&column.id, &until_id, INITIAL_LIMIT)
                .await
                .unwrap_or_default(),
            _ => vec![],
        };
        // [E, until_id) の範囲外(=このセッションでは未検証)の行を除外する。
        // load_cached_before 自体は下限を持たないため、範囲内の件数が不足していても
        // セッションをまたいだ古いキャッシュ行で limit を満たしてしまう可能性がある。
        if let Some(e) = &effective {
            cached.retain(|n| n.id.as_str() >= e.as_str());
        }
        // ミュート/フィルタ設定はキャッシュ後に変更されうるため、都度再適用する。
        let ctx = state.eval_context();
        let mute = state.mute.lock().unwrap().clone();
        cached.retain(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(&state, &column.account_id, n)
                && !state.is_word_muted(&column.account_id, n)
        });
        if let Some(notes) = cache_backfill_page(effective.as_deref(), &until_id, cached, INITIAL_LIMIT) {
            state.cache_metrics.record_backfill(BackfillOutcome::Hit);
            return Ok(notes);
        }
        state.cache_metrics.record_backfill(if effective.is_none() {
            BackfillOutcome::FallbackBoundaryUnset
        } else {
            BackfillOutcome::FallbackOther
        });
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    state.cache.cache_notes(&column.id, &fetch.cacheable).await?;
    if cache_eligible {
        // ソースごとに、既存の境界と連続している場合のみ延長する(plan_boundary_extend)。
        // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
        // open_stream_and_fetch が改めて確定させる。
        let entries = plan_boundary_extend(&boundaries, &until_id, &fetch.source_outcomes);
        if !entries.is_empty() {
            let _ = state.cache.extend_fetch_boundaries(&column.id, &entries).await;
        }
    }
    Ok(fetch.notes)
}
```

- [ ] **Step 6: 暫定シムの削除とコメント更新**

`src-tauri/src/store/note_cache.rs` の `// ---- 暫定シム` から `extend_fetch_boundary` までの3メソッドを削除する。`grep -rn "get_fetch_boundary\b\|set_fetch_boundary\b\|extend_fetch_boundary\b" src-tauri/src` が定義もなく0件になること（`fetch_boundary` を含む別名 `get_fetch_boundaries` は除く）を確認する。

`src-tauri/src/state.rs` の `BackfillOutcome::FallbackBoundaryUnset` のコメント1行目を次に変える:

```rust
    /// cache_eligibleだがいずれかのソースのbackfill境界(get_fetch_boundaries)が未確定でAPIへ。
```

- [ ] **Step 7: テストを実行して通ることを確認**

Run: `cd src-tauri && cargo test 2>&1 | tail -30`
Expected: PASS（新テスト9件と既存の `cache_backfill_page_*` を含む。`generates_frontend_bindings` が通り、`git diff --stat frontend/src/bindings/tauri.gen.ts` に差分が無いこと）。

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: 警告なしでビルド成功（`raw_oldest_id` の残骸や未使用 import が無い）。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/store/note_cache.rs src-tauri/src/state.rs
git commit -m "feat: TQL複数ソースカラムのbackfillをキャッシュ優先にする (#238)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 全体検証と引き継ぎ

**Files:** なし（検証のみ。修正が出たら該当 Task のファイルを直して同じ件名規則でコミットする）。

- [ ] **Step 1: 全Rustテストと実DBテストの再実行**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: PASS。

Run: `cd src-tauri && cargo test --lib postgres_ -- --ignored 2>&1 | tail -10` と `cargo test --lib mysql_ -- --ignored --test-threads=2 2>&1 | tail -10`
Expected: PASS。終了後、`docker ps` で残ったコンテナをID指定で `docker rm -f <id>`。

- [ ] **Step 2: 既存の単一ソース挙動が変わっていないことの確認（コードレビュー観点）**

`git diff main -- src-tauri/src/commands/column.rs` を読み、単一ソース（N=1）で次が従来と同じ結果になることを確認する: 初回取得後の境界は「生レスポンス最古ID」、`until_id >= b` のときだけ延長、境界未確定なら延長しない、`use_cache` カラムは境界を読み書きしない。

- [ ] **Step 3: ユーザーへ引き継ぐ実機確認項目を報告する（自分では実行しない）**

実 Misskey アカウントが必要なので、次をユーザーの環境での確認項目として報告する:
1. TQL カラム `from home, local` を作成して数ページ上スクロールし、アプリを再起動して同じカラムを再度上スクロールする。Backstage のデバッグメトリクス（`get_debug_metrics` の `backfill_cache_hit` / `backfill_cache_fallback_boundary`）で `Hit` が増えること。
2. 既存の単一ソースカラム（旧 `column_fetch_boundary` を持つ既存 `cache.db`）が、更新後の初回起動で移行されキャッシュ優先が引き続き働くこと。`cache.db` は稼働中のインスタンスから `sqlite3 -readonly <db> ".backup '<copy>'"` で取ったコピーだけを調べ、実DBを直接触らない。
3. 疎なソース（例: 投稿の少ない `user:`）と密なソース（`home`）の組み合わせで、枯渇ソース（`oldest_fetched_id = ''`）が有効境界を塞がず、キャッシュ優先が効くこと。0件ページが枯渇を意味しない API を持つソース（`search:` など）を含むカラムで欠落が出ないこと（spec のエラーハンドリング節の残リスク）。

- [ ] **Step 4: 別Issue候補をユーザーに提案する（起票はユーザーの承認後）**

spec のスコープ外に書いた既存バグ（フィルタが強い複数ソースで、フロントの `until_id` が浅いソースの生最古IDより古くなり API 経路で読み飛ばす）を別Issueとして起票するか、ユーザーに確認する。承認が無い限り `gh issue create` は実行しない。

- [ ] **Step 5: PR について**

PR は「ユーザーが依頼したときだけ」作成する。作成する場合は `Closes #238` を本文に入れ、`.github/pull_request_template.md` の構造に手で合わせ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは通常のマージコミット（`gh pr merge --merge`）。push 後に CI を Monitor で待たない。
