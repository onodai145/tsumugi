# カラムのキャッシュ書き込みの世代フェンス Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `update_column` / `close_column` が実行中の REST 取得と競合しても、旧定義の結果や古い境界がカラムのキャッシュに書かれず、旧フィルタの結果が画面に混ざらないようにする(Issue #446)。

**Architecture:** カラムごとの世代(epoch)とロックを持つ `ColumnFence` を `AppState` に置く。書き込み側は、カラム定義を読む前に `begin` で世代を控え、DB への書き込みの直前だけ `write_if_current` でロックを取って世代を確認し、古ければ何も書かない。`update_column` / `close_column` は `invalidate` の中で保存と clear を行い、その後で世代を進める。フロントエンドは、タブに `epoch` を持たせ、`updateColumn` をまたいで返った backfill の結果を捨てる。

**Tech Stack:** Rust(tokio、tauri、rusqlite)、Svelte 5 runes + TypeScript、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-03-column-write-fence-design.md`(承認済み)。実装者は計画と spec の両方を読むこと。

## Global Constraints

- DB スキーマ、Tauri コマンドの署名、`Error` の variant、TS バインディング(`frontend/src/bindings/tauri.gen.ts`)は変えない。`cargo test` 後に `git diff -- frontend/src/bindings/tauri.gen.ts` が空であること。
- 世代は `ColumnFence` のプロセス全体の単調増加カウンタから払い出す(`remove` で作り直したエントリで再利用されない)。
- ロックを持つのは DB 書き込み(`f`)の間だけ。`fetch_and_filter_multi` や `fill_gap` などのネットワーク取得は、ロックの外で行う。`f` の中から別の `write_if_current` / `invalidate` を呼ばない(入れ子にしない)。
- 書き込み側は、カラム定義(`load_column` / `resolve_sources`)を読む**前に** `begin` する(`add_column` は新規カラムなので、定義を組み立てた直後)。
- 世代が古い `fetch_backfill` は `Ok(vec![])` を返す。古い `open_stream_and_fetch` は、書き込まず、ストリームも開かず、`Ok((vec![], vec![]))` を返す。古いギャップ埋めは、書き込まず、`ColumnGapFill` イベントも出さない。
- コミットメッセージは件名のみ・本文なし(末尾の `Co-Authored-By` トレーラーは別)。`--no-verify` は使わない。コミットが失敗したら止まって報告する。
- `main` には直接コミットしない。作業ブランチは `fix/column-write-fence-446`(作成済み)。push と PR 作成は、ユーザーの指示を受けてから行う。
- UI 文言・ドキュメントは日本語。

## Review Focus

- **同じカラムへ `update_column` が連続した場合**: 先に終えた呼び出しが得た世代は古くなり、後の呼び出しだけが書ける(Task 1 のテスト `second_invalidate_stales_the_first_returned_epoch`)。
- **`close_column` の後に、実行中の backfill が書く場合**: 削除済みカラムの `column_note` や境界の孤児を作らない(Task 2 のテスト `commit_backfill_writes_leaves_no_orphans_after_close`)。
- **名前だけの変更(`updateColumn` の早期 return)**: `epoch` が進まず、進行中の backfill の結果を捨てない(Task 4 のテスト)。
- **ロックをネットワーク取得の間に持たない**: 取得が遅くても `update_column` が待たされない(Task 3 の静的確認: `commit_*` ヘルパーの中にネットワーク呼び出しが無いこと)。
- **古い結果を空で返しても、ギャップマーカーが消えない**: `fillRemainingGap` が「ギャップが埋まった」と誤認しない(Task 4 のテストで、`gapMarker` が残ることを確認する)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/fence.rs`(新規) | `ColumnFence` と `Epoch`。世代の払い出し、ロック、`begin` / `write_if_current` / `invalidate` / `remove`。他のモジュールに依存しない |
| `src-tauri/src/lib.rs` | `mod fence;` を追加 |
| `src-tauri/src/state.rs` | `AppState` に `column_fence: ColumnFence` を追加 |
| `src-tauri/src/commands/column.rs` | 書き込み3種(`commit_backfill_writes`、`commit_initial_writes`、`commit_gap_fill_writes`)の追加と、各経路への配線 |
| `frontend/src/lib/store.svelte.ts` | `TabView.epoch`、`updateColumn` での増分、`loadMore` / `fillRemainingGap` / `fillGapBelow` のガード |
| `frontend/src/lib/store.svelte.test.ts`、`frontend/src/ui/AddColumnModal.test.ts` | `TabView` リテラルへの `epoch: 0` の追加、新規テスト |

## Task 1: `ColumnFence`

**Files:**
- Create: `src-tauri/src/fence.rs`
- Modify: `src-tauri/src/lib.rs`(`mod` 宣言)、`src-tauri/src/state.rs`(`AppState` のフィールドと初期化)

**Interfaces:**
- Consumes: なし。
- Produces(以降のタスクが使う):
  - `pub struct Epoch(u64)` — `#[derive(Debug, Clone, PartialEq, Eq)]`
  - `pub struct ColumnFence` — `#[derive(Default)]`
  - `pub fn begin(&self, column_id: &str) -> Epoch`
  - `pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T> where F: FnOnce() -> Fut, Fut: Future<Output = T>`
  - `pub async fn invalidate<F, Fut, T>(&self, column_id: &str, f: F) -> (Epoch, T) where F: FnOnce() -> Fut, Fut: Future<Output = T>`
  - `pub fn remove(&self, column_id: &str)`
  - `AppState.column_fence: ColumnFence`

このタスクの時点では `ColumnFence` のメソッドを本体から呼ぶ箇所が無いので、`cargo build` で `dead_code` の警告が出てよい(Task 3 で解消する)。`#[allow(dead_code)]` は付けない。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/fence.rs` を、テストだけで作る(実装はまだ書かない)。

```rust
//! カラムごとの世代(epoch)とロック(Issue #446)。

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn write_runs_when_epoch_is_current() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        let out = fence.write_if_current("c1", &epoch, || async { 7 }).await;

        assert_eq!(out, Some(7));
    }

    #[tokio::test]
    async fn begin_is_stable_per_column_and_distinct_across_columns() {
        let fence = ColumnFence::default();

        assert_eq!(fence.begin("c1"), fence.begin("c1"));
        assert_ne!(fence.begin("c1"), fence.begin("c2"));
    }

    #[tokio::test]
    async fn write_is_stale_after_invalidate_and_returned_epoch_is_current() {
        let fence = ColumnFence::default();
        let before = fence.begin("c1");

        let (after, ran) = fence.invalidate("c1", || async { "cleared" }).await;

        assert_eq!(ran, "cleared");
        assert_eq!(fence.write_if_current("c1", &before, || async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &after, || async { 2 }).await, Some(2));
    }

    #[tokio::test]
    async fn second_invalidate_stales_the_first_returned_epoch() {
        let fence = ColumnFence::default();

        let (first, _) = fence.invalidate("c1", || async {}).await;
        let (second, _) = fence.invalidate("c1", || async {}).await;

        assert_eq!(fence.write_if_current("c1", &first, || async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &second, || async { 2 }).await, Some(2));
    }

    #[tokio::test]
    async fn write_is_stale_after_remove() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        fence.remove("c1");

        assert_eq!(fence.write_if_current("c1", &epoch, || async { 1 }).await, None);
    }

    #[tokio::test]
    async fn recreated_entry_never_reuses_an_old_epoch() {
        let fence = ColumnFence::default();
        let old = fence.begin("c1");
        fence.remove("c1");

        let new = fence.begin("c1");

        assert_ne!(old, new);
        assert_eq!(fence.write_if_current("c1", &old, || async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &new, || async { 2 }).await, Some(2));
    }

    #[tokio::test]
    async fn invalidate_of_one_column_does_not_stale_another() {
        let fence = ColumnFence::default();
        let other = fence.begin("c2");

        fence.invalidate("c1", || async {}).await;

        assert_eq!(fence.write_if_current("c2", &other, || async { 1 }).await, Some(1));
    }

    #[tokio::test]
    async fn write_waits_for_invalidate_in_progress_then_is_stale() {
        let fence = Arc::new(ColumnFence::default());
        let epoch = fence.begin("c1");
        let entered = Arc::new(tokio::sync::Notify::new());
        let gate = Arc::new(tokio::sync::Notify::new());

        let invalidating = tokio::spawn({
            let (fence, entered, gate) = (fence.clone(), entered.clone(), gate.clone());
            async move {
                fence
                    .invalidate("c1", move || async move {
                        entered.notify_one();
                        gate.notified().await;
                    })
                    .await
            }
        });
        entered.notified().await; // invalidate が f の実行中(=ロックを持っている)

        let writing = tokio::spawn({
            let fence = fence.clone();
            async move { fence.write_if_current("c1", &epoch, || async { 42 }).await }
        });
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert!(!writing.is_finished(), "invalidate の実行中は書き込みが待たされる");

        gate.notify_one();
        invalidating.await.unwrap();

        assert_eq!(writing.await.unwrap(), None);
    }
}
```

`lib.rs` の `mod error;` の次の行に `mod fence;` を足す(`mod events;` と `mod filter;` の間の、アルファベット順の位置)。

```rust
mod events;
mod fence;
mod filter;
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib fence:: 2>&1 | grep -E "^error" | head`
Expected: `cannot find type ColumnFence in this scope` などのコンパイルエラー(実装が無いため)。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/fence.rs` の先頭(`#[cfg(test)]` の前)に追加する。

```rust
//! カラムごとの世代(epoch)とロック(Issue #446)。
//!
//! `update_column` / `close_column` が実行中の REST 取得と競合して、旧定義の結果や古い境界を
//! カラムのキャッシュへ書かせないための仕組み。書き込み側は処理の開始時に `begin` で世代を控え、
//! 書き込みの直前だけ `write_if_current` でロックを取って世代を確認する。ロックは DB 書き込みの
//! 間だけ持ち、ネットワーク取得の間は持たない。設計は
//! docs/superpowers/specs/2026-10-03-column-write-fence-design.md。

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// `begin` / `invalidate` が返す、控えた時点のカラムの世代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epoch(u64);

struct Entry {
    epoch: AtomicU64,
    lock: tokio::sync::Mutex<()>,
}

#[derive(Default)]
pub struct ColumnFence {
    /// 世代の払い出し。プロセス全体で単調増加するので、`remove` で消して作り直したエントリの
    /// 世代が、以前のものと一致することはない。
    next: AtomicU64,
    columns: Mutex<HashMap<String, Arc<Entry>>>,
}

impl ColumnFence {
    fn issue(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn entry_or_create(&self, column_id: &str) -> Arc<Entry> {
        let mut columns = self.columns.lock().unwrap();
        Arc::clone(columns.entry(column_id.to_string()).or_insert_with(|| {
            Arc::new(Entry { epoch: AtomicU64::new(self.issue()), lock: tokio::sync::Mutex::new(()) })
        }))
    }

    /// 現在の世代を控える。エントリが無ければ作る。ロックは取らない。
    pub fn begin(&self, column_id: &str) -> Epoch {
        Epoch(self.entry_or_create(column_id).epoch.load(Ordering::SeqCst))
    }

    /// ロックを取り、世代が `epoch` と一致する時だけ `f` を実行する。
    /// 不一致、またはエントリが無い(`remove` 済み)なら `f` を実行せず `None`(=古い)を返す。
    pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let entry = self.columns.lock().unwrap().get(column_id).cloned()?;
        let _held = entry.lock.lock().await;
        if entry.epoch.load(Ordering::SeqCst) != epoch.0 {
            return None;
        }
        Some(f().await)
    }

    /// ロックを取り、`f`(新しい定義の保存と clear)を実行し、**その後で**世代を進める。
    /// 進めた後の世代を返す(`f` が失敗しても進める。定義が中途半端に更新されうるため)。
    /// `f` の実行中に `begin` した処理は旧い世代を控えるので、`f` が終わった後の書き込みで捨てられる。
    pub async fn invalidate<F, Fut, T>(&self, column_id: &str, f: F) -> (Epoch, T)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let entry = self.entry_or_create(column_id);
        let _held = entry.lock.lock().await;
        let result = f().await;
        let next = self.issue();
        entry.epoch.store(next, Ordering::SeqCst);
        (Epoch(next), result)
    }

    /// エントリを消す(`close_column` が `invalidate` の後に呼ぶ)。以降、そのカラムの
    /// 旧い世代による `write_if_current` は `None` になる。
    pub fn remove(&self, column_id: &str) {
        self.columns.lock().unwrap().remove(column_id);
    }
}
```

`src-tauri/src/state.rs` に、`use crate::fence::ColumnFence;` を既存の `use crate::` 行の並びへ足し、`AppState` の `cache_metrics` の次に追加する。

```rust
    /// キャッシュhit/fallback回数の集計(Issue #241)。Backstageの「メトリクス」タブ用。
    pub cache_metrics: CacheMetrics,
    /// カラムごとの世代とロック。`update_column` / `close_column` と、実行中の REST 取得の
    /// キャッシュ書き込みの競合を防ぐ(Issue #446)。
    pub column_fence: ColumnFence,
```

`new_with_sound` の `Self { ... }` の末尾(`cache_metrics: CacheMetrics::default(),` の次の行)に追加する。

```rust
            cache_metrics: CacheMetrics::default(),
            column_fence: ColumnFence::default(),
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib fence::`
Expected: `test result: ok. 8 passed`

- [ ] **Step 5: 全体が壊れていないことを確認してコミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^test result|FAILED|error"`
Expected: `test result: ok`(失敗 0。`dead_code` の警告は許容)

```bash
git add src-tauri/src/fence.rs src-tauri/src/lib.rs src-tauri/src/state.rs
git commit -m "feat: カラムごとの世代とロックを持つColumnFenceを追加する"
```

## Task 2: 書き込み3種のヘルパー

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(import 追加、`cache_fetched` の近くにヘルパー3つ、`mod tests` にテスト)

**Interfaces:**
- Consumes: Task 1 の `ColumnFence` / `Epoch`。既存の `cache_fetched(cache: &NoteCacheStore, column_id: &str, fetch: &FilteredFetch) -> Result<()>`、`apply_gap_fill_boundaries(cache: &NoteCacheStore, column_id: &str, gap: &GapFillResult)`、`FilteredFetch { notes, cacheable, source_outcomes }`、`GapFillResult { notes, truncated, boundary_id, sources, all_reached, dropped_floor }`、`GapSourceState { oldest_fetched, reached_target }`。
- Produces(Task 3 が配線する):
  - `async fn commit_backfill_writes(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str, epoch: &Epoch, fetch: &FilteredFetch, extend: &[(u32, String)]) -> Option<Result<()>>`
  - `async fn commit_initial_writes(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str, epoch: &Epoch, fetch: &FilteredFetch, boundaries: Option<&[(u32, String)]>) -> Option<Result<()>>`
  - `async fn commit_gap_fill_writes(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str, epoch: &Epoch, gap: &GapFillResult) -> bool`(`true` = 世代が現在で書いた)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `mod tests` の末尾(最後の `}` の前)に追加する。`note(id, created_at)` は既存のテスト用ヘルパー。

```rust
    fn mem_cache() -> NoteCacheStore {
        NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ))
    }

    fn fetch_of(ids: &[&str]) -> FilteredFetch {
        FilteredFetch {
            notes: vec![],
            cacheable: ids.iter().enumerate().map(|(i, id)| note(id, i as i64 + 1)).collect(),
            source_outcomes: vec![],
        }
    }

    fn pair(idx: u32, id: &str) -> (u32, String) {
        (idx, id.to_string())
    }

    /// `update_column` が行うこと(新しい定義の保存の代わりに新フィルタ側の境界を置く + clear)を模す。
    async fn simulate_update_column(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str) -> Epoch {
        fence
            .invalidate(column_id, || async {
                cache.clear_column_notes(column_id).await.unwrap();
                cache.replace_fetch_boundaries(column_id, &[pair(0, "n900")]).await.unwrap();
            })
            .await
            .0
    }

    #[tokio::test]
    async fn commit_backfill_writes_caches_notes_and_extends_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1");

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")]).await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n300")]);
    }

    #[tokio::test]
    async fn commit_backfill_writes_writes_nothing_after_update_column() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 旧定義で取得を始めた
        simulate_update_column(&fence, &cache, "c1").await;

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")]).await;

        assert!(written.is_none());
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        // 旧フィルタの延長(n300)で、新フィルタの境界(n900)が古い方へ動かない
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }

    #[tokio::test]
    async fn commit_backfill_writes_leaves_no_orphans_after_close() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        fence.invalidate("c1", || async { cache.clear_column_notes("c1").await.unwrap() }).await;
        fence.remove("c1"); // close_column

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")]).await;

        assert!(written.is_none());
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_initial_writes_replaces_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");

        let written =
            commit_initial_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), Some(&[pair(0, "n100")])).await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n100")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_keeps_boundaries_untouched_when_none_given() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n700")]).await.unwrap();
        let epoch = fence.begin("c1");

        commit_initial_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), None).await;

        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n700")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_does_not_overwrite_a_newer_update_column() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let slow = fence.begin("c1"); // 1回目の update_column の取得(遅い)
        simulate_update_column(&fence, &cache, "c1").await; // 2回目が先に終わった

        let written =
            commit_initial_writes(&fence, &cache, "c1", &slow, &fetch_of(&["n400"]), Some(&[pair(0, "n100")])).await;

        assert!(written.is_none());
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }

    fn gap_with(notes: Vec<Note>) -> GapFillResult {
        GapFillResult {
            notes,
            truncated: true,
            boundary_id: None,
            // 全ソースが追いついたが、limit で n450 以下を切り捨てた = 境界が n450 へ引き上がる
            sources: vec![GapSourceState { oldest_fetched: None, reached_target: true }],
            all_reached: true,
            dropped_floor: Some("n450".to_string()),
        }
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_caches_notes_and_raises_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n400")]).await.unwrap();
        let epoch = fence.begin("c1");

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n450")]);
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_writes_nothing_when_stale() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_update_column(&fence, &cache, "c1").await;

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(!written);
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib commit_ 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: `cannot find function commit_backfill_writes` / `commit_initial_writes` / `commit_gap_fill_writes` と、`ColumnFence` / `Epoch` が見つからないエラー(まだ import していないため)。

- [ ] **Step 3: 最小の実装を書く**

`column.rs` の先頭の import に追加する(`use crate::error::{Error, Result};` の次の行)。

```rust
use crate::fence::{ColumnFence, Epoch};
```

`cache_fetched`(`async fn cache_fetched(...)`)の直後に、ヘルパー3つを追加する。

```rust
/// `fetch_backfill` の書き込み: 取得ノートのキャッシュと、ソースごとの境界の延長(`extend`)。
/// `epoch` が古ければ(取得中に `update_column` / `close_column` が走った)何も書かず `None` を返す
/// (Issue #446)。境界の延長の失敗は握りつぶす(更新できなくても従来の挙動に戻るだけ)。
async fn commit_backfill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    fetch: &FilteredFetch,
    extend: &[(u32, String)],
) -> Option<Result<()>> {
    fence
        .write_if_current(column_id, epoch, || async {
            cache_fetched(cache, column_id, fetch).await?;
            if !extend.is_empty() {
                let _ = cache.extend_fetch_boundaries(column_id, extend).await;
            }
            Ok::<(), Error>(())
        })
        .await
}

/// `open_stream_and_fetch` の書き込み: 初回取得ノートのキャッシュと、`boundaries`(Some の時)での
/// 境界の置き換え。`epoch` が古ければ何も書かず `None` を返す(Issue #446)。
async fn commit_initial_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    fetch: &FilteredFetch,
    boundaries: Option<&[(u32, String)]>,
) -> Option<Result<()>> {
    fence
        .write_if_current(column_id, epoch, || async {
            cache_fetched(cache, column_id, fetch).await?;
            if let Some(entries) = boundaries {
                let _ = cache.replace_fetch_boundaries(column_id, entries).await;
            }
            Ok::<(), Error>(())
        })
        .await
}

/// ギャップ埋めの書き込み: 境界の引き上げと、収集したノートのキャッシュ。`epoch` が古ければ
/// 何も書かず `false` を返す。呼び出し元は `false` なら `ColumnGapFill` イベントも出さない(Issue #446)。
async fn commit_gap_fill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    gap: &GapFillResult,
) -> bool {
    fence
        .write_if_current(column_id, epoch, || async {
            apply_gap_fill_boundaries(cache, column_id, gap).await;
            if !gap.notes.is_empty() {
                let _ = cache.cache_notes(column_id, &gap.notes).await;
            }
        })
        .await
        .is_some()
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib commit_`
Expected: `test result: ok. 9 passed`

- [ ] **Step 5: コミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^test result|FAILED"`
Expected: `ok`、失敗 0。

```bash
git add src-tauri/src/commands/column.rs
git commit -m "feat: 世代が現在の時だけキャッシュへ書く3種のヘルパーを追加する"
```

## Task 3: 各経路への配線

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`fetch_backfill`、`open_stream_and_fetch` とその3つの呼び出し元、`update_column`、`close_column`、`resume_column` の裏処理、`gap_fill_on_reconnect`)

**Interfaces:**
- Consumes: Task 1・2 の `ColumnFence`、`Epoch`、`commit_backfill_writes`、`commit_initial_writes`、`commit_gap_fill_writes`、`AppState.column_fence`。
- Produces: `async fn open_stream_and_fetch(app, state, column, resolved, host, token, epoch: &Epoch)`(private。引数を末尾に1つ追加)。

配線は `AppHandle` を要するコマンドの中なので、単体テストは書けない。代わりに、既存テストが通ることと、最後の静的確認(Step 9)で、書き込みがすべてヘルパー経由になっていることを確認する。

- [ ] **Step 1: `fetch_backfill` を配線する**

関数の先頭(`let column = load_column(...)` の前)に、世代を控える行を足す。

```rust
) -> Result<Vec<Note>> {
    // カラム定義を読む前に世代を控える。取得中に update_column / close_column が走ったら、
    // 下の書き込みは捨てられる(Issue #446)。
    let epoch = state.column_fence.begin(&column_id);
    let column = load_column(&state, &column_id)?;
```

関数の末尾(`let fetch = fetch_and_filter_multi(...)` 以降)を置き換える。

```rust
    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    // ソースごとに、既存の境界と連続している場合のみ延長する(plan_boundary_extend)。
    // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
    // open_stream_and_fetch が改めて確定させる。
    let extend = if cache_eligible {
        plan_boundary_extend(&boundaries, &until_id, &fetch.source_outcomes)
    } else {
        vec![]
    };
    // 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず空で返す(Issue #446)。
    match commit_backfill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &fetch, &extend).await {
        None => return Ok(vec![]),
        Some(written) => written?,
    }
    Ok(fetch.notes)
}
```

- [ ] **Step 2: `open_stream_and_fetch` を配線する**

シグネチャの末尾に `epoch: &Epoch` を足す。

```rust
async fn open_stream_and_fetch(
    app: &AppHandle,
    state: &AppState,
    column: &Column,
    resolved: Option<ResolvedSources>,
    host: String,
    token: String,
    epoch: &Epoch,
) -> Result<(Vec<Note>, Vec<Notification>)> {
```

通知カラムの分岐(`if matches!(column.kind, ColumnKind::Notifications) { ... return ...; }`)は変えない。その後の、ノートカラム用の部分を置き換える。

```rust
    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let fetch = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    let boundaries = backfill_cache_eligible(&resolved).then(|| plan_boundary_initial(&fetch.source_outcomes));
    // 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず、ストリームも
    // 開かない。後続の update_column が自分の定義で開き直す(Issue #446)。
    match commit_initial_writes(&state.column_fence, &state.cache, &column.id, epoch, &fetch, boundaries.as_deref())
        .await
    {
        None => return Ok((vec![], vec![])),
        Some(written) => written?,
    }
    open_streams_only(app, state, column, &resolved, host, token);
    Ok((fetch.notes, vec![]))
}
```

- [ ] **Step 3: `add_column` を配線する**

`let column = Column { ... };` の直後、`state.settings.upsert_column(&column)?;` の直前に世代を控え、呼び出しに渡す。

```rust
    let epoch = state.column_fence.begin(&column.id);
    state.settings.upsert_column(&column)?;

    let (notes, notifications) =
        open_stream_and_fetch(&app, &state, &column, resolved, host, token, &epoch).await?;
```

- [ ] **Step 4: `update_column` を配線する**

保存・ストリームを閉じる・clear を、`invalidate` の中に入れ、返った世代を `open_stream_and_fetch` へ渡す。

置き換える範囲は、`column.title = ...;` の次の行にある `state.settings.upsert_column(&column)?;` から、`state.cache.clear_column_notes(&column_id).await?;` までと、後続の `open_stream_and_fetch` の呼び出し。

```rust
    // 新しい定義の保存・既存ストリームのクローズ・旧フィルタで貯めたキャッシュの破棄を、世代を進める
    // ロックの中で行う。実行中の取得は、世代が古くなって書き込みを捨てる(Issue #446)。
    let (epoch, cleared) = state
        .column_fence
        .invalidate(&column_id, || async {
            state.settings.upsert_column(&column)?;
            state.connections.close(&column_id);
            state.cache.clear_column_notes(&column_id).await
        })
        .await;
    cleared?;
```

```rust
    let (notes, notifications) =
        open_stream_and_fetch(&app, &state, &column, resolved, host, token, &epoch).await?;
```

- [ ] **Step 5: `close_column` を配線する**

`state.cache.clear_column_notes(&column_id).await?;` の行を置き換える。

```rust
    state.connections.close(&column_id);
    state.settings.delete_column(&column_id)?;
    // 世代を進めてから消し、以降の実行中の取得は書き込みを捨てる(孤児データを作らない, Issue #446)。
    let (_, cleared) = state
        .column_fence
        .invalidate(&column_id, || async { state.cache.clear_column_notes(&column_id).await })
        .await;
    state.column_fence.remove(&column_id);
    cleared?;
    state.settings.delete_empty_groups()?;
    Ok(())
```

- [ ] **Step 6: `resume_column` を配線する**

関数の先頭(`let column = load_column(...)` の前)に世代を控える。

```rust
) -> Result<OpenedColumn> {
    // カラム定義を読む前に世代を控える(Issue #446)。
    let epoch = state.column_fence.begin(&column_id);
    let column = load_column(&state, &column_id)?;
```

キャッシュが空の場合の呼び出しに `&epoch` を足す。

```rust
        open_stream_and_fetch(&app, &state, &column, resolved, host, token, &epoch).await?
```

裏の `spawn` の中で、`apply_gap_fill_boundaries(...)` から `cache_notes(...)` までを置き換える。

```rust
                // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
                // 残るので、空判定の前に行う(Issue #432)。取得中に update_column / close_column が
                // 走っていた(世代が古い)なら、何も書かず、イベントも出さない(Issue #446)。
                if !commit_gap_fill_writes(&state.column_fence, &state.cache, &column_id, &epoch, &gap_result).await {
                    return;
                }
                if gap_result.notes.is_empty() {
                    return;
                }
```

置き換えるのは、元の次の部分(コメントは上の新しいコメントに統合する)。

```rust
                apply_gap_fill_boundaries(&state.cache, &column_id, &gap_result).await;
                if gap_result.notes.is_empty() {
                    return;
                }
                let _ = state.cache.cache_notes(&column_id, &gap_result.notes).await;
```

`epoch` は `if notes.is_empty() { ... &epoch ... } else { ... spawn(async move { ... &epoch ... }) }` の両方の枝で使われる。枝は排他なので、`async move` で `epoch` を move してもコンパイルが通る。

- [ ] **Step 7: `gap_fill_on_reconnect` を配線する**

`GapFillGuard::try_acquire` の成功後、`load_column` の前に世代を控える。

```rust
    let Some(_guard) = GapFillGuard::try_acquire(app, &state, column_id) else {
        // 同一カラムの前回ギャップ埋めが実行中(フラッピング再接続対策)。
        return;
    };
    // カラム定義を読む前に世代を控える(Issue #446)。
    let epoch = state.column_fence.begin(column_id);
    let Ok(column) = load_column(&state, column_id) else {
```

関数末尾の、`apply_gap_fill_boundaries(...)` から `cache_notes(...)` までを置き換える。

```rust
    // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
    // 残るので、空判定の前に行う(Issue #432)。取得中に update_column / close_column が
    // 走っていた(世代が古い)なら、何も書かず、イベントも出さない(Issue #446)。
    if !commit_gap_fill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &gap_result).await {
        return;
    }
    if gap_result.notes.is_empty() {
        return;
    }
```

- [ ] **Step 8: コンパイルして既存テストを通す**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|^warning: unused|^test result|FAILED" -A4 | head -30`
Expected: `test result: ok`、失敗 0。`ColumnFence` の `dead_code` 警告が消えている(Task 1 の警告は、ここで使われて解消する)。

- [ ] **Step 9: 書き込みがすべてヘルパー経由であることを静的に確認する**

Run:
```bash
cd src-tauri && grep -n "\.cache_notes(\|extend_fetch_boundaries(\|replace_fetch_boundaries(\|cache_fetched(\|apply_gap_fill_boundaries(" src/commands/column.rs | grep -v "^[0-9]*:\s*//"
```
Expected: 次の箇所**だけ**が出る(行番号は前後してよい)。
- 関数定義: `cache_fetched`、`apply_gap_fill_boundaries`
- `commit_backfill_writes` / `commit_initial_writes` / `commit_gap_fill_writes` の本体の中
- `#[cfg(test)]` の中(テスト)

`fetch_backfill`、`open_stream_and_fetch`、`resume_column`、`gap_fill_on_reconnect` の本体に、直接の呼び出しが残っていないこと。

Run: `cd src-tauri && sed -n '/^async fn commit_backfill_writes/,/^}/p;/^async fn commit_initial_writes/,/^}/p;/^async fn commit_gap_fill_writes/,/^}/p' src/commands/column.rs | grep -n "fetch_and_filter_multi\|fill_gap(\|fetch_notes\|client_for"`
Expected: 出力なし(ヘルパーの中にネットワーク呼び出しが無い = ロックをネットワーク取得の間に持たない)。

- [ ] **Step 10: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "fix: 取得中にupdate_column/close_columnが走った場合はキャッシュへ書かない"
```

## Task 4: フロントエンドのガード

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`TabView`、`#makeTab`、`updateColumn`、`loadMore`、`fillRemainingGap`、`fillGapBelow`)
- Modify(テストの `TabView` リテラル): `frontend/src/lib/store.svelte.test.ts`(3か所)、`frontend/src/ui/AddColumnModal.test.ts`(1か所)
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: なし(バックエンドとは独立した防御)。
- Produces: `TabView.epoch: number`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の末尾に追加する。`makeNoteTab`、`makeGroup`、`makeNote`、`invokeMock`、`ACCOUNT_ID` は既存。

```ts
describe("updateColumn をまたぐ backfill の結果は捨てる(Issue #446)", () => {
  const opened = (tabId: string) => ({
    column: {
      id: tabId,
      accountId: ACCOUNT_ID,
      kind: { type: "local" },
      order: 0,
      filter: { kind: "keywords", value: [] },
      notifyDesktop: false,
      notifySound: false,
      notifySoundChoice: "",
      groupId: "group1",
      title: null,
    },
    group: { id: "group1", order: 0, width: 400, auto: false },
    notes: [] as Note[],
    notifications: [],
  });

  /// 最初の fetch_backfill の応答を、テスト側が好きなタイミングで返せるようにする。
  /// 2回目以降は空で即座に返す(ループ中のページ取得がテストを止めないように)。
  function deferFirstBackfill(tabId: string) {
    let resolveFirst!: (notes: Note[]) => void;
    let calls = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") {
        calls += 1;
        if (calls === 1) return new Promise<Note[]>((resolve) => (resolveFirst = resolve));
        return [];
      }
      if (cmd === "update_column") return opened(tabId);
      if (cmd === "rename_column") return null;
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });
    return { resolveFirst: (notes: Note[]) => resolveFirst(notes) };
  }

  const local: ColumnKind = { type: "local" };
  const keywords: FilterQuery = { kind: "keywords", value: [] };

  it("loadMore: updateColumn をまたいで返った結果は tab.notes に足さない", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0002", createdAt: 2 })]);
    app.groups = [makeGroup([tab])];
    const backfill = deferFirstBackfill(tab.id);

    const pending = app.loadMore(tab.id);
    await app.updateColumn(tab.id, local, keywords);
    backfill.resolveFirst([makeNote({ id: "n0001", createdAt: 1 })]);
    await pending;

    const live = app.groups[0].tabs[0];
    expect(live.epoch).toBe(1);
    expect(live.notes).toEqual([]); // updateColumn が差し替えた一覧へ、旧フィルタの結果は混ざらない
  });

  it("loadMore: updateColumn が無ければ従来どおり結果を足す", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0002", createdAt: 2 })]);
    app.groups = [makeGroup([tab])];
    const backfill = deferFirstBackfill(tab.id);

    const pending = app.loadMore(tab.id);
    backfill.resolveFirst([makeNote({ id: "n0001", createdAt: 1 })]);
    await pending;

    expect(app.groups[0].tabs[0].notes.map((n) => n.id)).toEqual(["n0002", "n0001"]);
  });

  it("loadMore: 名前だけの変更は epoch を進めず、進行中の結果を捨てない", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0002", createdAt: 2 })]);
    app.groups = [makeGroup([tab])];
    const backfill = deferFirstBackfill(tab.id);

    const pending = app.loadMore(tab.id);
    await app.updateColumn(tab.id, tab.kind, tab.filter, "新しい名前"); // ソース/フィルタは同じ
    backfill.resolveFirst([makeNote({ id: "n0001", createdAt: 1 })]);
    await pending;

    const live = app.groups[0].tabs[0];
    expect(live.epoch).toBe(0);
    expect(live.notes.map((n) => n.id)).toEqual(["n0002", "n0001"]);
  });

  it("fillRemainingGap: updateColumn をまたいだ結果は混ぜず、ギャップマーカーも消さない", async () => {
    const marker = { boundaryId: "n0005", targetId: "n0001" };
    const tab = makeNoteTab([makeNote({ id: "n0005", createdAt: 5 })], { gapMarker: marker });
    app.groups = [makeGroup([tab])];
    const backfill = deferFirstBackfill(tab.id);

    const pending = app.fillRemainingGap(tab.id);
    await app.updateColumn(tab.id, local, keywords);
    // targetId(n0001)に到達する結果。ガードが無いとマーカーが「埋まった」として消える
    backfill.resolveFirst([makeNote({ id: "n0001", createdAt: 1 })]);
    await pending;

    const live = app.groups[0].tabs[0];
    expect(live.notes).toEqual([]);
    expect(live.gapMarker).toEqual(marker);
  });

  it("fillGapBelow: updateColumn をまたいだ結果は一覧に混ぜない", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0005", createdAt: 5 }), makeNote({ id: "n0003", createdAt: 3 })]);
    app.groups = [makeGroup([tab])];
    const backfill = deferFirstBackfill(tab.id);

    const pending = app.fillGapBelow(tab.id, "n0005");
    await app.updateColumn(tab.id, local, keywords);
    backfill.resolveFirst([makeNote({ id: "n0004", createdAt: 4 })]);
    await pending;

    expect(app.groups[0].tabs[0].notes).toEqual([]);
  });
});
```

テストファイル先頭の型 import(`import type { Note, Notification, User } from "../bindings/tauri.gen";`)に `ColumnKind` と `FilterQuery` を足す。

```ts
import type { ColumnKind, FilterQuery, Note, Notification, User } from "../bindings/tauri.gen";
```

`TabView` リテラルに `epoch: 0,` を足す(型を満たすため)。`selectionMoveSeq: 0,` の次の行に追加する。

- `frontend/src/lib/store.svelte.test.ts`: 110行目付近(通知タブ用)、384行目付近(`makeNormalTab`)、472行目付近(`makeNoteTab`)
- `frontend/src/ui/AddColumnModal.test.ts`: 52行目付近

```ts
    selectionMoveSeq: 0,
    epoch: 0,
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts -t "Issue #446" 2>&1 | grep -E "✓|✗|×|FAIL|passed|failed" | head -12`
Expected: `updateColumn をまたぐ` の4テスト(`loadMore` の1つ目、名前だけの変更、`fillRemainingGap`、`fillGapBelow`)が FAIL(`epoch` が undefined、または旧結果が `notes` に混ざる)。「updateColumn が無ければ従来どおり」は、ガード導入前から通る(対照)。

- [ ] **Step 3: 最小の実装を書く**

`TabView` の `fillingGap` の次に追加する。

```ts
  /// `updateColumn` がタブの内容(ソース/フィルタ)を差し替えるたびに増える世代カウンタ(Issue #446)。
  /// backfill 系の非同期処理は呼び出し前の値を控え、結果が返った時に変わっていたら捨てる。
  epoch: number;
```

`#makeTab` の返すオブジェクトに、`selectionMoveSeq: 0,` の次の行を追加する。

```ts
      selectionMoveSeq: 0,
      epoch: 0,
```

`updateColumn` の `if (tab) {` の最初の行に追加する。

```ts
    if (tab) {
      tab.epoch += 1;
      const acc = this.accounts.find((a) => a.id === opened.column.accountId);
```

`loadMore` のノート側(`const oldest = tab.notes[...]` の後)を置き換える。

```ts
        const oldest = tab.notes[tab.notes.length - 1].id;
        const epoch = tab.epoch;
        const older = await unwrap(commands.fetchBackfill(tab.id, oldest, false));
        // 取得中に updateColumn で内容が差し替わっていたら、旧フィルタの結果は捨てる(Issue #446)。
        if (tab.epoch !== epoch) return;
        const known = new Set(tab.notes.map((n) => n.id));
```

`fillRemainingGap` は、`tab.fillingGap = true;` の次の行に `const epoch = tab.epoch;` を足し、ループ内の `fetchBackfill` の直後に判定を足す。

```ts
    tab.fillingGap = true;
    const epoch = tab.epoch;
    const { targetId } = tab.gapMarker;
```

```ts
        const fetched = await unwrap(commands.fetchBackfill(tabId, boundaryId, true));
        // 取得中に updateColumn で内容が差し替わっていたら、結果を混ぜず、ギャップマーカーも触らない(Issue #446)。
        if (tab.epoch !== epoch) return;
        if (fetched.length === 0) break;
```

`fillGapBelow` も同じ形で足す。

```ts
    tab.fillingGap = true;
    const epoch = tab.epoch;
    let boundaryId = noteId;
```

```ts
        const fetched = await unwrap(commands.fetchBackfill(tabId, boundaryId, true));
        if (tab.epoch !== epoch) return;
        if (fetched.length === 0) break;
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts -t "Issue #446" 2>&1 | grep -E "passed|failed|FAIL"`
Expected: `5 passed`。

- [ ] **Step 5: 型チェックと全テストを通してコミットする**

Run: `cd frontend && pnpm check && pnpm test 2>&1 | grep -E "Test Files|Tests |FAIL|error"`
Expected: `svelte-check` がエラー 0。`pnpm test` は失敗 0。

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts frontend/src/ui/AddColumnModal.test.ts
git commit -m "fix: updateColumnをまたいで返ったbackfillの結果を画面へ混ぜない"
```

## Task 5: 全体の検証

**Files:** 変更なし(検証と、結果の記録のみ)。

- [ ] **Step 1: Rust の全テストと TS バインディングの差分を確認する**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"; cd .. && git diff --stat main -- frontend/src/bindings/tauri.gen.ts`
Expected: `test result: ok`(失敗 0)。バインディングの差分は空(出力なし)。

- [ ] **Step 2: フェンスを外すと、競合のテストが落ちることを確認する(変異確認)**

世代の確認を一時的に無効にして、Task 2・4 のテストが「旧フィルタの結果が書かれてしまう」ことで落ちることを確かめる。これで、テストが競合を実際に捕まえていると言える。

`src-tauri/src/fence.rs` の `write_if_current` の、世代を比べる3行を一時的にコメントアウトする。

```rust
        // if entry.epoch.load(Ordering::SeqCst) != epoch.0 {
        //     return None;
        // }
```

Run: `cd src-tauri && cargo test --lib commit_ 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: `commit_backfill_writes_writes_nothing_after_update_column`、`commit_initial_writes_does_not_overwrite_a_newer_update_column`、`commit_gap_fill_writes_writes_nothing_when_stale` が FAILED(旧世代の書き込みが通り、ノートが入り、境界が `n900` から動くため)。`commit_backfill_writes_leaves_no_orphans_after_close` は、エントリが無い場合の `None` が別の分岐なので、通ったままでよい。

確認したら、必ず元に戻す。

Run: `git checkout -- src-tauri/src/fence.rs && git status --short`
Expected: 作業ツリーはクリーン(コミット済みの実装に戻る)。`cargo test --lib commit_` が再び全部通ること。

- [ ] **Step 3: 実機での動作確認(回帰が無いこと)**

`CLAUDE.md` の方針どおり、実 UI の確認は仮想ディスプレイ越しに行い、ユーザーの実画面と実データには触れない(`WAYLAND_DISPLAY` を unset、Xvfb、`dbus-run-session`)。

Run: `sed -n 1,60p e2e/README.md`
Expected: E2E の前提(Docker Compose の Misskey、tauri-driver)が分かる。既存の操作 E2E に「カラムの編集(update_column)」や「上スクロールの backfill」が含まれていれば、それを実行する。環境が整わない場合は、実行しなかったことと理由を PR の検証欄に書く(「確認済み」と書かない)。

競合そのものは、手動では再現しにくい(窓が往復1回分)。再現と回帰防止は Task 1・2・4 の単体テストが担い、実機の確認は「通常の編集と上スクロールが壊れていないこと」に限る。

- [ ] **Step 4: ブランチの状態を確認して、ユーザーに報告する**

Run: `git status --short; git log --oneline main..HEAD`
Expected: 作業ツリーはクリーン。コミットは spec 2つ、plan 1つ、Task 1〜4 の4つ。

push と PR 作成は、ユーザーの指示を受けてから行う。PR 本文には `Fixes #446` を入れる(番号の参照だけでは自動クローズされない)。`gh pr create` に `--body-file` を渡す場合は、`.github/pull_request_template.md` の構造(概要、関連Issue、修正内容、影響範囲、検証)に手で合わせる。検証欄には、実行したコマンドと結果、実機確認を行ったかどうか(行っていなければ理由)をそのまま書く。
