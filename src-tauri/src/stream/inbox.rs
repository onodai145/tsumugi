//! Inbox: 受信ノートの重複排除。1本の WS を複数カラムで共有するため、同じノートが
//! 複数チャンネルから届きうる。NoteID で直近 N 件の重複を弾く（設計書§6）。

use std::collections::{HashSet, VecDeque};

/// 直近 `capacity` 件の NoteID を覚えておく固定長の重複排除フィルタ。
pub struct Dedup {
    seen: HashSet<String>,
    order: VecDeque<String>,
    capacity: usize,
}

impl Dedup {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// 初見なら true を返して記録する。既知なら false。
    pub fn accept(&mut self, note_id: &str) -> bool {
        if self.seen.contains(note_id) {
            return false;
        }
        self.seen.insert(note_id.to_string());
        self.order.push_back(note_id.to_string());
        if self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_new_rejects_duplicates() {
        let mut d = Dedup::new(8);
        assert!(d.accept("a"));
        assert!(d.accept("b"));
        assert!(!d.accept("a")); // 重複
        assert!(!d.accept("b"));
        assert!(d.accept("c"));
    }

    #[test]
    fn evicts_oldest_beyond_capacity() {
        let mut d = Dedup::new(2);
        assert!(d.accept("a"));
        assert!(d.accept("b"));
        assert!(d.accept("c")); // a を追い出す
        assert!(d.accept("a")); // a は忘れられたので再び新規扱い
        assert!(!d.accept("c")); // c はまだ覚えている
    }
}
