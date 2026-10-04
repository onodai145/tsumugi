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
    ///
    /// 境界の世代を `f` の実行**後**に読んでも安全な理由(Issue #456): 呼び出し側は、この関数が返った
    /// 後に、ミュート設定を読んで取得する。`set_mute` / サーバー側ミュートの同期は、ミュート設定を
    /// 差し替えてから `invalidate_boundaries` で世代を進める。したがって、ここで読んだ世代が、その
    /// 進行を含んでいれば、後続のミュート設定の読みは新しい設定を見る。含んでいなければ、後で
    /// 世代が進んだ時点で `boundaries_ok` が偽になり、境界の書き込みが飛ばされる。どちらでも、
    /// 旧い設定の結果に基づく境界は書かれない。(`begin` が、設定を読む前に世代を控えるのと同じ向き。)
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

    /// そのカラムのエントリがあるか(テスト用)。
    #[cfg(test)]
    pub(crate) fn tracks(&self, column_id: &str) -> bool {
        self.columns.lock().unwrap().contains_key(column_id)
    }

    /// エントリを消す(`close_column` が `invalidate` の後に呼ぶ)。以降、そのカラムの
    /// 旧い世代による `write_if_current` は `None` になる。
    pub fn remove(&self, column_id: &str) {
        self.columns.lock().unwrap().remove(column_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn write_runs_when_epoch_is_current() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        let out = fence.write_if_current("c1", &epoch, |_| async { 7 }).await;

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
        assert_eq!(fence.write_if_current("c1", &before, |_| async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &after, |_| async { 2 }).await, Some(2));
    }

    #[tokio::test]
    async fn second_invalidate_stales_the_first_returned_epoch() {
        let fence = ColumnFence::default();

        let (first, _) = fence.invalidate("c1", || async {}).await;
        let (second, _) = fence.invalidate("c1", || async {}).await;

        assert_eq!(fence.write_if_current("c1", &first, |_| async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &second, |_| async { 2 }).await, Some(2));
    }

    #[test]
    fn tracks_reports_whether_an_entry_exists() {
        let fence = ColumnFence::default();
        assert!(!fence.tracks("c1"));

        fence.begin("c1");
        assert!(fence.tracks("c1"));

        fence.remove("c1");
        assert!(!fence.tracks("c1"));
    }

    #[tokio::test]
    async fn write_is_stale_after_remove() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        fence.remove("c1");

        assert_eq!(fence.write_if_current("c1", &epoch, |_| async { 1 }).await, None);
    }

    #[tokio::test]
    async fn recreated_entry_never_reuses_an_old_epoch() {
        let fence = ColumnFence::default();
        let old = fence.begin("c1");
        fence.remove("c1");

        let new = fence.begin("c1");

        assert_ne!(old, new);
        assert_eq!(fence.write_if_current("c1", &old, |_| async { 1 }).await, None);
        assert_eq!(fence.write_if_current("c1", &new, |_| async { 2 }).await, Some(2));
    }

    #[tokio::test]
    async fn invalidate_of_one_column_does_not_stale_another() {
        let fence = ColumnFence::default();
        let other = fence.begin("c2");

        fence.invalidate("c1", || async {}).await;

        assert_eq!(fence.write_if_current("c2", &other, |_| async { 1 }).await, Some(1));
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
            async move { fence.write_if_current("c1", &epoch, |_| async { 42 }).await }
        });
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert!(!writing.is_finished(), "invalidate の実行中は書き込みが待たされる");

        gate.notify_one();
        invalidating.await.unwrap();

        assert_eq!(writing.await.unwrap(), None);
    }

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
}
