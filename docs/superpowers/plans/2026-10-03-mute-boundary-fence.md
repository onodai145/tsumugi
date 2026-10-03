# set_mute と実行中の取得の競合(境界の世代) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `set_mute` が捨てた backfill 境界を、実行中の REST 取得が書き込んで復活させないようにする(Issue #452)。

**Architecture:** `ColumnFence`(#446)に、全カラム共通の「境界の世代」と RwLock を足す。書き込みは境界の読みロックを持ち、`set_mute` は書きロックを取って境界を捨て、世代を進める。世代が古い書き込みは、古い情報から作った境界(`extend` / `replace`)の書き込みだけを飛ばし、ノートのキャッシュ・ストリームを開く処理・ギャップ埋めの境界の引き上げは、そのまま行う。

**Tech Stack:** Rust(tokio、tauri、rusqlite)。

**Spec:** `docs/superpowers/specs/2026-10-03-mute-boundary-fence-design.md`(承認済み。計画を書く途中で、ギャップ埋めの境界の引き上げを飛ばさない形に直した。コミット `18221a6`)。実装者は計画と spec の両方を読むこと。

## Global Constraints

- DB スキーマ、Tauri コマンドの署名、TS バインディング(`frontend/src/bindings/tauri.gen.ts`)、フロントエンドは変えない。`cargo test` 後に `git diff main -- frontend/src/bindings/tauri.gen.ts` が空であること。`Error` の variant も足さない。
- ロックの順序は常に「境界の RwLock → カラムの Mutex」。単一カラムの `invalidate` は境界のロックを取らない。`write_if_current` / `invalidate` / `invalidate_boundaries` に渡す `f` の中から、別の `write_if_current` / `invalidate` / `invalidate_boundaries` を呼ばない(入れ子にしない)。
- `set_mute` は、`state.mute` を差し替えた**後で** `invalidate_boundaries` を呼ぶ。書き込み側は、ミュート設定を読む(フィルタする)**前に** `begin` する(既存のすべての経路が満たしている。変えない)。
- 境界の世代が古い書き込みが飛ばすのは、`commit_backfill_writes` の `extend_fetch_boundaries` と、`commit_initial_writes` の `replace_fetch_boundaries` だけ。`commit_gap_fill_writes` の `apply_gap_fill_boundaries` は**飛ばさない**(最新の境界をロックの中で読み、既存の行を保守的な方向へ動かすだけで、`clear` 後の空の状態からは何も書かない。飛ばすとギャップが覆われないまま残る)。
- ノートのキャッシュ、ノートの返却、`open_streams_only`(`on_current`)、`ColumnGapFill` イベント、戻り値は、境界の世代に関わらず変えない。
- コミットメッセージは件名のみ・本文なし(末尾の `Co-Authored-By` トレーラーは別)。`--no-verify` は使わない。コミットが失敗したら止まって報告する。
- `main` には直接コミットしない。作業ブランチは `fix/mute-boundary-fence-452`(作成済み)。push と PR 作成は、ユーザーの指示を受けてから行う。PR 本文には `Fixes #452` を入れる。
- この環境では、既定のワーカー数でのフロントエンドの全テストが OOM で落ちた。今回はフロントエンドを変えないので、`pnpm test` は走らせない。

## Review Focus

- **旧ミュートで取得を始めた backfill が、`set_mute` の後に境界を復活させない**(Task 2 のテスト `commit_backfill_writes_caches_notes_but_does_not_resurrect_boundaries_after_set_mute`)。
- **カラムを開いている最中にミュートを変えても、カラムは開く**: ストリームが開き、ノートが返り、キャッシュされる(Task 2 のテスト `commit_initial_writes_keeps_notes_and_streams_but_skips_boundaries_after_set_mute`)。
- **ミュート変更で、ギャップ埋めの境界の引き上げが飛ばされて、ギャップが覆われないまま残らない**(Task 2 のテスト `commit_gap_fill_writes_still_raises_boundaries_when_only_the_boundary_epoch_is_stale`)。
- **`set_mute` が、実行中の書き込みと排他**: 書き込みの最中は `invalidate_boundaries` が待ち、`invalidate_boundaries` の最中は書き込みが待つ(Task 1 のテスト2件)。
- **ミュート変更の後に始めた取得は、従来どおり境界を書く**: 過剰に飛ばさない(Task 1 のテスト `invalidate_boundaries_makes_earlier_epochs_boundary_stale_but_still_current_for_the_column`、Task 2 の対照テスト)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/fence.rs` | `Epoch` に境界の世代を足す。`boundary_gen` と `boundary_lock`、`write_if_current` の `f` の引数(`boundaries_ok: bool`)、`invalidate_boundaries` |
| `src-tauri/src/commands/column.rs` | `commit_backfill_writes` / `commit_initial_writes` が `boundaries_ok` で境界の書き込みを飛ばす。`commit_gap_fill_writes` は引数を無視する |
| `src-tauri/src/commands/mute.rs` | `set_mute` の本体を `apply_mute_config` に切り出し、境界を捨てる処理を `invalidate_boundaries` の中で行う |

## Task 1: `ColumnFence` に境界の世代を足す

**Files:**
- Modify: `src-tauri/src/fence.rs`(実装とテスト)
- Modify: `src-tauri/src/commands/column.rs`(`commit_*` ヘルパー3つのクロージャの引数だけ。動作は変えない)

**Interfaces:**
- Consumes: #446 の `ColumnFence`(`begin`、`write_if_current`、`invalidate`、`remove`)と `Epoch`。
- Produces(以降のタスクが使う):
  - `Epoch` が、カラムの世代と境界の世代の両方を持つ(`#[derive(Debug, Clone, PartialEq, Eq)]`)
  - `pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T> where F: FnOnce(bool) -> Fut, Fut: Future<Output = T>` — `f` の引数 `boundaries_ok` は、控えた境界の世代がいまも現在か
  - `pub async fn invalidate_boundaries<F, Fut, T>(&self, f: F) -> T where F: FnOnce() -> Fut, Fut: Future<Output = T>`
  - `invalidate` が返す `Epoch` は、その時点の境界の世代を持つ

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/fence.rs` の `mod tests` の中で、既存の `write_if_current` の呼び出しのクロージャを機械的に直す(引数が増えるため)。`#[cfg(test)]` より後ろの部分だけを対象にする。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && python3 - <<'EOF'
import re
p = 'src-tauri/src/fence.rs'
s = open(p).read()
i = s.index('#[cfg(test)]')
head, tests = s[:i], s[i:]
tests, n = re.subn(r'(write_if_current\([^)]*?)\|\| async', r'\1|_| async', tests)
print('rewritten closures:', n)
open(p, 'w').write(head + tests)
EOF
```
Expected: `rewritten closures:` が、既存のテストの `write_if_current` の呼び出しの数(10前後)になる。

同じ `mod tests` の末尾(最後の `}` の前)に、新しいテストを追加する。

```rust
    #[tokio::test]
    async fn boundaries_are_ok_when_nothing_invalidated_them() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        let ok = fence.write_if_current("c1", &epoch, |ok| async move { ok }).await;

        assert_eq!(ok, Some(true));
    }

    #[tokio::test]
    async fn invalidate_boundaries_makes_earlier_epochs_boundary_stale_but_still_current_for_the_column() {
        let fence = ColumnFence::default();
        let before = fence.begin("c1");

        let ran = fence.invalidate_boundaries(|| async { "cleared" }).await;

        assert_eq!(ran, "cleared");
        // カラムの世代は変わっていないので書き込み自体は通る(`None` にならない)が、境界は古い
        assert_eq!(fence.write_if_current("c1", &before, |ok| async move { ok }).await, Some(false));
        // その後に控えた世代は、境界も現在
        let after = fence.begin("c1");
        assert_eq!(fence.write_if_current("c1", &after, |ok| async move { ok }).await, Some(true));
    }

    #[tokio::test]
    async fn invalidate_boundaries_affects_every_column() {
        let fence = ColumnFence::default();
        let (a, b) = (fence.begin("c1"), fence.begin("c2"));

        fence.invalidate_boundaries(|| async {}).await;

        assert_eq!(fence.write_if_current("c1", &a, |ok| async move { ok }).await, Some(false));
        assert_eq!(fence.write_if_current("c2", &b, |ok| async move { ok }).await, Some(false));
    }

    #[tokio::test]
    async fn invalidate_returns_an_epoch_with_the_current_boundary_generation() {
        let fence = ColumnFence::default();
        fence.invalidate_boundaries(|| async {}).await;

        let (epoch, _) = fence.invalidate("c1", || async {}).await;

        assert_eq!(fence.write_if_current("c1", &epoch, |ok| async move { ok }).await, Some(true));
    }

    #[tokio::test]
    async fn invalidate_boundaries_waits_for_a_write_in_progress() {
        let fence = Arc::new(ColumnFence::default());
        let epoch = fence.begin("c1");
        let entered = Arc::new(tokio::sync::Notify::new());
        let gate = Arc::new(tokio::sync::Notify::new());

        let writing = tokio::spawn({
            let (fence, entered, gate) = (fence.clone(), entered.clone(), gate.clone());
            async move {
                fence
                    .write_if_current("c1", &epoch, move |_| async move {
                        entered.notify_one();
                        gate.notified().await;
                        1
                    })
                    .await
            }
        });
        entered.notified().await; // 書き込みが f の実行中(=境界の読みロックを持っている)

        let invalidating = tokio::spawn({
            let fence = fence.clone();
            async move { fence.invalidate_boundaries(|| async { "done" }).await }
        });
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert!(!invalidating.is_finished(), "書き込みの実行中は invalidate_boundaries が待たされる");

        gate.notify_one();
        assert_eq!(writing.await.unwrap(), Some(1));
        assert_eq!(invalidating.await.unwrap(), "done");
    }

    #[tokio::test]
    async fn write_waits_for_invalidate_boundaries_in_progress_then_sees_boundaries_stale() {
        let fence = Arc::new(ColumnFence::default());
        let epoch = fence.begin("c1");
        let entered = Arc::new(tokio::sync::Notify::new());
        let gate = Arc::new(tokio::sync::Notify::new());

        let invalidating = tokio::spawn({
            let (fence, entered, gate) = (fence.clone(), entered.clone(), gate.clone());
            async move {
                fence
                    .invalidate_boundaries(move || async move {
                        entered.notify_one();
                        gate.notified().await;
                    })
                    .await
            }
        });
        entered.notified().await; // invalidate_boundaries が f の実行中(=書きロックを持っている)

        let writing = tokio::spawn({
            let fence = fence.clone();
            async move { fence.write_if_current("c1", &epoch, |ok| async move { ok }).await }
        });
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert!(!writing.is_finished(), "invalidate_boundaries の実行中は書き込みが待たされる");

        gate.notify_one();
        invalidating.await.unwrap();

        assert_eq!(writing.await.unwrap(), Some(false));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib fence:: 2>&1 | grep -E "^error" | sed 's/`[^`]*`/X/g' | sort | uniq -c`
Expected: `no method named invalidate_boundaries` と、`write_if_current` のクロージャの引数の数が合わない(`E0593`)などのコンパイルエラー。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/fence.rs` の `#[cfg(test)]` より**前**の部分(ファイルの先頭から `impl ColumnFence { ... }` の終わりまで)を、次の内容に丸ごと置き換える。`mod tests` は触らない。

```rust
//! カラムごとの世代(epoch)とロック(Issue #446)、全カラム共通の境界の世代(Issue #452)。
//!
//! `update_column` / `close_column` が実行中の REST 取得と競合して、旧定義の結果や古い境界を
//! カラムのキャッシュへ書かせないための仕組み。書き込み側は処理の開始時に `begin` で世代を控え、
//! 書き込みの直前だけ `write_if_current` でロックを取って世代を確認する。ロックは DB 書き込みの
//! 間だけ持ち、ネットワーク取得の間は持たない。
//!
//! `set_mute` は、全カラムの backfill 境界を捨てる。実行中の取得が、旧ミュートで作った境界を
//! 書き込んで復活させないよう、境界には別の世代(`boundary_gen`)と RwLock を持つ。書き込みは
//! 読みロック、`invalidate_boundaries` は書きロックを取る。ロックの順序は常に「境界 → カラム」。
//! 設計は docs/superpowers/specs/2026-10-03-column-write-fence-design.md と
//! docs/superpowers/specs/2026-10-03-mute-boundary-fence-design.md。

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// `begin` / `invalidate` が返す、控えた時点のカラムの世代と境界の世代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epoch {
    column: u64,
    boundary: u64,
}

struct Entry {
    epoch: AtomicU64,
    lock: tokio::sync::Mutex<()>,
}

#[derive(Default)]
pub struct ColumnFence {
    /// 世代の払い出し。プロセス全体で単調増加するので、`remove` で消して作り直したエントリの
    /// 世代や、境界の世代が、以前のものと一致することはない。
    next: AtomicU64,
    /// 境界の世代(全カラム共通)。`invalidate_boundaries` が進める。
    boundary_gen: AtomicU64,
    /// 書き込み(読みロック)と `invalidate_boundaries`(書きロック)の排他。
    boundary_lock: tokio::sync::RwLock<()>,
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

    /// 現在の世代(カラムと境界)を控える。エントリが無ければ作る。ロックは取らない。
    pub fn begin(&self, column_id: &str) -> Epoch {
        Epoch {
            column: self.entry_or_create(column_id).epoch.load(Ordering::SeqCst),
            boundary: self.boundary_gen.load(Ordering::SeqCst),
        }
    }

    /// 境界の読みロックとカラムのロックをこの順に取り、カラムの世代が `epoch` と一致する時だけ
    /// `f` を実行する。不一致、またはエントリが無い(`remove` 済み)なら `f` を実行せず `None`
    /// (=古い)を返す。`f` には、控えた境界の世代がいまも現在か(`boundaries_ok`)が渡される。
    /// 偽のとき、`f` は「古い情報から作った境界」の書き込みを飛ばすこと。
    pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T>
    where
        F: FnOnce(bool) -> Fut,
        Fut: Future<Output = T>,
    {
        let _boundary = self.boundary_lock.read().await;
        let entry = self.columns.lock().unwrap().get(column_id).cloned()?;
        let _held = entry.lock.lock().await;
        if entry.epoch.load(Ordering::SeqCst) != epoch.column {
            return None;
        }
        let boundaries_ok = self.boundary_gen.load(Ordering::SeqCst) == epoch.boundary;
        Some(f(boundaries_ok).await)
    }

    /// ロックを取り、`f`(新しい定義の保存と clear)を実行し、**その後で**カラムの世代を進める。
    /// 進めた後の世代を返す(`f` が失敗しても進める。定義が中途半端に更新されうるため)。
    /// 返す `Epoch` の境界の世代は、その時点のもの。
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
        (Epoch { column: next, boundary: self.boundary_gen.load(Ordering::SeqCst) }, result)
    }

    /// 境界の書きロックを取り、`f`(全カラムの境界を捨てる処理)を実行し、その後で境界の世代を進める。
    /// 書きロックは、実行中の書き込み(読みロックを持つ)の完了を待つので、`f` が捨てた境界を、
    /// 直後に書き込みが復活させる隙間が無い。以前に控えた境界の世代は、`f` の後、古くなる。
    pub async fn invalidate_boundaries<F, Fut, T>(&self, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _write = self.boundary_lock.write().await;
        let result = f().await;
        self.boundary_gen.store(self.issue(), Ordering::SeqCst);
        result
    }

    /// エントリを消す(`close_column` が `invalidate` の後に呼ぶ)。以降、そのカラムの
    /// 旧い世代による `write_if_current` は `None` になる。
    pub fn remove(&self, column_id: &str) {
        self.columns.lock().unwrap().remove(column_id);
    }
}

```

`src-tauri/src/commands/column.rs` の `commit_*` ヘルパー3つのクロージャを、引数を受け取る形に機械的に直す(この時点では引数を使わない。動作は変えない)。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && python3 - <<'EOF'
p = 'src-tauri/src/commands/column.rs'
s = open(p).read()

def rep(old, new):
    global s
    assert s.count(old) == 1, old[:60]
    s = s.replace(old, new)

# commit_backfill_writes
rep('''        .write_if_current(column_id, epoch, || async {
            cache_fetched(cache, column_id, fetch).await?;
            if !extend.is_empty() {''',
    '''        .write_if_current(column_id, epoch, |_boundaries_ok| async {
            cache_fetched(cache, column_id, fetch).await?;
            if !extend.is_empty() {''')
# commit_initial_writes
rep('''        .write_if_current(column_id, epoch, || async move {
            cache_fetched(cache, column_id, fetch).await?;
            if let Some(entries) = boundaries {''',
    '''        .write_if_current(column_id, epoch, |_boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            if let Some(entries) = boundaries {''')
# commit_gap_fill_writes
rep('''        .write_if_current(column_id, epoch, || async {
            apply_gap_fill_boundaries(cache, column_id, gap).await;''',
    '''        .write_if_current(column_id, epoch, |_boundaries_ok| async {
            apply_gap_fill_boundaries(cache, column_id, gap).await;''')
open(p, 'w').write(s)
EOF
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib fence:: 2>&1 | grep -E "^error|^test .*FAILED|test result"`
Expected: `test result: ok`(既存8件 + 新規6件、失敗 0)。

- [ ] **Step 5: 全体が壊れていないことを確認してコミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0(`commit_*` ヘルパーの既存テストが、動作の変化なしで通ること)。

```bash
git add src-tauri/src/fence.rs src-tauri/src/commands/column.rs
git commit -m "feat: ColumnFenceに全カラム共通の境界の世代と書きロックを足す"
```

## Task 2: `commit_*` ヘルパーが境界の書き込みを飛ばす

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`commit_backfill_writes`、`commit_initial_writes`、`mod tests`)

**Interfaces:**
- Consumes: Task 1 の `write_if_current` の `f: FnOnce(bool)`、`invalidate_boundaries`。既存の `commit_*` ヘルパー、テスト用ヘルパー(`mem_cache`、`fetch_of`、`pair`、`gap_with`、`simulate_update_column`、`note`)。
- Produces: `commit_backfill_writes` と `commit_initial_writes` の外向きの引数・戻り値は変わらない。`boundaries_ok` が偽なら、それぞれ `extend_fetch_boundaries` / `replace_fetch_boundaries` を飛ばす。

- [ ] **Step 1: 失敗するテストを書く**

`column.rs` の `mod tests` の末尾に追加する(`commit_gap_fill_writes_writes_nothing_when_stale` の後ろ、最後の `}` の前)。

```rust
    /// `set_mute` が行うこと(境界の書きロックの中で、全カラムの境界を捨てる)を模す。
    async fn simulate_set_mute(fence: &ColumnFence, cache: &NoteCacheStore) {
        fence
            .invalidate_boundaries(|| async { cache.clear_all_fetch_boundaries().await.unwrap() })
            .await;
    }

    #[tokio::test]
    async fn commit_backfill_writes_caches_notes_but_does_not_resurrect_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1"); // 旧ミュートで取得を始めた
        simulate_set_mute(&fence, &cache).await;

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")]).await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1); // ノートは書かれる
        // `extend` は行が無ければ挿入するので、飛ばさないと、捨てた境界が復活する
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_backfill_writes_extends_boundaries_when_the_epoch_was_begun_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        simulate_set_mute(&fence, &cache).await;
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1"); // ミュート変更の後に取得を始めた(対照)

        commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n300")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_keeps_notes_and_streams_but_skips_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 旧ミュートでカラムを開き始めた
        simulate_set_mute(&fence, &cache).await;
        let opened = std::cell::Cell::new(false);

        let written = commit_initial_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_of(&["n400"]),
            Some(&[pair(0, "n100")]),
            || opened.set(true),
        )
        .await;

        assert!(written.is_ok(), "ミュートを変えても、カラムは開ける(Err にしない)");
        assert!(opened.get(), "ストリームは開く");
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        // 旧ミュートの結果に基づく境界は書かない。未確定のままなら、次回の backfill は API 経由になる
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_does_not_resurrect_cleared_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_set_mute(&fence, &cache).await; // 境界は空

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        // 引き上げは既存の行にしか効かない(`prev` が空なら何も書かない)ので、復活しない
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_still_raises_boundaries_when_only_the_boundary_epoch_is_stale() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_set_mute(&fence, &cache).await;
        // ミュート変更の後に、別の取得が作った行
        cache.replace_fetch_boundaries("c1", &[pair(0, "n400")]).await.unwrap();

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        // 引き上げを飛ばすと、打ち切られたギャップが境界で覆われないまま残る
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n450")]);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib commit_ 2>&1 | grep -E "^error|^test .*(FAILED|ok)$|test result"`
Expected: コンパイルは通る(Task 1 で `invalidate_boundaries` がある)。次の2件が **FAILED**(境界が復活する/書かれる)。
- `commit_backfill_writes_caches_notes_but_does_not_resurrect_boundaries_after_set_mute`
- `commit_initial_writes_keeps_notes_and_streams_but_skips_boundaries_after_set_mute`

次の3件は、実装前でも通る(対照と、「飛ばさない」を固定するテスト)。
- `commit_backfill_writes_extends_boundaries_when_the_epoch_was_begun_after_set_mute`
- `commit_gap_fill_writes_does_not_resurrect_cleared_boundaries_after_set_mute`
- `commit_gap_fill_writes_still_raises_boundaries_when_only_the_boundary_epoch_is_stale`

- [ ] **Step 3: 最小の実装を書く**

`commit_backfill_writes` のクロージャを置き換える。

```rust
        .write_if_current(column_id, epoch, |boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            // 境界の世代が古い(取得中に set_mute が境界を捨てた)なら、旧ミュートの結果に基づく
            // 延長を書かない。`extend` は行が無ければ挿入するので、書くと捨てた境界が復活する(Issue #452)。
            if boundaries_ok && !extend.is_empty() {
                let _ = cache.extend_fetch_boundaries(column_id, extend).await;
            }
            Ok::<(), Error>(())
        })
```

`commit_initial_writes` のクロージャを置き換える。

```rust
        .write_if_current(column_id, epoch, |boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            // 境界の世代が古いなら、旧ミュートの結果に基づく境界を書かない。未確定のままなら、
            // 次回の backfill は API 経由になるので安全(Issue #452)。
            if boundaries_ok {
                if let Some(entries) = boundaries {
                    let _ = cache.replace_fetch_boundaries(column_id, entries).await;
                }
            }
            on_current();
            Ok::<(), Error>(())
        })
```

`commit_gap_fill_writes` は変えない(`|_boundaries_ok|` のまま)。ただし、理由が分かるよう、関数の doc コメントの末尾に1行足す。

```rust
/// 境界の世代が古くても、引き上げは飛ばさない: 引き上げは、ロックの中で読んだ最新の境界の既存の行を
/// 保守的な方向へ動かすだけで、`set_mute` が捨てた後の空の状態からは何も書かない。飛ばすと、
/// 打ち切られたギャップが境界で覆われないまま残る(Issue #452)。
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib commit_ 2>&1 | grep -E "^error|^test .*FAILED|test result"`
Expected: `test result: ok`、失敗 0。

- [ ] **Step 5: コミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0。

```bash
git add src-tauri/src/commands/column.rs
git commit -m "fix: set_mute後に旧ミュートの結果に基づく境界を書き込まない"
```

## Task 3: `set_mute` の配線

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`(`set_mute` の本体を切り出し、`mod tests` にテストを追加)

**Interfaces:**
- Consumes: Task 1 の `invalidate_boundaries`。`AppState`(`mute`、`settings`、`cache`、`column_fence`)。
- Produces: `async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()>`(private。`set_mute` から呼ばれる。テストのために切り出す)。

- [ ] **Step 1: 失敗するテストを書く**

`mute.rs` の最初の `mod tests`(`use wiremock::...` の import がある方)の末尾に追加する。

```rust
    #[tokio::test]
    async fn apply_mute_config_clears_boundaries_and_stales_earlier_boundary_epochs() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.cache.replace_fetch_boundaries("c1", &[(0, "n500".to_string())]).await.unwrap();
        let epoch = state.column_fence.begin("c1"); // 旧ミュートで取得を始めた

        apply_mute_config(&state, MuteConfig::default()).await.unwrap();

        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
        let boundaries_ok = state.column_fence.write_if_current("c1", &epoch, |ok| async move { ok }).await;
        assert_eq!(boundaries_ok, Some(false), "旧ミュートで控えた境界の世代は古くなる");
    }

    #[tokio::test]
    async fn apply_mute_config_replaces_the_mute_config() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let config = MuteConfig { ng_words: vec!["secret".to_string()], ..MuteConfig::default() };

        apply_mute_config(&state, config.clone()).await.unwrap();

        // 新しい設定が `state.mute` に反映される。差し替えが境界を捨てる前に行われる順序(書き込み側の
        // 不変条件)は、このテストでは検証できない。`apply_mute_config` のコードの順序で守る
        assert_eq!(*state.mute.lock().unwrap(), config);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib apply_mute_config 2>&1 | grep -E "^error" | sed 's/`[^`]*`/X/g' | sort | uniq -c`
Expected: `cannot find function apply_mute_config` のコンパイルエラー。

- [ ] **Step 3: 最小の実装を書く**

`mute.rs` の `set_mute` を、次の2つの関数に置き換える。

```rust
pub async fn set_mute(state: State<'_, AppState>, config: MuteConfig) -> Result<()> {
    apply_mute_config(&state, config).await
}

/// ミュート設定を保存して差し替え、全カラムの backfill 境界を捨てる(`set_mute` の本体)。
///
/// `state.mute` の差し替えは、境界を捨てる**前**に行う。書き込み側は、ミュート設定を読む前に
/// 世代を控えるので、境界の世代が進んだ時点で、新しい設定がすでに反映されている(Issue #452)。
/// 境界を捨てる処理は、境界の書きロックの中で行う。実行中の取得が、旧ミュートの結果に基づく境界を
/// 直後に書き込んで復活させないため(`ColumnFence::invalidate_boundaries`)。
async fn apply_mute_config(state: &AppState, config: MuteConfig) -> Result<()> {
    state.settings.save_mute(&config)?;
    *state.mute.lock().unwrap() = config;
    // ミュート解除方向の変更は、除外済み(=キャッシュされていない)ノートを読み直せないため
    // キャッシュ提供パスでは反映できない。境界を捨てて次回backfillをAPI経由に倒す(Issue #228)。
    state
        .column_fence
        .invalidate_boundaries(|| async {
            let _ = state.cache.clear_all_fetch_boundaries().await;
        })
        .await;
    Ok(())
}
```

`set_mute` の `#[tauri::command]` と `#[specta::specta]` の属性、doc コメントは、そのまま残す(関数の外向きの署名は変えない)。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib apply_mute_config 2>&1 | grep -E "^error|^test .*FAILED|test result"`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 5: コミットする**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|FAILED|test result"`
Expected: `test result: ok`、失敗 0。

```bash
git add src-tauri/src/commands/mute.rs
git commit -m "fix: set_muteの境界の破棄を境界の書きロックの中で行う"
```

## Task 4: 全体の検証

**Files:** 変更なし(検証と、結果の記録のみ)。

- [ ] **Step 1: Rust の全テストと TS バインディングの差分を確認する**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"; cd .. && echo "bindings diff vs main: [$(git diff --stat main -- frontend/src/bindings/tauri.gen.ts)]"; git status --short`
Expected: `test result: ok`(失敗 0)。バインディングの差分は空(`[]`)。作業ツリーはクリーン。

- [ ] **Step 2: 変異確認: 境界の世代の確認を外すと、競合のテストが落ちる**

`src-tauri/src/fence.rs` の `write_if_current` の、境界の世代を比べる行を、一時的に常に真にする。

```rust
        // MUTATION: let boundaries_ok = self.boundary_gen.load(Ordering::SeqCst) == epoch.boundary;
        let boundaries_ok = true;
```

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: 次のテストが FAILED になる(`boundaries_ok` が常に真になり、境界が書かれる/世代の比較が効かないため)。
- `commit_backfill_writes_caches_notes_but_does_not_resurrect_boundaries_after_set_mute`
- `commit_initial_writes_keeps_notes_and_streams_but_skips_boundaries_after_set_mute`
- `invalidate_boundaries_makes_earlier_epochs_boundary_stale_but_still_current_for_the_column`
- `invalidate_boundaries_affects_every_column`
- `apply_mute_config_clears_boundaries_and_stales_earlier_boundary_epochs`
- `write_waits_for_invalidate_boundaries_in_progress_then_sees_boundaries_stale`

「飛ばさない」を固定するテスト(`commit_gap_fill_writes_still_raises_boundaries_when_only_the_boundary_epoch_is_stale` など)は、この変異では落ちない。それでよい。

確認したら、必ず元に戻す。

Run: `git checkout -- src-tauri/src/fence.rs && git status --short`
Expected: 作業ツリーはクリーン(コミット済みの実装に戻る)。`cargo test --lib fence:: commit_ apply_mute_config` が再び全部通ること。

- [ ] **Step 3: 静的確認: 境界の書き込みがすべてヘルパー経由で、飛ばす条件が正しい**

Run: `cd src-tauri && grep -n "boundaries_ok" src/commands/column.rs src/commands/mute.rs src/fence.rs | grep -v "^src/fence.rs.*//"`
Expected: `commit_backfill_writes` と `commit_initial_writes` が `boundaries_ok` を使って境界の書き込みを条件付きにしていること。`commit_gap_fill_writes` は `_boundaries_ok`(無視)であること。

Run: `cd src-tauri && grep -n "extend_fetch_boundaries(\|replace_fetch_boundaries(\|clear_all_fetch_boundaries(" src/commands/column.rs src/commands/mute.rs | awk -F: '$2 < 2200'`
Expected: `extend_fetch_boundaries` は `commit_backfill_writes` の中、`replace_fetch_boundaries` は `commit_initial_writes` と `apply_gap_fill_boundaries` の中、`clear_all_fetch_boundaries` は `apply_mute_config` の `invalidate_boundaries` の中だけ(テストを除く)。

- [ ] **Step 4: 実機確認の扱いを記録する**

実アプリでの確認は、#446 / PR #451 と同じ理由で行わない(競合の窓が往復1回分で、手動では再現できない。既存の E2E は、ミュートも backfill も通らない)。PR の検証欄に、そのままの事実を書く。`set_mute` の配線は、`apply_mute_config` のテスト(境界が空になり、世代が進み、`state.mute` が先に差し替わっていること)で確認する。`#[tauri::command]` の `set_mute` 自体(`State` の取り出し)は、単体テストを書けないので、コードの確認のみ。

- [ ] **Step 5: ブランチの状態を確認して、ユーザーに報告する**

Run: `git status --short; git log --oneline main..HEAD`
Expected: 作業ツリーはクリーン。コミットは spec 2つ、plan 1つ、Task 1〜3 の3つ。

push と PR 作成は、ユーザーの指示を受けてから行う。PR 本文には `Fixes #452` を入れ、`.github/pull_request_template.md` の構造(概要、関連Issue、修正内容、影響範囲、検証)に手で合わせる。検証欄には、実行したコマンドと結果、実機確認を行っていないこと(理由つき)を、そのまま書く。
