//! クライアント側の予約投稿(Issue #60 B)の永続化。`store/draft.rs` の `DraftStore` と同じ流儀で、
//! プレーンテキスト(JSON)1 ファイル(app_config_dir/scheduled_posts.json)に保存する。
//! ユーザーが書いた再取得不能なデータなので、破棄前提のノートキャッシュ DB には置かない。
//!
//! E2E(`e2e/helpers/sessionHooks.ts`)がこのファイルを直接書き換える。項目名(`scheduledAt`、
//! `status` の値など)を変えたら、E2E も直すこと。

use crate::domain::{LocalScheduleStatus, LocalScheduledNote, ScheduledNote};
use crate::error::{Error, Result};
use crate::store::draft::DraftInput;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// 起動時に `Posting` のまま残っていた予約に付ける理由(送信後に終了した可能性があり、再送しない)。
pub const UNKNOWN_RESULT_MESSAGE: &str = "投稿結果が不明です。投稿されたか確認してください";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPostEntry {
    pub id: String,
    pub account_id: String,
    /// 予約日時(epoch 秒)。
    pub scheduled_at: i64,
    pub input: DraftInput,
    pub status: LocalScheduleStatus,
    #[serde(default)]
    pub error: Option<String>,
    /// これまでに失敗した(再試行の対象になった)回数。
    #[serde(default)]
    pub attempts: u32,
    /// 再試行の予定時刻(epoch 秒)。無ければ `scheduled_at`。
    #[serde(default)]
    pub next_attempt_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    posts: Vec<ScheduledPostEntry>,
}

enum Backing {
    File(PathBuf),
    Memory,
}

pub struct ScheduledPostStore {
    backing: Backing,
    data: Mutex<Data>,
}

impl ScheduledPostEntry {
    /// 次に処理してよい時刻(epoch 秒)。
    pub fn ready_at(&self) -> i64 {
        self.next_attempt_at.unwrap_or(self.scheduled_at).max(self.scheduled_at)
    }

    /// 一覧(`ScheduledModal`)用の形。中身は A の `ScheduledNote` と同じ項目。
    pub fn to_local_note(&self) -> LocalScheduledNote {
        let i = &self.input;
        LocalScheduledNote {
            note: ScheduledNote {
                id: self.id.clone(),
                scheduled_at: self.scheduled_at,
                text: i.text.clone(),
                cw: i.cw.clone(),
                visibility: i.visibility,
                local_only: i.local_only,
                reaction_acceptance: i.reaction_acceptance,
                channel_id: i.channel_id.clone(),
                poll: i.poll.clone(),
                file_ids: i.file_ids.clone(),
                reply_note: i.reply_note.clone(),
                quote_note: i.quote_note.clone(),
            },
            status: self.status,
            error: self.error.clone(),
        }
    }
}

impl ScheduledPostStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        let data = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)?
        } else {
            Data::default()
        };
        Ok(Self { backing: Backing::File(path), data: Mutex::new(data) })
    }

    /// メモリだけの版。`AppState::new*` の既定(起動時に `with_scheduled_posts` で永続版へ差し替える)と、
    /// テストで使う。
    pub(crate) fn new_in_memory() -> Self {
        Self { backing: Backing::Memory, data: Mutex::new(Data::default()) }
    }

    fn save(&self, data: &Data) -> Result<()> {
        if let Backing::File(path) = &self.backing {
            let json = serde_json::to_string_pretty(data)?;
            let tmp_path = path.with_extension("json.tmp");
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, path)?;
        }
        Ok(())
    }

    /// `id` の予約に `f` を適用して保存する。無ければ None。
    fn with_entry<R>(&self, id: &str, f: impl FnOnce(&mut ScheduledPostEntry) -> R) -> Result<Option<R>> {
        let mut g = self.data.lock().unwrap();
        let Some(entry) = g.posts.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        let out = f(entry);
        self.save(&g)?;
        Ok(Some(out))
    }

    pub fn add(&self, account_id: &str, scheduled_at: i64, input: DraftInput, now: i64) -> Result<ScheduledPostEntry> {
        let mut g = self.data.lock().unwrap();
        let entry = ScheduledPostEntry {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.to_string(),
            scheduled_at,
            input,
            status: LocalScheduleStatus::Pending,
            error: None,
            attempts: 0,
            next_attempt_at: None,
            created_at: now,
        };
        g.posts.push(entry.clone());
        self.save(&g)?;
        Ok(entry)
    }

    pub fn get(&self, id: &str) -> Option<ScheduledPostEntry> {
        self.data.lock().unwrap().posts.iter().find(|e| e.id == id).cloned()
    }

    /// そのアカウントの予約を、予約日時の昇順(同時刻は作成順)で返す。
    pub fn list_for_account(&self, account_id: &str) -> Vec<ScheduledPostEntry> {
        let mut v: Vec<_> = self
            .data
            .lock()
            .unwrap()
            .posts
            .iter()
            .filter(|e| e.account_id == account_id)
            .cloned()
            .collect();
        v.sort_by_key(|e| (e.scheduled_at, e.created_at));
        v
    }

    /// 処理してよい `Pending` を、予約日時の古い順に返す。
    pub fn due(&self, now: i64) -> Vec<ScheduledPostEntry> {
        let mut v: Vec<_> = self
            .data
            .lock()
            .unwrap()
            .posts
            .iter()
            .filter(|e| e.status == LocalScheduleStatus::Pending && e.ready_at() <= now)
            .cloned()
            .collect();
        v.sort_by_key(|e| (e.scheduled_at, e.created_at));
        v
    }

    /// 次にスケジューラが起きるべき時刻(`Pending` の `ready_at` の最小)。
    pub fn next_wakeup(&self) -> Option<i64> {
        self.data
            .lock()
            .unwrap()
            .posts
            .iter()
            .filter(|e| e.status == LocalScheduleStatus::Pending)
            .map(|e| e.ready_at())
            .min()
    }

    /// 送信を始める。`Pending` / `Expired` / `Failed` を `Posting` にして返す。既に `Posting` か、無ければ None。
    /// `Mutex` を保持したまま遷移するので、同じ予約の送信は 1 回しか始まらない。
    pub fn begin_post(&self, id: &str) -> Result<Option<ScheduledPostEntry>> {
        let mut g = self.data.lock().unwrap();
        let Some(entry) = g.posts.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        if entry.status == LocalScheduleStatus::Posting {
            return Ok(None);
        }
        entry.status = LocalScheduleStatus::Posting;
        entry.error = None;
        let started = entry.clone();
        self.save(&g)?;
        Ok(Some(started))
    }

    pub fn finish_posted(&self, id: &str) -> Result<()> {
        let mut g = self.data.lock().unwrap();
        g.posts.retain(|e| e.id != id);
        self.save(&g)
    }

    pub fn finish_failed(&self, id: &str, message: &str) -> Result<()> {
        self.with_entry(id, |e| {
            e.status = LocalScheduleStatus::Failed;
            e.error = Some(message.to_string());
            e.next_attempt_at = None;
        })?;
        Ok(())
    }

    pub fn finish_retry(&self, id: &str, attempts: u32, next_attempt_at: i64) -> Result<()> {
        self.with_entry(id, |e| {
            e.status = LocalScheduleStatus::Pending;
            e.attempts = attempts;
            e.next_attempt_at = Some(next_attempt_at);
        })?;
        Ok(())
    }

    /// `Pending` を `Expired` にする。`Pending` でなければ(または無ければ)false。
    pub fn mark_expired(&self, id: &str) -> Result<bool> {
        let changed = self.with_entry(id, |e| {
            if e.status == LocalScheduleStatus::Pending {
                e.status = LocalScheduleStatus::Expired;
                true
            } else {
                false
            }
        })?;
        Ok(changed.unwrap_or(false))
    }

    /// 取り消し(削除)。他アカウントの予約・存在しない予約は NotFound、`Posting` は Invalid。
    pub fn cancel(&self, account_id: &str, id: &str) -> Result<()> {
        let mut g = self.data.lock().unwrap();
        let Some(pos) = g.posts.iter().position(|e| e.id == id && e.account_id == account_id) else {
            return Err(Error::NotFound(format!("scheduled post: {id}")));
        };
        if g.posts[pos].status == LocalScheduleStatus::Posting {
            return Err(Error::Invalid("投稿処理中のため取り消せません".into()));
        }
        g.posts.remove(pos);
        self.save(&g)
    }

    pub fn remove_account(&self, account_id: &str) -> Result<()> {
        let mut g = self.data.lock().unwrap();
        g.posts.retain(|e| e.account_id != account_id);
        self.save(&g)
    }

    /// 起動時: `Posting` のまま残っている予約を、再送せず `Failed` にする。直した件数を返す。
    pub fn recover_posting(&self) -> Result<usize> {
        let mut g = self.data.lock().unwrap();
        let mut n = 0;
        for e in g.posts.iter_mut().filter(|e| e.status == LocalScheduleStatus::Posting) {
            e.status = LocalScheduleStatus::Failed;
            e.error = Some(UNKNOWN_RESULT_MESSAGE.to_string());
            n += 1;
        }
        if n > 0 {
            self.save(&g)?;
        }
        Ok(n)
    }
}

#[cfg(test)]
pub(crate) fn test_input(text: &str) -> DraftInput {
    use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
    DraftInput {
        text: text.to_string(),
        cw: None,
        visibility: VisibilityInput::Public,
        local_only: false,
        reaction_acceptance: ReactionAcceptanceInput::All,
        channel_id: None,
        poll: None,
        file_ids: vec![],
        reply_note: None,
        quote_note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ScheduledPostStore {
        ScheduledPostStore::new_in_memory()
    }

    #[test]
    fn add_creates_a_pending_entry_and_lists_per_account_in_time_order() {
        let s = store();
        let late = s.add("acc1", 2_000, test_input("late"), 100).unwrap();
        let early = s.add("acc1", 1_000, test_input("early"), 101).unwrap();
        s.add("acc2", 500, test_input("other"), 102).unwrap();

        assert_eq!(late.status, LocalScheduleStatus::Pending);
        assert_eq!((late.attempts, late.next_attempt_at, late.error.clone()), (0, None, None));
        let ids: Vec<_> = s.list_for_account("acc1").into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![early.id, late.id]);
        assert_eq!(s.list_for_account("acc2").len(), 1);
        assert!(s.list_for_account("nobody").is_empty());
    }

    #[test]
    fn due_waits_for_the_scheduled_time_and_for_a_retry_time() {
        let s = store();
        let e = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        assert!(s.due(999).is_empty());
        assert_eq!(s.due(1_000).len(), 1);

        s.begin_post(&e.id).unwrap().unwrap();
        s.finish_retry(&e.id, 1, 1_030).unwrap();
        assert!(s.due(1_029).is_empty(), "retry time not reached");
        assert_eq!(s.due(1_030).len(), 1);
        assert_eq!(s.get(&e.id).unwrap().attempts, 1);
    }

    #[test]
    fn due_only_returns_pending_entries_oldest_first() {
        let s = store();
        let a = s.add("acc1", 2_000, test_input("a"), 0).unwrap();
        let b = s.add("acc1", 1_000, test_input("b"), 0).unwrap();
        let c = s.add("acc1", 1_500, test_input("c"), 0).unwrap();
        s.begin_post(&c.id).unwrap(); // Posting は対象外
        let ids: Vec<_> = s.due(5_000).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![b.id, a.id]);
    }

    #[test]
    fn next_wakeup_is_the_earliest_pending_ready_time() {
        let s = store();
        assert_eq!(s.next_wakeup(), None);
        let a = s.add("acc1", 2_000, test_input("a"), 0).unwrap();
        s.add("acc1", 1_500, test_input("b"), 0).unwrap();
        assert_eq!(s.next_wakeup(), Some(1_500));
        s.begin_post(&a.id).unwrap();
        s.finish_failed(&a.id, "x").unwrap();
        assert_eq!(s.next_wakeup(), Some(1_500), "failed entries do not wake the scheduler");
    }

    /// Review Focus 2: 同じ予約を 2 回送らない。
    #[test]
    fn begin_post_succeeds_only_once() {
        let s = store();
        let e = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        let first = s.begin_post(&e.id).unwrap().expect("first begin_post wins");
        assert_eq!(first.status, LocalScheduleStatus::Posting);
        assert!(s.begin_post(&e.id).unwrap().is_none(), "already posting");
        assert_eq!(s.get(&e.id).unwrap().status, LocalScheduleStatus::Posting);
        assert!(s.begin_post("missing").unwrap().is_none());
    }

    #[test]
    fn begin_post_works_from_expired_and_failed() {
        let s = store();
        let e = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        assert!(s.mark_expired(&e.id).unwrap());
        assert!(s.begin_post(&e.id).unwrap().is_some());
        s.finish_failed(&e.id, "boom").unwrap();
        assert!(s.begin_post(&e.id).unwrap().is_some());
    }

    #[test]
    fn finish_posted_removes_and_finish_failed_keeps_the_reason() {
        let s = store();
        let a = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        let b = s.add("acc1", 1_000, test_input("b"), 0).unwrap();
        s.begin_post(&a.id).unwrap();
        s.finish_posted(&a.id).unwrap();
        assert!(s.get(&a.id).is_none());

        s.begin_post(&b.id).unwrap();
        s.finish_failed(&b.id, "理由").unwrap();
        let got = s.get(&b.id).unwrap();
        assert_eq!(got.status, LocalScheduleStatus::Failed);
        assert_eq!(got.error.as_deref(), Some("理由"));
        assert_eq!(got.next_attempt_at, None);
    }

    #[test]
    fn mark_expired_only_applies_to_pending() {
        let s = store();
        let e = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        assert!(s.mark_expired(&e.id).unwrap());
        assert_eq!(s.get(&e.id).unwrap().status, LocalScheduleStatus::Expired);
        assert!(!s.mark_expired(&e.id).unwrap(), "already expired");
        assert!(!s.mark_expired("missing").unwrap());
    }

    #[test]
    fn cancel_removes_but_not_posting_nor_other_accounts() {
        let s = store();
        let e = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        assert!(matches!(s.cancel("acc2", &e.id), Err(Error::NotFound(_))), "other account's entry");
        assert!(matches!(s.cancel("acc1", "missing"), Err(Error::NotFound(_))));

        s.begin_post(&e.id).unwrap();
        assert!(matches!(s.cancel("acc1", &e.id), Err(Error::Invalid(_))), "posting can't be cancelled");
        s.finish_failed(&e.id, "x").unwrap();
        s.cancel("acc1", &e.id).unwrap();
        assert!(s.get(&e.id).is_none());
    }

    #[test]
    fn remove_account_drops_only_that_accounts_entries() {
        let s = store();
        s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        s.add("acc2", 1_000, test_input("b"), 0).unwrap();
        s.remove_account("acc1").unwrap();
        assert!(s.list_for_account("acc1").is_empty());
        assert_eq!(s.list_for_account("acc2").len(), 1);
    }

    /// Review Focus 2: 起動時の Posting は再送せず Failed にする。
    #[test]
    fn recover_posting_marks_unknown_results_failed_without_resending() {
        let s = store();
        let a = s.add("acc1", 1_000, test_input("a"), 0).unwrap();
        let b = s.add("acc1", 1_000, test_input("b"), 0).unwrap();
        s.begin_post(&a.id).unwrap();
        assert_eq!(s.recover_posting().unwrap(), 1);
        let got = s.get(&a.id).unwrap();
        assert_eq!(got.status, LocalScheduleStatus::Failed);
        assert_eq!(got.error.as_deref(), Some(UNKNOWN_RESULT_MESSAGE));
        assert_eq!(s.get(&b.id).unwrap().status, LocalScheduleStatus::Pending);
        assert_eq!(s.recover_posting().unwrap(), 0);
    }

    #[test]
    fn to_local_note_carries_the_draft_content() {
        let s = store();
        let mut input = test_input("本文");
        input.cw = Some("注意".into());
        input.file_ids = vec!["f1".into()];
        let e = s.add("acc1", 1_234, input, 0).unwrap();
        s.begin_post(&e.id).unwrap();
        s.finish_failed(&e.id, "理由").unwrap();
        let n = s.get(&e.id).unwrap().to_local_note();
        assert_eq!(n.note.id, e.id);
        assert_eq!(n.note.scheduled_at, 1_234);
        assert_eq!(n.note.text, "本文");
        assert_eq!(n.note.cw.as_deref(), Some("注意"));
        assert_eq!(n.note.file_ids, vec!["f1"]);
        assert_eq!(n.status, LocalScheduleStatus::Failed);
        assert_eq!(n.error.as_deref(), Some("理由"));
    }

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("tsumugi-scheduled-{}.json", uuid::Uuid::new_v4()))
    }

    /// E2E がこのファイルを直接書き換えるため、キー名(camelCase)と状態の値を固定する。
    #[test]
    fn file_round_trip_persists_and_uses_the_keys_the_e2e_edits() {
        let path = temp_path();
        let id = {
            let s = ScheduledPostStore::new(path.clone()).unwrap();
            let e = s.add("acc1", 1_234, test_input("hello"), 5).unwrap();
            s.begin_post(&e.id).unwrap();
            e.id
        };
        let raw = std::fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let post = &json["posts"][0];
        assert_eq!(post["scheduledAt"], 1_234);
        assert_eq!(post["status"], "posting");
        assert_eq!(post["input"]["text"], "hello");

        let s2 = ScheduledPostStore::new(path.clone()).unwrap();
        let got = s2.get(&id).unwrap();
        assert_eq!(got.status, LocalScheduleStatus::Posting);
        assert_eq!(got.input.text, "hello");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_file_is_empty_and_a_corrupt_file_is_an_error() {
        let path = temp_path();
        assert!(ScheduledPostStore::new(path.clone()).unwrap().list_for_account("acc1").is_empty());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(ScheduledPostStore::new(path.clone()).is_err());
        std::fs::remove_file(&path).unwrap();
    }
}
