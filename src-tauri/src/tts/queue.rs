//! 読み上げの待ち行列(Issue #625)。音声・時刻・Tauri から独立した純粋なロジックで、単体テストで完結する。
//!
//! 方針: 全体で1本の FIFO(声が重ならないようにする)。上限は**カラムごと**で、あるカラムの待ち件数が
//! 上限を超えたら、そのカラムの最古の項目を捨てる。同じノート ID は二重に積まない。

use super::engine::SynthParams;
use std::collections::VecDeque;

/// 重複排除のために覚えておく、直近のノート ID の数。
pub const RECENT_IDS_MAX: usize = 256;

#[derive(Debug, Clone)]
pub struct SpeechItem {
    pub note_id: String,
    pub column_id: String,
    /// フロントが整形した読み上げ文。
    pub text: String,
    /// 解決済みの声の ID(カラムの声、なければ既定の声)。
    pub voice_id: String,
    pub params: SynthParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// 積んだ。`dropped` は、上限を超えたために捨てた(そのカラムの最古の)件数。
    Accepted { dropped: usize },
    /// 同じノート ID が待ち中、または直近に読み上げ済みなので積まなかった。
    Duplicate,
}

#[derive(Debug, Default)]
pub struct SpeechQueue {
    items: VecDeque<SpeechItem>,
    /// 待ち中または直近に取り出したノート ID(重複排除用。古いものから忘れる)。
    recent: VecDeque<String>,
}

impl SpeechQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn push(&mut self, item: SpeechItem, column_limit: usize) -> PushOutcome {
        if self.recent.iter().any(|id| *id == item.note_id) {
            return PushOutcome::Duplicate;
        }
        let limit = column_limit.max(1);
        let column_id = item.column_id.clone();
        self.recent.push_back(item.note_id.clone());
        while self.recent.len() > RECENT_IDS_MAX {
            self.recent.pop_front();
        }
        self.items.push_back(item);

        let mut dropped = 0;
        while self
            .items
            .iter()
            .filter(|i| i.column_id == column_id)
            .count()
            > limit
        {
            // そのカラムの最古の項目を捨てる(必ず見つかる: 上のループ条件が真)。
            if let Some(pos) = self.items.iter().position(|i| i.column_id == column_id) {
                self.items.remove(pos);
                dropped += 1;
            }
        }
        PushOutcome::Accepted { dropped }
    }

    pub fn pop(&mut self) -> Option<SpeechItem> {
        self.items.pop_front()
    }

    /// 指定カラムの待ち項目を消し、消した件数を返す。
    pub fn remove_column(&mut self, column_id: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|i| i.column_id != column_id);
        before - self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::engine::SynthParams;

    fn item(note: &str, col: &str) -> SpeechItem {
        SpeechItem {
            note_id: note.into(),
            column_id: col.into(),
            text: format!("text-{note}"),
            voice_id: "1".into(),
            params: SynthParams {
                speed_scale: 1.0,
                volume: 1.0,
            },
        }
    }

    fn ids(q: &mut SpeechQueue) -> Vec<String> {
        std::iter::from_fn(|| q.pop()).map(|i| i.note_id).collect()
    }

    #[test]
    fn items_come_out_in_arrival_order_across_columns() {
        let mut q = SpeechQueue::new();
        q.push(item("n1", "a"), 5);
        q.push(item("n2", "b"), 5);
        q.push(item("n3", "a"), 5);
        assert_eq!(ids(&mut q), ["n1", "n2", "n3"]);
    }

    #[test]
    fn exceeding_a_columns_limit_drops_that_columns_oldest() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 2);
        q.push(item("b1", "b"), 2);
        q.push(item("a2", "a"), 2);
        let out = q.push(item("a3", "a"), 2);
        assert_eq!(out, PushOutcome::Accepted { dropped: 1 });
        assert_eq!(
            ids(&mut q),
            ["b1", "a2", "a3"],
            "a の最古(a1)だけが消え、b は無傷"
        );
    }

    #[test]
    fn a_limit_of_one_keeps_only_the_newest_of_that_column() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 1);
        q.push(item("a2", "a"), 1);
        q.push(item("a3", "a"), 1);
        assert_eq!(ids(&mut q), ["a3"]);
    }

    #[test]
    fn a_limit_of_zero_is_treated_as_one() {
        let mut q = SpeechQueue::new();
        assert_eq!(
            q.push(item("a1", "a"), 0),
            PushOutcome::Accepted { dropped: 0 }
        );
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn the_same_note_is_not_queued_twice_even_from_another_column() {
        let mut q = SpeechQueue::new();
        assert_eq!(
            q.push(item("n1", "a"), 5),
            PushOutcome::Accepted { dropped: 0 }
        );
        assert_eq!(q.push(item("n1", "b"), 5), PushOutcome::Duplicate);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn a_note_already_taken_out_is_still_recognised_as_a_duplicate() {
        let mut q = SpeechQueue::new();
        q.push(item("n1", "a"), 5);
        assert!(q.pop().is_some());
        assert_eq!(
            q.push(item("n1", "a"), 5),
            PushOutcome::Duplicate,
            "読み上げ済みのノートは再度積まない"
        );
    }

    #[test]
    fn the_recent_id_memory_is_bounded() {
        let mut q = SpeechQueue::new();
        for i in 0..(RECENT_IDS_MAX + 10) {
            q.push(item(&format!("n{i}"), "a"), 1000);
            q.pop();
        }
        // 十分古い ID は忘れる(メモリが増え続けない)。
        assert_eq!(
            q.push(item("n0", "a"), 5),
            PushOutcome::Accepted { dropped: 0 }
        );
    }

    #[test]
    fn remove_column_removes_only_its_items_and_reports_the_count() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 5);
        q.push(item("b1", "b"), 5);
        q.push(item("a2", "a"), 5);
        assert_eq!(q.remove_column("a"), 2);
        assert_eq!(ids(&mut q), ["b1"]);
        assert_eq!(q.remove_column("nope"), 0);
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 5);
        q.clear();
        assert_eq!(q.len(), 0);
        assert!(q.pop().is_none());
    }
}
