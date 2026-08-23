# backfillキャッシュ優先化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `fetch_backfill`（上スクロールの過去ページ取得）で、単一ソースのカラムについて既にAPI取得済み・キャッシュ完全な範囲はローカルSQLiteキャッシュから返し、無駄なREST再問い合わせを減らす。

**Architecture:** カラムごとに「これより新しいノートはAPI取得済みで完全」という境界値(`oldest_fetched_id`)を`cache.db`の新テーブルに持つ。`fetch_backfill`は要求範囲がこの境界より新しければキャッシュのみで応答し、古い・未確定・件数不足ならAPIへフォールバックして境界を延長する。キャッシュ間引き(prune)がこの境界の正しさを壊さないよう、削除時に境界を追従させる。

**Tech Stack:** Rust / rusqlite（`src-tauri/src/store/note_cache.rs`, `src-tauri/src/store/db.rs`）、Tauri command層（`src-tauri/src/commands/column.rs`）。

## Global Constraints

- 対象は `resolve_sources` の結果が単一ソース(`resolved.kinds.len() == 1`)かつ `use_cache == false` のカラムのみ。TQL複数ソース・`from cache`併用カラムは対象外（今回は実装しない）。
- ID比較はMisskeyのソート可能なID文字列の辞書順比較(`&str`の`<`/`>`)を使う。既存コード(`n.id.as_str() <= newest_known_id`)と同じ流儀。
- 境界の読み書き失敗（SQLiteエラー）は`fetch_backfill`全体を失敗させない。キャッシュ優先を諦めてAPIフォールバックに倒す（`.ok()`で握りつぶす）。
- `fill_gap` / `gap_fill_on_reconnect` / `fillRemainingGap` は変更しない（このIssueのスコープ外）。
- 新規追加するコード（store層の純粋な決定ロジック含む）はTDDで進める。I/O(ネットワーク)を伴う`fetch_backfill`/`open_stream_and_fetch`自体の配線部分は、このコードベースの既存の慣習（`finalize_gap_fill`のように判定ロジックだけを純粋関数に切り出してテストし、配線部分はテストしない）に倣う。
- 参照spec: `docs/superpowers/specs/2026-08-23-cache-first-backfill-design.md`

---

### Task 1: 境界テーブルと基本CRUD (`get`/`set`/`extend_fetch_boundary`)

**Files:**
- Modify: `src-tauri/src/store/db.rs`（`CACHE_SCHEMA`に新テーブル追加）
- Modify: `src-tauri/src/store/note_cache.rs`（`NoteCacheStore`に3メソッド追加）
- Test: `src-tauri/src/store/note_cache.rs`（既存の`#[cfg(test)] mod tests`内）

**Interfaces:**
- Produces:
  - `NoteCacheStore::get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>>`
  - `NoteCacheStore::set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>`（無条件上書き）
  - `NoteCacheStore::extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>`（既存より古い方向へのみ更新、単調性を保証）

- [ ] **Step 1: `CACHE_SCHEMA`に新テーブルを追加**

`src-tauri/src/store/db.rs`の`CACHE_SCHEMA`定数、`column_note`テーブル定義の直後（`CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);`の次の行）に追加:

```sql

-- カラムごとの「これより新しいノートはAPI取得済みで完全」境界（Issue #228）
CREATE TABLE IF NOT EXISTS column_fetch_boundary (
    column_id         TEXT PRIMARY KEY,
    oldest_fetched_id TEXT NOT NULL
);
```

`CREATE TABLE IF NOT EXISTS`のため、既存DBでも次回起動時に自動で追加される（マイグレーション不要、`note_cache.rs`冒頭コメントの既存方針どおり）。

- [ ] **Step 2: 失敗するテストを書く**

`src-tauri/src/store/note_cache.rs`の`mod tests`内、`column_isolation_and_clear`テストの直後に追加:

```rust
#[test]
fn fetch_boundary_roundtrip() {
    let s = store();
    assert!(s.get_fetch_boundary("col1").unwrap().is_none());

    s.set_fetch_boundary("col1", "n100").unwrap();
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n100"));
}

#[test]
fn set_fetch_boundary_overwrites_unconditionally() {
    let s = store();
    s.set_fetch_boundary("col1", "n100").unwrap();
    s.set_fetch_boundary("col1", "n999"); // より新しい値でも無条件に上書き
    s.set_fetch_boundary("col1", "n999").unwrap();
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n999"));
}

#[test]
fn extend_fetch_boundary_only_moves_older() {
    let s = store();
    s.set_fetch_boundary("col1", "n500").unwrap();

    // より古い値(n300)への延長は反映される
    s.extend_fetch_boundary("col1", "n300").unwrap();
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));

    // より新しい値(n800)は無視される(単調性)
    s.extend_fetch_boundary("col1", "n800").unwrap();
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));
}

#[test]
fn extend_fetch_boundary_sets_when_absent() {
    let s = store();
    assert!(s.get_fetch_boundary("col1").unwrap().is_none());
    s.extend_fetch_boundary("col1", "n300").unwrap();
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));
}
```

- [ ] **Step 3: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::fetch_boundary_roundtrip`
Expected: FAIL（`get_fetch_boundary`が未定義でコンパイルエラー）

- [ ] **Step 4: 3メソッドを実装**

`src-tauri/src/store/note_cache.rs`の`impl NoteCacheStore`ブロック（`clear_column_notes`の直後）に追加:

```rust
    /// カラムの境界(oldest_fetched_id)を取得。未確定ならNone。
    pub fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let v: Option<String> = conn
            .query_row(
                "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = ?1",
                params![column_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    /// 境界を new_oldest_id で無条件に新規セット/上書きする(初回REST取得時に使う)。
    pub fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
            params![column_id, new_oldest_id],
        )?;
        Ok(())
    }

    /// 境界を new_oldest_id まで延長する(古い方向へのみ、単調性を保証)。
    /// 既存値の方が既に古ければ何もしない。
    pub fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET
                oldest_fetched_id = MIN(oldest_fetched_id, excluded.oldest_fetched_id)",
            params![column_id, new_oldest_id],
        )?;
        Ok(())
    }
```

- [ ] **Step 5: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::`
Expected: PASS（新規4テストを含め全て通る）

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/db.rs src-tauri/src/store/note_cache.rs
git commit -m "feat: カラム単位のbackfill境界(column_fetch_boundary)を追加"
```

---

### Task 2: `load_cached_before`（until_id指定でのカラムキャッシュ取得）

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`

**Interfaces:**
- Consumes: なし（Task 1と独立）
- Produces: `NoteCacheStore::load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>>`

- [ ] **Step 1: 失敗するテストを書く**

`load_cached`関連のテストの近く（`column_isolation_and_clear`の後、Task 1で追加したテストの後）に追加:

```rust
#[test]
fn load_cached_before_returns_notes_older_than_until_id_desc() {
    let s = store();
    s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();

    let got = s.load_cached_before("col1", "n3", 10).unwrap();
    assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n1"]);
}

#[test]
fn load_cached_before_respects_limit_and_column_scope() {
    let s = store();
    s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();
    s.cache_notes("col2", &[note("m1", 250)]).unwrap();

    let got = s.load_cached_before("col1", "n3", 1).unwrap();
    assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);

    // col2 のノートは混ざらない
    let got_all = s.load_cached_before("col1", "n3", 10).unwrap();
    assert!(got_all.iter().all(|n| n.id != "m1"));
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::load_cached_before_returns_notes_older_than_until_id_desc`
Expected: FAIL（`load_cached_before`未定義）

- [ ] **Step 3: 実装**

`load_cached`メソッドの直後に追加:

```rust
    /// カラムのキャッシュから until_id より古いノートを取得（新しい順、最大 limit 件）。
    /// backfill のキャッシュ優先パス用（load_cached の until_id 版）。
    pub fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1 AND cn.note_id < ?2
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![column_id, until_id, limit], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for payload in rows {
            out.extend(deserialize_note_or_warn(&payload?));
        }
        Ok(out)
    }
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/note_cache.rs
git commit -m "feat: カラムキャッシュのuntil_id指定取得(load_cached_before)を追加"
```

---

### Task 3: `clear_column_notes`で境界も削除

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`

**Interfaces:**
- Consumes: Task 1の`get_fetch_boundary`/`set_fetch_boundary`
- Produces: `clear_column_notes`の挙動変更（既存シグネチャは変えない）

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[test]
fn clear_column_notes_also_removes_boundary() {
    let s = store();
    s.cache_notes("col1", &[note("n1", 100)]).unwrap();
    s.set_fetch_boundary("col1", "n1").unwrap();

    s.clear_column_notes("col1").unwrap();

    assert!(s.get_fetch_boundary("col1").unwrap().is_none());
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::clear_column_notes_also_removes_boundary`
Expected: FAIL（境界が残ったままなので`is_none()`がfalse）

- [ ] **Step 3: `clear_column_notes`を修正**

```rust
    /// カラム所属レコードを消す（カラム削除時。note 本体は他カラムと共有しうるので残す）。
    pub fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_note WHERE column_id = ?1", params![column_id])?;
        conn.execute("DELETE FROM column_fetch_boundary WHERE column_id = ?1", params![column_id])?;
        Ok(())
    }
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/note_cache.rs
git commit -m "fix: clear_column_notesでbackfill境界も削除する"
```

---

### Task 4: `prune`（間引き）で境界を追従させる

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`（`delete_matching`関数）

**Interfaces:**
- Consumes: Task 1の`column_fetch_boundary`テーブル
- Produces: `delete_matching`の挙動変更（戻り値・シグネチャは変えない）。`prune`（`keep`超過・`max_age_days`・`max_size_mb`の3経路すべて）はこれを内部で呼ぶため自動的に対象になる。

- [ ] **Step 1: 失敗するテストを書く**

`prune_removes_oldest_beyond_keep_and_related_rows`テストの近くに追加:

```rust
#[test]
fn prune_raises_boundary_to_surviving_oldest_note_after_keep_exceeded() {
    let s = store();
    s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();
    s.set_fetch_boundary("col1", "n1").unwrap(); // n1まで(=全件)取得済みと主張

    let deleted = s.prune(2, 0, 0).unwrap(); // 最古のn1が削除される
    assert_eq!(deleted, 1);

    // n1が消えたので、生存最古のn2まで境界を引き上げる
    assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n2"));
}

#[test]
fn prune_clears_boundary_when_column_fully_pruned() {
    let s = store();
    let now = now_epoch();
    let one_day = 86_400;
    s.cache_notes("col1", &[note("old", now - 40 * one_day)]).unwrap();
    s.set_fetch_boundary("col1", "old").unwrap();

    let deleted = s.prune(0, 30, 0).unwrap();
    assert_eq!(deleted, 1);

    // カラムのキャッシュが全滅したので境界は未確定に戻る
    assert!(s.get_fetch_boundary("col1").unwrap().is_none());
}

#[test]
fn prune_leaves_unaffected_columns_boundary_untouched() {
    let s = store();
    s.cache_notes("colA", &[note("a1", 50)]).unwrap();
    s.cache_notes("colB", &[note("b1", 100), note("b2", 200), note("b3", 300)]).unwrap();
    s.set_fetch_boundary("colA", "a1").unwrap();
    s.set_fetch_boundary("colB", "b1").unwrap();

    let deleted = s.prune(3, 0, 0).unwrap(); // 4件中keep=3 → 全体最古のa1のみ削除
    assert_eq!(deleted, 1);

    assert!(s.get_fetch_boundary("colA").unwrap().is_none());
    assert_eq!(s.get_fetch_boundary("colB").unwrap().as_deref(), Some("b1")); // 変わらない
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::prune_raises_boundary_to_surviving_oldest_note_after_keep_exceeded`
Expected: FAIL（境界が`Some("n1")`のまま更新されない）

- [ ] **Step 3: `delete_matching`を修正**

```rust
/// `select_sql`（`SELECT id FROM note ...` 形式）にマッチするノートと、その関連テーブル
/// （note_reaction 等）・column_note を削除する（FK制約は張っていないため手動カスケード）。
/// 削除によって影響を受けたカラムの backfill 境界(column_fetch_boundary)も、生存している
/// 最古ノートIDまで引き上げる（全滅したカラムは境界ごと削除）。境界が「削除前の完全な範囲」
/// を主張したままだと、prune後にキャッシュに無いノートを「完全」と誤認して欠落表示になる
/// ため(Issue #228)。
/// 戻り値は削除したノート件数。
fn delete_matching(conn: &Connection, select_sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64> {
    conn.execute(&format!("CREATE TEMP TABLE prune_ids AS {select_sql}"), params)?;
    let deleted = conn.execute("DELETE FROM note WHERE id IN (SELECT id FROM prune_ids)", [])? as i64;

    // column_note をカスケード削除する前に、影響を受けるカラムを確定しておく
    let affected_columns: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT column_id FROM column_note WHERE note_id IN (SELECT id FROM prune_ids)",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE note_id IN (SELECT id FROM prune_ids)"),
            [],
        )?;
    }
    conn.execute("DROP TABLE prune_ids", [])?;

    for column_id in &affected_columns {
        let survivor: Option<String> = conn
            .query_row(
                "SELECT MIN(note_id) FROM column_note WHERE column_id = ?1",
                params![column_id],
                |r| r.get(0),
            )
            .optional()?;
        match survivor {
            Some(oldest) => {
                conn.execute(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = ?2
                     WHERE column_id = ?1 AND oldest_fetched_id < ?2",
                    params![column_id, oldest],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM column_fetch_boundary WHERE column_id = ?1",
                    params![column_id],
                )?;
            }
        }
    }
    Ok(deleted)
}
```

`OptionalExtension`は既にファイル冒頭で`use`済み（`get_note`等で使用）なのでそのまま使える。

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib store::note_cache::tests::`
Expected: PASS（新規3テストに加え、既存の`prune_*`テストも全て通る）

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/note_cache.rs
git commit -m "fix: pruneでbackfill境界を生存範囲まで引き上げる"
```

---

### Task 5: キャッシュ応答可否の純粋関数 `cache_backfill_page`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`

**Interfaces:**
- Consumes: `domain::Note`
- Produces: `fn cache_backfill_page(boundary: Option<&str>, until_id: &str, cached: Vec<Note>, limit: u32) -> Option<Vec<Note>>`
  （`Some(notes)` = キャッシュのみで応答してよい、`None` = APIフォールバックすべき）

- [ ] **Step 1: 失敗するテストを書く**

`finalize_gap_fill_dedupes_by_id`テストの直後（`mod tests`の末尾）に追加:

```rust
    #[test]
    fn cache_backfill_page_none_when_boundary_unknown() {
        let cached = vec![note("n1", 10); 0]; // 空でも境界未確定なら常にAPIへ
        let result = cache_backfill_page(None, "n999", cached, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_until_id_at_or_before_boundary() {
        let cached = vec![note("n1", 10)];
        // until_id が境界と同じ、または境界より古い場合は「未検証の領域」を含みうるのでAPIへ
        assert!(cache_backfill_page(Some("n500"), "n500", cached.clone(), 1).is_none());
        assert!(cache_backfill_page(Some("n500"), "n400", cached, 1).is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_cached_count_below_limit() {
        let cached = vec![note("n1", 10), note("n2", 20)];
        let result = cache_backfill_page(Some("n001"), "n999", cached, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_some_when_within_boundary_and_enough_notes() {
        let cached = vec![note("n2", 20), note("n1", 10)];
        let result = cache_backfill_page(Some("n001"), "n999", cached.clone(), 2);
        assert_eq!(
            result.unwrap().iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            cached.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib commands::column::tests::cache_backfill_page_none_when_boundary_unknown`
Expected: FAIL（`cache_backfill_page`未定義でコンパイルエラー）

- [ ] **Step 3: 実装**

`finalize_gap_fill`関数の直後に追加:

```rust
/// backfill 要求(until_id より古いページ)をキャッシュのみで賄えるか判定する純粋関数。
/// `boundary`(Some) は「これより新しいノートはAPI取得済みで完全」という境界。
/// `cached` は呼び出し元が事前に `load_cached_before` で取得した結果。
/// 境界が未確定、要求範囲が境界に届かない(未検証領域を含みうる)、
/// またはキャッシュ件数が limit に満たない場合は None(=APIへフォールバックすべき)を返す。
fn cache_backfill_page(
    boundary: Option<&str>,
    until_id: &str,
    cached: Vec<Note>,
    limit: u32,
) -> Option<Vec<Note>> {
    let boundary = boundary?;
    if until_id <= boundary {
        return None;
    }
    if cached.len() as u32 >= limit {
        Some(cached)
    } else {
        None
    }
}
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib commands::column::tests::`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/column.rs
git commit -m "feat: backfillのキャッシュ応答可否判定(cache_backfill_page)を追加"
```

---

### Task 6: `fetch_backfill`/`open_stream_and_fetch`への配線

**Files:**
- Modify: `src-tauri/src/commands/column.rs`

**Interfaces:**
- Consumes:
  - Task 1: `get_fetch_boundary`, `set_fetch_boundary`, `extend_fetch_boundary`
  - Task 2: `load_cached_before`
  - Task 5: `cache_backfill_page`
- Produces: `fetch_and_filter_multi`の戻り値型変更（`Vec<Note>` → `FilteredFetch`）。この型は本タスク内で完結し、他タスクからは参照されない。

このタスクはI/O(REST API呼び出し)を伴う配線のため、既存コードベースの慣習（`open_stream_and_fetch`等は単体テストを持たず、判定ロジックのみ純粋関数としてテスト済み — Task 5で対応済み）に倣い、新規の失敗するテストは書かない。実装後は既存の全テストスイートで回帰がないことを確認する。

- [ ] **Step 1: `fetch_and_filter_multi`の戻り値を`FilteredFetch`に変更**

`src-tauri/src/commands/column.rs`の`fetch_and_filter_multi`関数（およびその直前）を以下に置き換える:

```rust
/// `fetch_and_filter_multi` の戻り値。`raw_oldest_id` は単一ソース時のみ、
/// フィルタ適用前の生APIレスポンスの最古IDを持つ（backfill境界の更新に使う。
/// フィルタ後の最古IDだと、末尾がフィルタで弾かれた場合に「実際にはもっと深く
/// APIを見ている」事実を取り逃すため）。複数ソース時はNone(境界追跡の対象外)。
struct FilteredFetch {
    notes: Vec<Note>,
    raw_oldest_id: Option<String>,
}

/// 解決済みソース群から REST 初期/過去ページを取得し、id重複除去+created_at降順マージの上、
/// フィルタ/ミュートを適用する。`cache` ソースが含まれる場合はローカルSQLite検索も合成する。
/// 個別ソースの取得失敗は他ソースの結果を活かすため無視する（TQL§複数ソースは OR 合成のため）。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    let mut all: Vec<Note> = Vec::new();

    if !resolved.kinds.is_empty() {
        let client = state.client_for(account_id)?;
        for k in &resolved.kinds {
            if let Some((endpoint, body)) = k.rest_request(INITIAL_LIMIT, until_id) {
                if let Ok(raw) = fetch_notes(&client, endpoint, &body).await {
                    all.extend(raw);
                }
            }
        }
    }

    // 単一ソース時のみ、フィルタ適用前の生レスポンスの最古IDを控える(backfill境界用)。
    let raw_oldest_id = if resolved.kinds.len() == 1 {
        all.iter()
            .min_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)))
            .map(|n| n.id.clone())
    } else {
        None
    };

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
        if let Ok(cached) = state.cache.search_cache(&where_sql, until_id, INITIAL_LIMIT) {
            all.extend(cached);
        }
    }

    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut filtered: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
        })
        .collect();

    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート + limit へ切り詰め
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(INITIAL_LIMIT as usize);
    Ok(FilteredFetch { notes: filtered, raw_oldest_id })
}
```

- [ ] **Step 2: `fetch_backfill`を修正**

既存の`fetch_backfill`関数全体を以下に置き換える:

```rust
/// 過去ページ（上スクロール）。単一ソースのカラムは、要求範囲がbackfill境界より新しければ
/// キャッシュのみで応答する(Issue #228)。境界未確定・範囲外・件数不足なら通常どおりAPIへ。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = resolved.kinds.len() == 1 && !resolved.use_cache;
    if cache_eligible {
        let boundary = state.cache.get_fetch_boundary(&column.id).ok().flatten();
        let cached = match &boundary {
            Some(b) if until_id.as_str() > b.as_str() => state
                .cache
                .load_cached_before(&column.id, &until_id, INITIAL_LIMIT)
                .unwrap_or_default(),
            _ => vec![],
        };
        if let Some(notes) = cache_backfill_page(boundary.as_deref(), &until_id, cached, INITIAL_LIMIT) {
            return Ok(notes);
        }
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    state.cache.cache_notes(&column.id, &fetch.notes)?;
    if cache_eligible {
        if let Some(oldest) = &fetch.raw_oldest_id {
            let _ = state.cache.extend_fetch_boundary(&column.id, oldest);
        }
    }
    Ok(fetch.notes)
}
```

- [ ] **Step 3: `open_stream_and_fetch`を修正**

既存の`open_stream_and_fetch`関数内、`fetch_and_filter_multi`を呼んでいる箇所を以下に置き換える:

```rust
    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let fetch = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    state.cache.cache_notes(&column.id, &fetch.notes)?;
    if resolved.kinds.len() == 1 && !resolved.use_cache {
        if let Some(oldest) = &fetch.raw_oldest_id {
            let _ = state.cache.set_fetch_boundary(&column.id, oldest);
        }
    }
    open_streams_only(app, state, column, &resolved, host, token);
    Ok((fetch.notes, vec![]))
```

- [ ] **Step 4: ビルドと既存テストで回帰がないことを確認**

Run: `cd src-tauri && cargo build && cargo test --lib`
Expected: ビルド成功、既存テスト（`store::note_cache::tests::*`, `commands::column::tests::*`含む）が全てPASS

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/column.rs
git commit -m "feat: fetch_backfillでbackfill境界内はキャッシュから応答する"
```

---

### Task 7: 全体テストとフロントエンドバインディングの確認

**Files:** なし（検証のみ）

**Interfaces:** なし

- [ ] **Step 1: Rust側フルテスト**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]`が付いた実Misskey接続テストを除く全テストが通る）

- [ ] **Step 2: TSバインディングが変わっていないことを確認**

今回のタスクでは新規`#[tauri::command]`や`specta::specta`公開型を追加していない（`FilteredFetch`・`cache_backfill_page`はどちらもprivateなRust内部実装）ため、`frontend/src/bindings/tauri.gen.ts`は変化しないはず。

Run: `git status frontend/src/bindings/tauri.gen.ts`
Expected: 変更なし（`cargo test`実行時に`generates_frontend_bindings`テストが再生成するため、差分が出ないことを確認する）

- [ ] **Step 3: フロントエンドの型チェック**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 4: 実装計画の完了をユーザーに報告**

このタスクはコミットを含まない（検証のみのため、変更が無ければ何もコミットしない）。全ステップがPASSしたら、実装完了をユーザーに報告し、PR作成（`commit-commands:commit-push-pr`）に進む。
