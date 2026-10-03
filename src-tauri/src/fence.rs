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
