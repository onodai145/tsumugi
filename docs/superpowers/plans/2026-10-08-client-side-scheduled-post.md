# 予約投稿(クライアント側フォールバック) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 予約投稿に対応していないサーバーのアカウントでも、tsumugi が予約をローカルに保持し、予約時刻に自分で投稿できるようにする(Issue #60 のサブプロジェクト B)。確認は E2E で行う。

**Architecture:** Rust 側に `ScheduledPostStore`(JSON 永続化)と常駐スケジューラ(`scheduler.rs`)を追加する。スケジューラは次の期限まで待ち(最大 30 秒)、期限が来た予約を既存の `create_note` で投稿して、結果をイベントでフロントへ通知する。フロントは、`get_schedule_capabilities` の結果で「サーバー予約(A)かローカル予約(B)か」を振り分け、予約一覧モーダルで両方を統合して見せる。E2E は、デバッグビルド限定の環境変数でサーバーバージョンを上書きして、ローカル予約の経路を通す。

**Tech Stack:** Rust(tauri v2、tauri-specta、tokio、reqwest、wiremock)、Svelte 5(runes)、Vitest、WebdriverIO(E2E、tauri-driver)。

**Spec:** `docs/superpowers/specs/2026-10-08-client-side-scheduled-post-design.md`

## Global Constraints

- 時刻は Rust の command 引数・ストア・`ScheduledNote.scheduled_at` とも **epoch 秒**(A と同じ)。ミリ秒は投票の締切(`PollDraftSnapshot.expires_at` / `PollInput.expires_at`)だけ。
- ローカル予約の状態は `Pending` / `Posting` / `Expired` / `Failed`。**期限切れは自動では投稿しない**(猶予 `MISSED_GRACE_SEC = 300` 秒以内なら投稿する)。
- 失敗の分類: 再試行は「確実に届いていない」場合だけ(`Network` の `connect:` で始まるメッセージ、`RateLimited`)。**`timeout:` とその他の `Network` は再試行しない**(投稿が成功している可能性があり、再送すると重複投稿になる)。再試行は 30・60・120 秒の最大 3 回。
- 起動時に `Posting` のまま残っていた予約は、**再送せず** `Failed` にする(`UNKNOWN_RESULT_MESSAGE`)。
- A の型・command は変えない(追加のみ)。例外: ComposeBar の「非対応アカウントに切り替えたら予約日時を消す」処理とそのテスト、`ScheduledModal` とそのテストは本計画で書き換える。
- 既存の `AppState::new*` のシグネチャは変えない(既存テストに波及させない)。
- デバッグ限定の環境変数 `TSUMUGI_DEBUG_SERVER_VERSION` は `#[cfg(debug_assertions)]` のときだけ読む。リリースビルドにこの分岐を含めない。
- 新規 UI の角丸・文字サイズ・アイコンサイズは `docs/design/style-guide.md` と既存クラスに合わせる。日時入力は `DateTimeInput`(ネイティブ入力を使わない)。
- コミットメッセージは件名のみ(本文なし)。末尾の Co-Authored-By トレーラは別途付与される。`--no-verify` / `--no-gpg-sign` は使わない。コミットが失敗またはタイムアウトしたら、リトライせず停止して報告する。
- ログに `account_id` を出さない(CodeQL の誤検知を避けるため)。予約の ID(uuid)は出してよい。
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。`cd src-tauri && cargo test generates_frontend_bindings` で再生成し、差分をコミットする。
- フロントのテストは `cd frontend && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視する)。`pnpm check` はエラー 0・既存の警告 1(UrlPreviewCard)のまま保つ。
- E2E は `e2e/README.md` の手順で動かす(Docker、Xvfb、`cargo build` 済みのデバッグバイナリ、`pnpm build` 済みのフロント)。起動したプロセスは PID 指定で止める(`pkill`/`killall` 禁止)。`docker compose down -v` で後始末する。

## Review Focus

1. タイムアウトとその他の `Network` エラーでは**再試行せず `Failed`** にする(重複投稿を防ぐ)。接頭辞(`connect:` / `timeout:`)への依存をテストで固定する(Task 2)。
2. 同じ予約を 2 回送らない。`begin_post` は 1 回しか成功せず、`Posting` の予約は取り消しも「今すぐ投稿」もできない。起動時の `Posting` は再送せず `Failed`(Task 1・2、E2E の Task 9)。
3. 期限切れ(猶予超過)は、起動直後でも**投稿しない**(Task 2、E2E の Task 9)。
4. 経路が決まる前(機能判定の取得中)に予約日時つきで送信しても、**即時投稿にならず**、予約にもならない(Task 6)。
5. ローカル予約でも、期間指定の投票は予約日時を基準に締切を計算し、チャンネル投稿は `local_only` になる(Task 2・6)。

---

### Task 1: ドメイン型・ストア・`AppState`(Rust)

**Files:**
- Modify: `src-tauri/src/domain/scheduled.rs`(`LocalScheduleStatus`、`LocalScheduledNote`)
- Modify: `src-tauri/src/domain/mod.rs`(re-export)
- Modify: `src-tauri/src/domain/schedule.rs`(`ScheduleCapabilities.available` の説明コメントだけ)
- Create: `src-tauri/src/store/scheduled_post.rs`
- Modify: `src-tauri/src/store/mod.rs`
- Modify: `src-tauri/src/state.rs`

**Interfaces:**
- Produces:
  - `crate::domain::{LocalScheduleStatus, LocalScheduledNote}`
  - `crate::store::scheduled_post::{ScheduledPostStore, ScheduledPostEntry, UNKNOWN_RESULT_MESSAGE}` と、`ScheduledPostStore` の API: `new(path) -> Result<Self>`、`new_in_memory() -> Self`、`add(&self, account_id: &str, scheduled_at: i64, input: DraftInput, now: i64) -> Result<ScheduledPostEntry>`、`get(&self, id: &str) -> Option<ScheduledPostEntry>`、`list_for_account(&self, account_id: &str) -> Vec<ScheduledPostEntry>`、`due(&self, now: i64) -> Vec<ScheduledPostEntry>`、`next_wakeup(&self) -> Option<i64>`、`begin_post(&self, id: &str) -> Result<Option<ScheduledPostEntry>>`、`finish_posted(&self, id: &str) -> Result<()>`、`finish_failed(&self, id: &str, message: &str) -> Result<()>`、`finish_retry(&self, id: &str, attempts: u32, next_attempt_at: i64) -> Result<()>`、`mark_expired(&self, id: &str) -> Result<bool>`、`cancel(&self, account_id: &str, id: &str) -> Result<()>`、`remove_account(&self, account_id: &str) -> Result<()>`、`recover_posting(&self) -> Result<usize>`
  - `ScheduledPostEntry::to_local_note(&self) -> LocalScheduledNote`、`ScheduledPostEntry::ready_at(&self) -> i64`
  - テスト用 `#[cfg(test)] pub(crate) fn test_input(text: &str) -> DraftInput`
  - `AppState.scheduled_posts: ScheduledPostStore`、`AppState.scheduler_wakeup: std::sync::Arc<tokio::sync::Notify>`、`AppState::with_scheduled_posts(self, store) -> Self`

- [ ] **Step 1: ドメイン型を足す**

`src-tauri/src/domain/scheduled.rs` の `use serde::Serialize;` を `use serde::{Deserialize, Serialize};` に変え、末尾に次を追加する。

```rust
/// クライアント側(ローカル)予約の状態(Issue #60 B)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LocalScheduleStatus {
    /// 予約時刻(または再試行の予定時刻)を待っている。
    Pending,
    /// 送信中。結果が分かるまで、取り消しも再送もできない。
    Posting,
    /// アプリが起動していない間に予約時刻を過ぎた(猶予超過)。自動では投稿しない。
    Expired,
    /// 投稿に失敗した。理由は `error`。
    Failed,
}

/// ローカル予約 1 件。中身は A の `ScheduledNote` を再利用する(`id` はローカル予約の ID)。
/// これにより、A の「作成欄に戻す」の処理をそのまま使える。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalScheduledNote {
    pub note: ScheduledNote,
    pub status: LocalScheduleStatus,
    pub error: Option<String>,
}
```

`src-tauri/src/domain/mod.rs` の `pub use scheduled::ScheduledNote;` を次にする。

```rust
pub use scheduled::{LocalScheduleStatus, LocalScheduledNote, ScheduledNote};
```

`src-tauri/src/domain/schedule.rs` の `ScheduleCapabilities.available` の doc コメントを次に直す(挙動は変えない)。

```rust
    /// **サーバー側予約**(`notes/drafts` の `scheduledAt`)が使えるか。false のときは、
    /// クライアント側予約(tsumugi が保持して投稿する、Issue #60 B)を使う。
    pub available: bool,
```

- [ ] **Step 2: 失敗するストアのテストを書く(実装は未着手)**

`src-tauri/src/store/mod.rs` に `pub mod scheduled_post;`(`pub mod draft;` の次)と `pub use scheduled_post::ScheduledPostStore;`(`pub use draft::DraftStore;` の次)を追加する。

`src-tauri/src/store/scheduled_post.rs` を作成し、まず**型と関数の宣言だけ**(本体は `unimplemented!()`)とテストを書く。

```rust
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

impl ScheduledPostEntry {
    /// 次に処理してよい時刻(epoch 秒)。
    pub fn ready_at(&self) -> i64 {
        let _ = self;
        unimplemented!()
    }

    pub fn to_local_note(&self) -> LocalScheduledNote {
        let _ = self;
        unimplemented!()
    }
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

impl ScheduledPostStore {
    pub fn new(_path: PathBuf) -> Result<Self> {
        unimplemented!()
    }
    pub(crate) fn new_in_memory() -> Self {
        unimplemented!()
    }
    pub fn add(&self, _account_id: &str, _scheduled_at: i64, _input: DraftInput, _now: i64) -> Result<ScheduledPostEntry> {
        unimplemented!()
    }
    pub fn get(&self, _id: &str) -> Option<ScheduledPostEntry> {
        unimplemented!()
    }
    pub fn list_for_account(&self, _account_id: &str) -> Vec<ScheduledPostEntry> {
        unimplemented!()
    }
    pub fn due(&self, _now: i64) -> Vec<ScheduledPostEntry> {
        unimplemented!()
    }
    pub fn next_wakeup(&self) -> Option<i64> {
        unimplemented!()
    }
    pub fn begin_post(&self, _id: &str) -> Result<Option<ScheduledPostEntry>> {
        unimplemented!()
    }
    pub fn finish_posted(&self, _id: &str) -> Result<()> {
        unimplemented!()
    }
    pub fn finish_failed(&self, _id: &str, _message: &str) -> Result<()> {
        unimplemented!()
    }
    pub fn finish_retry(&self, _id: &str, _attempts: u32, _next_attempt_at: i64) -> Result<()> {
        unimplemented!()
    }
    pub fn mark_expired(&self, _id: &str) -> Result<bool> {
        unimplemented!()
    }
    pub fn cancel(&self, _account_id: &str, _id: &str) -> Result<()> {
        unimplemented!()
    }
    pub fn remove_account(&self, _account_id: &str) -> Result<()> {
        unimplemented!()
    }
    pub fn recover_posting(&self) -> Result<usize> {
        unimplemented!()
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
```

Run: `cd src-tauri && cargo test --lib store::scheduled_post`
Expected: FAIL(`not implemented` の panic が各テストで出る。コンパイルは通ること)

- [ ] **Step 3: ストアを実装する**

Step 2 のスタブを、次の実装に置き換える(`use`・型定義・テストはそのまま)。

```rust
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
```

Run: `cd src-tauri && cargo test --lib store::scheduled_post`
Expected: PASS(13 tests)

- [ ] **Step 4: `AppState` にストアと通知を足す**

`src-tauri/src/state.rs`:

(a) `AppState` の `pub drafts: DraftStore,` の次に追加する。

```rust
    /// クライアント側の予約投稿(Issue #60 B)。`AppState::new*` のシグネチャを変えずに済むよう、
    /// 既定はメモリ版で、起動時に `with_scheduled_posts` で永続版へ差し替える。
    pub scheduled_posts: crate::store::ScheduledPostStore,
    /// スケジューラを起こす通知。予約の追加・取り消し・「今すぐ投稿」の後に `notify_one` する。
    pub scheduler_wakeup: std::sync::Arc<tokio::sync::Notify>,
```

(b) `new_with_sound` の `Self { ... }` で、`drafts,` の次に追加する。

```rust
            scheduled_posts: crate::store::ScheduledPostStore::new_in_memory(),
            scheduler_wakeup: std::sync::Arc::new(tokio::sync::Notify::new()),
```

(c) `impl AppState` の `pub fn new(...)` の直後に追加する。

```rust
    /// 予約投稿のストアを差し替える(起動時に永続版へ)。
    pub fn with_scheduled_posts(mut self, store: crate::store::ScheduledPostStore) -> Self {
        self.scheduled_posts = store;
        self
    }
```

Run: `cd src-tauri && cargo build 2>&1 | tail -5 && cargo test --lib 2>&1 | grep -E "^test result|FAILED"`
Expected: ビルド成功。テストは全体で PASS(未使用の警告は、Task 2 で使うので許容する。`dead_code` の警告が出ても、新しい警告がこの 2 つのフィールド・メソッド・ストア関連だけであることを報告する)。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/domain src-tauri/src/store src-tauri/src/state.rs
git commit -m "feat: クライアント側予約の型とストアを追加(#60 B)"
```

---

### Task 2: スケジューラの中核ロジック(Rust、`scheduler.rs`)

**Files:**
- Create: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/lib.rs`(`mod scheduler;` の追加だけ)

**Interfaces:**
- Consumes: Task 1 の `ScheduledPostStore` の API、`AppState::{scheduled_posts, scheduler_wakeup, client_for, host_token}`、`api::notes::{create_note, NoteDraft, PollInput}`、`store::draft::DraftInput`
- Produces(Task 3 が使う):
  - 定数 `MISSED_GRACE_SEC: i64 = 300`、`RETRY_DELAYS_SEC: [i64; 3] = [30, 60, 120]`、`MAX_SLEEP_SEC: i64 = 30`、`EXPIRED_MESSAGE: &str`
  - `pub fn now_sec() -> i64`
  - `pub fn to_note_draft(input: &DraftInput) -> NoteDraft`
  - `pub enum Failure { Retry, Fatal(String) }`、`pub fn classify(e: &Error) -> Failure`
  - `pub fn next_sleep(now: i64, next_wakeup: Option<i64>) -> u64`(秒。1〜`MAX_SLEEP_SEC`)
  - `pub enum Outcome { Posted { account_id, id, note_id }, Failed { account_id, id, message }, Expired { account_id, id } }`
  - `pub async fn process_due(state: &AppState, now: i64) -> Vec<Outcome>`
  - `pub async fn post_now(state: &AppState, account_id: &str, id: &str, now: i64) -> Result<Outcome>`
  - `pub fn schedule_local(state: &AppState, account_id: &str, input: DraftInput, scheduled_at: i64, now: i64) -> Result<LocalScheduledNote>`、`pub fn list_local(state: &AppState, account_id: &str) -> Vec<LocalScheduledNote>`、`pub fn cancel_local(state: &AppState, account_id: &str, id: &str) -> Result<()>`

- [ ] **Step 1: 失敗するテストを書く(本体はスタブ)**

`src-tauri/src/lib.rs` の `mod session;` の前(アルファベット順で `mod mobile_intent;` の次)に `mod scheduler;` を追加する。

`src-tauri/src/scheduler.rs` を作成する。まず、型・定数・関数の宣言(本体は `unimplemented!()`)とテストを書く。

```rust
//! クライアント側の予約投稿(Issue #60 B)のスケジューラ。
//!
//! 予約はストア(`store::scheduled_post`)が持ち、ここは「期限が来たら既存の `create_note` で投稿する」
//! 処理だけを担う。1 回の処理(`process_due`)をループから切り離してあり、時刻(`now`)を引数で渡せる。
//!
//! tsumugi が起動している間しか実行できない。起動していない間に予約時刻を過ぎた予約は、猶予
//! (`MISSED_GRACE_SEC`)を超えていれば**投稿せず** `Expired` にする。

use crate::api::notes::{create_note, NoteDraft, PollInput};
use crate::domain::{LocalScheduleStatus, LocalScheduledNote};
use crate::error::{Error, Result};
use crate::state::AppState;
use crate::store::draft::DraftInput;

/// 予約時刻からこの秒数以内なら投稿する。超えていたら期限切れ(自動では投稿しない)。
pub const MISSED_GRACE_SEC: i64 = 300;
/// 再試行の間隔(秒)。この回数だけ再試行する。
pub const RETRY_DELAYS_SEC: [i64; 3] = [30, 60, 120];
/// スリープ復帰や時計の変更に備えて、最大でこの秒数ごとに必ず起き直す。
pub const MAX_SLEEP_SEC: i64 = 30;
pub const EXPIRED_MESSAGE: &str = "予約時刻を過ぎたため投稿しませんでした(期限切れ)";

pub fn now_sec() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 保存した下書きの形(`DraftInput`)から、投稿のリクエスト(`NoteDraft`)を作る。
pub fn to_note_draft(_input: &DraftInput) -> NoteDraft {
    unimplemented!()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 確実に届いていない。再試行してよい。
    Retry,
    /// 再試行しない。理由をユーザーに見せる。
    Fatal(String),
}

pub fn classify(_e: &Error) -> Failure {
    unimplemented!()
}

/// 次に寝る秒数。次の期限までを `1..=MAX_SLEEP_SEC` に丸める(0 だと busy loop になりうるため最低 1 秒)。
pub fn next_sleep(_now: i64, _next_wakeup: Option<i64>) -> u64 {
    unimplemented!()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Posted { account_id: String, id: String, note_id: String },
    Failed { account_id: String, id: String, message: String },
    Expired { account_id: String, id: String },
}

pub async fn process_due(_state: &AppState, _now: i64) -> Vec<Outcome> {
    unimplemented!()
}

pub async fn post_now(_state: &AppState, _account_id: &str, _id: &str, _now: i64) -> Result<Outcome> {
    unimplemented!()
}

pub fn schedule_local(
    _state: &AppState,
    _account_id: &str,
    _input: DraftInput,
    _scheduled_at: i64,
    _now: i64,
) -> Result<LocalScheduledNote> {
    unimplemented!()
}

pub fn list_local(_state: &AppState, _account_id: &str) -> Vec<LocalScheduledNote> {
    unimplemented!()
}

pub fn cancel_local(_state: &AppState, _account_id: &str, _id: &str) -> Result<()> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
    use crate::store::draft::{DraftNoteSnapshot, PollDraftSnapshot};
    use crate::store::scheduled_post::test_input;
    use crate::store::SettingsStore;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const NOW: i64 = 1_800_000_000;

    fn state() -> AppState {
        let s = AppState::new_for_test(SettingsStore::new_in_memory());
        s.register_test_account("acc1");
        s
    }

    fn created_note() -> serde_json::Value {
        json!({ "createdNote": {
            "id": "n1",
            "createdAt": "2026-10-08T00:00:00.000Z",
            "user": { "id": "u1", "username": "me" }
        } })
    }

    async fn mock_create(status: u16, body: serde_json::Value) -> MockServer {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/create"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&mock)
            .await;
        mock
    }

    // ---- to_note_draft ----

    #[test]
    fn to_note_draft_trims_and_maps_reply_quote_and_poll() {
        let mut i = test_input("  こんにちは  ");
        i.cw = Some("  注意  ".into());
        i.visibility = VisibilityInput::Home;
        i.reaction_acceptance = ReactionAcceptanceInput::LikeOnly;
        i.file_ids = vec!["f1".into()];
        i.reply_note = Some(DraftNoteSnapshot { id: "r1".into(), username: "alice".into(), text: Some("x".into()) });
        i.quote_note = Some(DraftNoteSnapshot { id: "q1".into(), username: "bob".into(), text: None });
        i.poll = Some(PollDraftSnapshot { choices: vec!["a".into(), "b".into()], multiple: true, expires_at: Some(123_000) });
        let d = to_note_draft(&i);
        assert_eq!(d.text.as_deref(), Some("こんにちは"));
        assert_eq!(d.cw.as_deref(), Some("注意"));
        assert_eq!(d.visibility, VisibilityInput::Home);
        assert_eq!(d.file_ids, vec!["f1"]);
        assert_eq!(d.reply_id.as_deref(), Some("r1"));
        assert_eq!(d.renote_id.as_deref(), Some("q1"));
        let poll = d.poll.unwrap();
        assert_eq!((poll.choices, poll.multiple, poll.expires_at), (vec!["a".to_string(), "b".to_string()], true, Some(123_000)));
        assert_eq!(d.reaction_acceptance, Some(ReactionAcceptanceInput::LikeOnly));
    }

    #[test]
    fn to_note_draft_drops_blank_text_and_cw() {
        let mut i = test_input("   ");
        i.cw = Some("  ".into());
        let d = to_note_draft(&i);
        assert_eq!((d.text, d.cw), (None, None));
    }

    /// Review Focus 5: チャンネル投稿は連合なし(ComposeBar の通常投稿と同じ)。
    #[test]
    fn to_note_draft_forces_local_only_for_channel_posts() {
        let mut i = test_input("x");
        i.channel_id = Some("ch1".into());
        assert!(to_note_draft(&i).local_only);
        i.channel_id = None;
        assert!(!to_note_draft(&i).local_only);
        i.local_only = true;
        assert!(to_note_draft(&i).local_only);
    }

    // ---- classify ----

    /// Review Focus 1: 再試行は「確実に届いていない」場合だけ。接頭辞への依存もここで固定する。
    #[test]
    fn classify_retries_only_when_the_request_surely_did_not_arrive() {
        assert_eq!(classify(&Error::Network("connect: refused".into())), Failure::Retry);
        assert_eq!(classify(&Error::RateLimited), Failure::Retry);
        for e in [
            Error::Network("timeout: deadline".into()),
            Error::Network("error sending request".into()),
            Error::Unauthorized("401".into()),
            Error::Forbidden("403".into()),
            Error::NotFound("404".into()),
            Error::Api("notes/create: SOME_CODE x".into()),
            Error::Invalid("unknown account".into()),
        ] {
            assert!(matches!(classify(&e), Failure::Fatal(_)), "{e:?} must not be retried");
        }
        let Failure::Fatal(msg) = classify(&Error::Network("timeout: x".into())) else { panic!() };
        assert!(msg.contains("投稿されたか確認"), "{msg}");
    }

    /// `error.rs` の `From<reqwest::Error>` が付ける接頭辞に依存している。ここが変わると再試行の判定が変わる。
    #[tokio::test]
    async fn a_refused_connection_is_classified_as_retry() {
        let err = reqwest::Client::new().post("http://127.0.0.1:1/").send().await.unwrap_err();
        assert_eq!(classify(&Error::from(err)), Failure::Retry);
    }

    // ---- next_sleep ----

    #[test]
    fn next_sleep_is_clamped_between_one_and_the_cap() {
        assert_eq!(next_sleep(1_000, None), MAX_SLEEP_SEC as u64);
        assert_eq!(next_sleep(1_000, Some(1_010)), 10);
        assert_eq!(next_sleep(1_000, Some(9_999)), MAX_SLEEP_SEC as u64);
        assert_eq!(next_sleep(1_000, Some(1_000)), 1);
        assert_eq!(next_sleep(1_000, Some(500)), 1, "overdue entries must not spin");
    }

    // ---- process_due ----

    #[tokio::test]
    async fn due_entry_is_posted_once_and_removed() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/create"))
            .and(body_partial_json(json!({ "text": "hello", "visibility": "public" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(created_note()))
            .expect(1)
            .mount(&mock)
            .await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW - 10, test_input("hello"), NOW - 100).unwrap();

        let out = process_due(&s, NOW).await;
        assert_eq!(out, vec![Outcome::Posted { account_id: "acc1".into(), id: e.id.clone(), note_id: "n1".into() }]);
        assert!(s.scheduled_posts.get(&e.id).is_none());
        // 2 回目は何も送らない(`expect(1)` は MockServer の drop で検証される)
        assert!(process_due(&s, NOW + 1).await.is_empty());
    }

    #[tokio::test]
    async fn entries_not_yet_due_are_left_alone() {
        let mock = mock_create(200, created_note()).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW + 60, test_input("later"), NOW).unwrap();
        assert!(process_due(&s, NOW).await.is_empty());
        assert_eq!(s.scheduled_posts.get(&e.id).unwrap().status, LocalScheduleStatus::Pending);
        assert!(mock.received_requests().await.unwrap().is_empty());
    }

    /// Review Focus 3: 猶予(5 分)を超えた予約は、投稿せず Expired にする。ちょうど猶予の境界は投稿する。
    #[tokio::test]
    async fn entries_older_than_the_grace_are_expired_not_posted() {
        let mock = mock_create(200, created_note()).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let old = s.scheduled_posts.add("acc1", NOW - MISSED_GRACE_SEC - 1, test_input("old"), 0).unwrap();
        let edge = s.scheduled_posts.add("acc1", NOW - MISSED_GRACE_SEC, test_input("edge"), 0).unwrap();

        let out = process_due(&s, NOW).await;
        assert!(out.contains(&Outcome::Expired { account_id: "acc1".into(), id: old.id.clone() }));
        assert!(out.iter().any(|o| matches!(o, Outcome::Posted { id, .. } if *id == edge.id)));
        assert_eq!(s.scheduled_posts.get(&old.id).unwrap().status, LocalScheduleStatus::Expired);
        assert_eq!(mock.received_requests().await.unwrap().len(), 1, "only the edge entry is posted");
        // 期限切れは、その後何度処理しても投稿されない
        assert!(process_due(&s, NOW + 3_600).await.is_empty());
        assert_eq!(mock.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn connection_failures_are_retried_with_backoff_then_fail() {
        let s = state();
        s.set_test_api_base("http://127.0.0.1:1".into()); // 接続拒否
        let e = s.scheduled_posts.add("acc1", NOW - 1, test_input("x"), 0).unwrap();

        // 1〜3 回目の失敗: 再試行を予約する(outcome なし)
        let mut t = NOW;
        for (i, delay) in RETRY_DELAYS_SEC.iter().enumerate() {
            assert!(process_due(&s, t).await.is_empty(), "retry {i} produces no outcome");
            let got = s.scheduled_posts.get(&e.id).unwrap();
            assert_eq!(got.status, LocalScheduleStatus::Pending);
            assert_eq!(got.attempts, i as u32 + 1);
            assert_eq!(got.next_attempt_at, Some(t + delay));
            assert!(process_due(&s, t + delay - 1).await.is_empty(), "not before the retry time");
            t += delay;
        }
        // 4 回目も失敗: 再試行を使い切って Failed
        let out = process_due(&s, t).await;
        let [Outcome::Failed { message, .. }] = out.as_slice() else { panic!("{out:?}") };
        assert!(message.contains("再試行"), "{message}");
        assert_eq!(s.scheduled_posts.get(&e.id).unwrap().status, LocalScheduleStatus::Failed);
    }

    #[tokio::test]
    async fn a_rate_limited_post_is_retried_and_then_succeeds() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/create"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/notes/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(created_note()))
            .mount(&mock)
            .await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW - 1, test_input("x"), 0).unwrap();

        assert!(process_due(&s, NOW).await.is_empty());
        assert_eq!(s.scheduled_posts.get(&e.id).unwrap().next_attempt_at, Some(NOW + 30));
        let out = process_due(&s, NOW + 30).await;
        assert!(matches!(out.as_slice(), [Outcome::Posted { .. }]));
    }

    #[tokio::test]
    async fn an_auth_error_fails_without_retrying() {
        let mock = mock_create(401, json!({ "error": { "code": "AUTHENTICATION_FAILED", "message": "x", "id": "i" } })).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW - 1, test_input("x"), 0).unwrap();

        let out = process_due(&s, NOW).await;
        let [Outcome::Failed { message, .. }] = out.as_slice() else { panic!("{out:?}") };
        assert!(message.contains("unauthorized"), "{message}");
        let got = s.scheduled_posts.get(&e.id).unwrap();
        assert_eq!((got.status, got.attempts), (LocalScheduleStatus::Failed, 0));
        assert_eq!(mock.received_requests().await.unwrap().len(), 1, "no retry");
    }

    /// Review Focus 2: 送信中の予約は、スケジューラが拾い直さない。
    #[tokio::test]
    async fn a_posting_entry_is_never_picked_up_again() {
        let mock = mock_create(200, created_note()).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW - 1, test_input("x"), 0).unwrap();
        s.scheduled_posts.begin_post(&e.id).unwrap();
        assert!(process_due(&s, NOW).await.is_empty());
        assert!(mock.received_requests().await.unwrap().is_empty());
    }

    // ---- post_now ----

    #[tokio::test]
    async fn post_now_sends_an_expired_entry_once_and_removes_it() {
        let mock = mock_create(200, created_note()).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let e = s.scheduled_posts.add("acc1", NOW - 10_000, test_input("late"), 0).unwrap();
        s.scheduled_posts.mark_expired(&e.id).unwrap();

        let out = post_now(&s, "acc1", &e.id, NOW).await.unwrap();
        assert!(matches!(out, Outcome::Posted { .. }));
        assert!(s.scheduled_posts.get(&e.id).is_none());
        assert_eq!(mock.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn post_now_failure_marks_failed_without_retrying() {
        let s = state();
        s.set_test_api_base("http://127.0.0.1:1".into());
        let e = s.scheduled_posts.add("acc1", NOW - 10_000, test_input("late"), 0).unwrap();
        s.scheduled_posts.mark_expired(&e.id).unwrap();

        let out = post_now(&s, "acc1", &e.id, NOW).await.unwrap();
        assert!(matches!(out, Outcome::Failed { .. }));
        let got = s.scheduled_posts.get(&e.id).unwrap();
        assert_eq!((got.status, got.attempts), (LocalScheduleStatus::Failed, 0));
    }

    #[tokio::test]
    async fn post_now_rejects_pending_posting_other_accounts_and_missing() {
        let s = state();
        let pending = s.scheduled_posts.add("acc1", NOW + 60, test_input("a"), 0).unwrap();
        let posting = s.scheduled_posts.add("acc1", NOW - 10, test_input("b"), 0).unwrap();
        s.scheduled_posts.begin_post(&posting.id).unwrap();
        assert!(matches!(post_now(&s, "acc1", &pending.id, NOW).await, Err(Error::Invalid(_))));
        assert!(matches!(post_now(&s, "acc1", &posting.id, NOW).await, Err(Error::Invalid(_))));
        assert!(matches!(post_now(&s, "acc2", &pending.id, NOW).await, Err(Error::NotFound(_))));
        assert!(matches!(post_now(&s, "acc1", "missing", NOW).await, Err(Error::NotFound(_))));
    }

    // ---- schedule_local / list_local / cancel_local ----

    #[tokio::test]
    async fn schedule_local_stores_the_entry_and_wakes_the_scheduler() {
        let s = state();
        let n = schedule_local(&s, "acc1", test_input("hi"), NOW + 60, NOW).unwrap();
        assert_eq!((n.note.scheduled_at, n.status), (NOW + 60, LocalScheduleStatus::Pending));
        assert_eq!(list_local(&s, "acc1").len(), 1);
        // notify_one の許可証が残っているので、待たずに起きる
        tokio::time::timeout(std::time::Duration::from_millis(50), s.scheduler_wakeup.notified())
            .await
            .expect("scheduler should be woken");
    }

    #[test]
    fn schedule_local_rejects_past_times_and_unknown_accounts() {
        let s = state();
        assert!(matches!(schedule_local(&s, "acc1", test_input("x"), NOW, NOW), Err(Error::Invalid(_))));
        assert!(matches!(schedule_local(&s, "acc1", test_input("x"), NOW - 1, NOW), Err(Error::Invalid(_))));
        assert!(matches!(schedule_local(&s, "nobody", test_input("x"), NOW + 60, NOW), Err(Error::Invalid(_))));
        assert!(list_local(&s, "acc1").is_empty());
    }

    #[test]
    fn cancel_local_removes_the_entry() {
        let s = state();
        let n = schedule_local(&s, "acc1", test_input("x"), NOW + 60, NOW).unwrap();
        cancel_local(&s, "acc1", &n.note.id).unwrap();
        assert!(list_local(&s, "acc1").is_empty());
    }
}
```

Run: `cd src-tauri && cargo test --lib scheduler`
Expected: FAIL(`not implemented` の panic。コンパイルは通ること。`wiremock` の `received_requests` は `MockServer::start()` が既定で記録を有効にしていることを前提にしている。コンパイルエラーなら wiremock 0.6 の API に合わせて直す)

- [ ] **Step 2: 実装する**

スタブの 8 関数を次に置き換える(`use`・型・定数・テストはそのまま)。

```rust
/// 保存した下書きの形(`DraftInput`)から、投稿のリクエスト(`NoteDraft`)を作る。
/// ComposeBar の通常投稿(`submit()`)と同じ変換: 本文・CW は trim して空なら None、チャンネル投稿は連合なし。
pub fn to_note_draft(input: &DraftInput) -> NoteDraft {
    let non_blank = |s: &str| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_string())
    };
    NoteDraft {
        text: non_blank(&input.text),
        cw: input.cw.as_deref().and_then(non_blank),
        visibility: input.visibility,
        file_ids: input.file_ids.clone(),
        poll: input.poll.as_ref().map(|p| PollInput {
            choices: p.choices.clone(),
            multiple: p.multiple,
            expires_at: p.expires_at,
        }),
        reply_id: input.reply_note.as_ref().map(|n| n.id.clone()),
        renote_id: input.quote_note.as_ref().map(|n| n.id.clone()),
        channel_id: input.channel_id.clone(),
        local_only: input.local_only || input.channel_id.is_some(),
        reaction_acceptance: Some(input.reaction_acceptance),
    }
}

/// 失敗の分類。投稿が既に成功している可能性があるもの(タイムアウトなど)は再試行しない
/// (再送すると重複投稿になる)。接続失敗と 429 だけが「確実に届いていない」。
/// 接続失敗とタイムアウトの区別は、`error.rs` の `From<reqwest::Error>` が付ける接頭辞に依存する。
pub fn classify(e: &Error) -> Failure {
    match e {
        Error::Network(m) if m.starts_with("connect:") => Failure::Retry,
        Error::RateLimited => Failure::Retry,
        Error::Network(m) if m.starts_with("timeout:") => {
            Failure::Fatal("通信がタイムアウトしました。投稿されたか確認してください".into())
        }
        Error::Network(_) => {
            Failure::Fatal("通信エラーのため投稿結果が不明です。投稿されたか確認してください".into())
        }
        other => Failure::Fatal(other.to_string()),
    }
}

pub fn next_sleep(now: i64, next_wakeup: Option<i64>) -> u64 {
    match next_wakeup {
        Some(t) => (t - now).clamp(1, MAX_SLEEP_SEC) as u64,
        None => MAX_SLEEP_SEC as u64,
    }
}

/// 1 件を送る。`Posting` への遷移に勝った場合だけ送る(None なら誰かが送信中、または存在しない)。
/// `allow_retry` のとき、再試行できる失敗は `Pending` に戻して None を返す。
async fn attempt(state: &AppState, id: &str, now: i64, allow_retry: bool) -> Option<Outcome> {
    let entry = match state.scheduled_posts.begin_post(id) {
        Ok(Some(e)) => e,
        Ok(None) => return None,
        Err(e) => {
            log::warn!("scheduled post {id}: failed to start: {e}");
            return None;
        }
    };
    let account_id = entry.account_id.clone();
    let result = match state.client_for(&account_id) {
        Ok(client) => create_note(&client, &to_note_draft(&entry.input)).await,
        Err(e) => Err(e),
    };
    let fail = |message: String| {
        if let Err(e) = state.scheduled_posts.finish_failed(id, &message) {
            log::warn!("scheduled post {id}: failed to record failure: {e}");
        }
        Some(Outcome::Failed { account_id: account_id.clone(), id: id.to_string(), message })
    };
    match result {
        Ok(note) => {
            if let Err(e) = state.scheduled_posts.finish_posted(id) {
                log::warn!("scheduled post {id}: posted but failed to remove: {e}");
            }
            Some(Outcome::Posted { account_id, id: id.to_string(), note_id: note.id })
        }
        Err(e) => match classify(&e) {
            Failure::Retry if allow_retry && (entry.attempts as usize) < RETRY_DELAYS_SEC.len() => {
                let delay = RETRY_DELAYS_SEC[entry.attempts as usize];
                if let Err(err) = state.scheduled_posts.finish_retry(id, entry.attempts + 1, now + delay) {
                    log::warn!("scheduled post {id}: failed to schedule retry: {err}");
                }
                None
            }
            Failure::Retry => fail(format!("再試行しても投稿できませんでした: {e}")),
            Failure::Fatal(message) => fail(message),
        },
    }
}

/// 期限が来た予約を処理する。猶予を超えていれば `Expired`、そうでなければ送信する。
pub async fn process_due(state: &AppState, now: i64) -> Vec<Outcome> {
    let mut out = Vec::new();
    for e in state.scheduled_posts.due(now) {
        if now - e.scheduled_at > MISSED_GRACE_SEC {
            if state.scheduled_posts.mark_expired(&e.id).unwrap_or(false) {
                out.push(Outcome::Expired { account_id: e.account_id.clone(), id: e.id.clone() });
            }
            continue;
        }
        if let Some(o) = attempt(state, &e.id, now, true).await {
            out.push(o);
        }
    }
    out
}

/// 期限切れ・失敗した予約を、今すぐ 1 回だけ送る(再試行しない)。
pub async fn post_now(state: &AppState, account_id: &str, id: &str, now: i64) -> Result<Outcome> {
    let entry = state
        .scheduled_posts
        .get(id)
        .filter(|e| e.account_id == account_id)
        .ok_or_else(|| Error::NotFound(format!("scheduled post: {id}")))?;
    match entry.status {
        LocalScheduleStatus::Expired | LocalScheduleStatus::Failed => {}
        LocalScheduleStatus::Posting => return Err(Error::Invalid("投稿処理中です".into())),
        LocalScheduleStatus::Pending => {
            return Err(Error::Invalid(
                "予約時刻前の予約は「今すぐ投稿」できません。取り消して通常の投稿にしてください".into(),
            ))
        }
    }
    attempt(state, id, now, false)
        .await
        .ok_or_else(|| Error::Invalid("投稿処理中です".into()))
}

pub fn schedule_local(
    state: &AppState,
    account_id: &str,
    input: DraftInput,
    scheduled_at: i64,
    now: i64,
) -> Result<LocalScheduledNote> {
    state.host_token(account_id)?; // 未登録のアカウントは Invalid
    if scheduled_at <= now {
        return Err(Error::Invalid("予約日時は現在より後にしてください".into()));
    }
    let entry = state.scheduled_posts.add(account_id, scheduled_at, input, now)?;
    state.scheduler_wakeup.notify_one();
    Ok(entry.to_local_note())
}

pub fn list_local(state: &AppState, account_id: &str) -> Vec<LocalScheduledNote> {
    state
        .scheduled_posts
        .list_for_account(account_id)
        .iter()
        .map(|e| e.to_local_note())
        .collect()
}

pub fn cancel_local(state: &AppState, account_id: &str, id: &str) -> Result<()> {
    state.scheduled_posts.cancel(account_id, id)?;
    state.scheduler_wakeup.notify_one();
    Ok(())
}
```

Run: `cd src-tauri && cargo test --lib scheduler 2>&1 | tail -30`
Expected: PASS(20 件前後)。`a_refused_connection_is_classified_as_retry` が落ちる場合は、`error.rs` の接頭辞(`connect:`)と reqwest の `is_connect()` の判定を確認して報告する(実装側を曲げて通さない)。

- [ ] **Step 3: 全体を確認してコミット**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^test result|FAILED" ; cargo build 2>&1 | grep -c warning`
Expected: PASS。警告は、まだ呼ばれていない関数(`process_due` など)の `dead_code` だけ(Task 3 で解消する)。

```bash
git add src-tauri/src/scheduler.rs src-tauri/src/lib.rs
git commit -m "feat: クライアント側予約のスケジューラの中核ロジックを追加(#60 B)"
```

---

### Task 3: イベント・command・起動配線・アカウント削除(Rust)とバインディング

**Files:**
- Modify: `src-tauri/src/events.rs`(2 つのイベント)
- Modify: `src-tauri/src/scheduler.rs`(`emit_outcome`、`spawn` を追加)
- Modify: `src-tauri/src/commands/scheduled.rs`(4 つの command を追加)
- Modify: `src-tauri/src/commands/account.rs`(`remove_account` で予約も消す)
- Modify: `src-tauri/src/lib.rs`(`collect_commands!` / `collect_events!` への登録、`setup` でのストアの永続化・復旧・スケジューラ起動、バインディングのアサーション)
- Modify: `frontend/src/bindings/tauri.gen.ts`(再生成のみ)

**Interfaces:**
- Consumes: Task 2 の `scheduler::{schedule_local, list_local, cancel_local, post_now, process_due, next_sleep, now_sec, Outcome, EXPIRED_MESSAGE}`
- Produces(TS 側):
  - `commands.scheduleNoteLocal(accountId: string, input: DraftInput, scheduledAt: number): Result<LocalScheduledNote>`
  - `commands.listLocalScheduledNotes(accountId: string): Result<LocalScheduledNote[]>`
  - `commands.cancelLocalScheduledNote(accountId: string, id: string): Result<null>`
  - `commands.postLocalScheduledNow(accountId: string, id: string): Result<null>`
  - `events.scheduledPostPosted`(`{ accountId, id, noteId }`)、`events.scheduledPostFailed`(`{ accountId, id, message }`)
  - 型 `LocalScheduledNote`、`LocalScheduleStatus`

- [ ] **Step 1: 失敗するバインディングのアサーションを足す**

`src-tauri/src/lib.rs` の `generates_frontend_bindings` で、予約投稿(Issue #60)のアサーション群の末尾に追加する。

```rust
        // クライアント側予約(Issue #60 B)のコマンド・型・イベント
        assert!(ts.contains("scheduleNoteLocal"), "missing scheduleNoteLocal command");
        assert!(ts.contains("listLocalScheduledNotes"), "missing listLocalScheduledNotes command");
        assert!(ts.contains("cancelLocalScheduledNote"), "missing cancelLocalScheduledNote command");
        assert!(ts.contains("postLocalScheduledNow"), "missing postLocalScheduledNow command");
        assert!(ts.contains("LocalScheduledNote"), "missing LocalScheduledNote type");
        assert!(ts.contains("ScheduledPostPosted"), "missing ScheduledPostPosted event");
        assert!(ts.contains("ScheduledPostFailed"), "missing ScheduledPostFailed event");
        assert!(
            ts.contains("\"pending\"") && ts.contains("\"expired\""),
            "LocalScheduleStatus should export camelCase variants"
        );
```

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: FAIL(`missing scheduleNoteLocal command`)

- [ ] **Step 2: イベントを足す**

`src-tauri/src/events.rs` の末尾に追加する。

```rust
/// クライアント側の予約投稿が投稿された(Issue #60 B)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPostPosted {
    pub account_id: String,
    /// ローカル予約の ID(投稿後は一覧から消える)。
    pub id: String,
    /// 投稿されたノートの ID。
    pub note_id: String,
}

/// クライアント側の予約投稿が失敗した、または期限切れになった(Issue #60 B)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPostFailed {
    pub account_id: String,
    pub id: String,
    pub message: String,
}
```

`src-tauri/src/lib.rs` の `collect_events![...]` に、`events::ColumnNotificationGapFill,` の次の行として `events::ScheduledPostPosted,` と `events::ScheduledPostFailed,` を追加する。

- [ ] **Step 3: スケジューラのループとイベント送出を足す**

`src-tauri/src/scheduler.rs` の `use` 群に追加する。

```rust
use tauri::Manager;
use tauri_specta::Event as _;
```

`schedule_local` の前(`post_now` の後)に追加する。

```rust
/// 結果をフロントへ通知する(トースト・一覧の読み直し用)。`Expired` も、ユーザーに知らせるため
/// 失敗のイベントで通知する。
pub fn emit_outcome(app: &tauri::AppHandle, outcome: &Outcome) {
    use crate::events::{ScheduledPostFailed, ScheduledPostPosted};
    let result = match outcome {
        Outcome::Posted { account_id, id, note_id } => ScheduledPostPosted {
            account_id: account_id.clone(),
            id: id.clone(),
            note_id: note_id.clone(),
        }
        .emit(app),
        Outcome::Failed { account_id, id, message } => ScheduledPostFailed {
            account_id: account_id.clone(),
            id: id.clone(),
            message: message.clone(),
        }
        .emit(app),
        Outcome::Expired { account_id, id } => ScheduledPostFailed {
            account_id: account_id.clone(),
            id: id.clone(),
            message: EXPIRED_MESSAGE.to_string(),
        }
        .emit(app),
    };
    if let Err(e) = result {
        log::warn!("failed to emit scheduled post event: {e}");
    }
}

/// 常駐タスクを起動する。次の期限まで(最大 `MAX_SLEEP_SEC` 秒)寝て、期限が来た予約を処理する。
/// 予約の追加・取り消しなどで `scheduler_wakeup` が通知されたら、すぐ起き直す。
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            for outcome in process_due(&state, now_sec()).await {
                emit_outcome(&app, &outcome);
            }
            let wait = next_sleep(now_sec(), state.scheduled_posts.next_wakeup());
            let wakeup = state.scheduler_wakeup.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {}
                _ = wakeup.notified() => {}
            }
        }
    });
}
```

- [ ] **Step 4: command を足す**

`src-tauri/src/commands/scheduled.rs` の `use` 群に追加する(既存の `use` はそのまま)。

```rust
use crate::domain::LocalScheduledNote;
use crate::scheduler;
use crate::store::draft::DraftInput;
```

末尾に追加する。

```rust
/// クライアント側(ローカル)予約を作る(Issue #60 B)。`scheduled_at` は epoch 秒。
/// 添付は呼び出し側でアップロード済みの `fileIds`(`DraftInput.file_ids`)。
#[tauri::command]
#[specta::specta]
pub async fn schedule_note_local(
    state: State<'_, AppState>,
    account_id: String,
    input: DraftInput,
    scheduled_at: u32,
) -> Result<LocalScheduledNote> {
    scheduler::schedule_local(&state, &account_id, input, i64::from(scheduled_at), scheduler::now_sec())
}

/// そのアカウントのローカル予約(予約日時の昇順)。
#[tauri::command]
#[specta::specta]
pub async fn list_local_scheduled_notes(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<LocalScheduledNote>> {
    Ok(scheduler::list_local(&state, &account_id))
}

/// ローカル予約の取り消し(削除)。送信中のものは取り消せない。
#[tauri::command]
#[specta::specta]
pub async fn cancel_local_scheduled_note(
    state: State<'_, AppState>,
    account_id: String,
    id: String,
) -> Result<()> {
    scheduler::cancel_local(&state, &account_id, &id)
}

/// 期限切れ・失敗したローカル予約を、今すぐ 1 回だけ送る。結果はイベントでも通知する。
#[tauri::command]
#[specta::specta]
pub async fn post_local_scheduled_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    id: String,
) -> Result<()> {
    let outcome = scheduler::post_now(&state, &account_id, &id, scheduler::now_sec()).await?;
    scheduler::emit_outcome(&app, &outcome);
    Ok(())
}
```

`src-tauri/src/lib.rs` の `collect_commands![...]` の `commands::scheduled::cancel_scheduled_note,` の次に追加する。

```rust
            commands::scheduled::schedule_note_local,
            commands::scheduled::list_local_scheduled_notes,
            commands::scheduled::cancel_local_scheduled_note,
            commands::scheduled::post_local_scheduled_now,
```

- [ ] **Step 5: 起動配線とアカウント削除**

`src-tauri/src/lib.rs` の `setup` で、`let drafts = DraftStore::new(drafts_path).expect("failed to open drafts file");` の次に追加する。

```rust
            // クライアント側の予約投稿(Issue #60 B)。起動時に、送信中のまま残っていた予約は結果が
            // 分からないため、再送せず失敗にする。
            let scheduled_posts = store::ScheduledPostStore::new(config_dir.join("scheduled_posts.json"))
                .expect("failed to open scheduled posts file");
            match scheduled_posts.recover_posting() {
                Ok(0) => {}
                Ok(n) => log::warn!("{n} scheduled post(s) were left in 'posting'; marked as failed"),
                Err(e) => log::warn!("failed to recover scheduled posts: {e}"),
            }
```

`app.manage(AppState::new(Box::new(KeyringStore), settings, drafts, cache, cache_dir.clone()));` を次に置き換える。

```rust
            app.manage(
                AppState::new(Box::new(KeyringStore), settings, drafts, cache, cache_dir.clone())
                    .with_scheduled_posts(scheduled_posts),
            );
            scheduler::spawn(app.handle().clone());
```

`src-tauri/src/commands/account.rs` の `remove_account` で、`state.accounts.lock().unwrap().remove(&account_id)?;` の直前に追加する。

```rust
    // そのアカウントのクライアント側予約(Issue #60 B)も消す。送信先が無くなるため。
    state.scheduled_posts.remove_account(&account_id)?;
```

- [ ] **Step 6: テストが通り、バインディングが再生成されることを確認する**

Run:
```bash
cd src-tauri && cargo test generates_frontend_bindings
cargo test 2>&1 | grep -E "^test result|FAILED"
cargo build 2>&1 | grep -c "warning: unused\|never used\|dead_code"
cd .. && git diff --stat frontend/src/bindings/tauri.gen.ts
```
Expected: PASS(Rust 全体で既存 + 新規。581 + 約 33 件)。`dead_code` の警告は 0(Task 1・2 の保留が解消)。`tauri.gen.ts` は追加のみ(4 つの command、`LocalScheduledNote`、`LocalScheduleStatus`、2 つのイベント、`events` の定義)。`scheduledAt: number` が `ScheduledNote` で `number`、`LocalScheduleStatus` が `"pending" | "posting" | "expired" | "failed"` であること。

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src frontend/src/bindings/tauri.gen.ts
git commit -m "feat: クライアント側予約のcommandとイベントと起動配線を追加(#60 B)"
```

---

### Task 4: デバッグビルド限定のバージョン上書き(Rust)

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`cached_server_version`)

**Interfaces:**
- Produces: 環境変数 `TSUMUGI_DEBUG_SERVER_VERSION`(デバッグビルドのみ)。設定されていれば、`cached_server_version` はその値を返す(E2E が Task 8・9 で使う)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の末尾にある `#[cfg(test)] mod tests` に追加する(無ければ末尾に `#[cfg(test)] mod version_override_tests { use super::*; ... }` として足す)。

```rust
    /// 環境変数そのものをテストに使うと、並行して走る他のテストの判定に影響するため、
    /// 値の解釈(空白・空文字の扱い)だけを純関数で固定する。環境変数が実際にアプリまで届くことは、
    /// E2E(`e2e/specs-scheduled/`)で確認する。
    #[test]
    fn parse_version_override_trims_and_ignores_blank() {
        assert_eq!(parse_version_override(None), None);
        assert_eq!(parse_version_override(Some("")), None);
        assert_eq!(parse_version_override(Some("   ")), None);
        assert_eq!(parse_version_override(Some("2025.9.0")), Some("2025.9.0".to_string()));
        assert_eq!(parse_version_override(Some(" 2025.9.0 \n")), Some("2025.9.0".to_string()));
    }
```

Run: `cd src-tauri && cargo test --lib parse_version_override`
Expected: FAIL(`cannot find function parse_version_override`)

- [ ] **Step 2: 実装する**

`src-tauri/src/commands/column.rs` の `cached_server_version` の直前に追加する。

```rust
/// 環境変数の値を、上書きするバージョン文字列として解釈する(空白は除き、空なら無し)。
#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn parse_version_override(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

/// **デバッグビルド限定**: `TSUMUGI_DEBUG_SERVER_VERSION` で、アプリが認識する接続先サーバーの
/// バージョンを上書きする。E2E で、予約に対応した使い捨ての Misskey を「非対応サーバー」に見せて、
/// クライアント側予約(Issue #60 B)の経路を通すために使う。実サーバーは変わらないので、投稿など
/// 以降の API 呼び出しは本物のまま。リリースビルド(`debug_assertions` なし)には含めない。
#[cfg(debug_assertions)]
fn debug_server_version_override() -> Option<String> {
    parse_version_override(std::env::var("TSUMUGI_DEBUG_SERVER_VERSION").ok().as_deref())
}

#[cfg(not(debug_assertions))]
fn debug_server_version_override() -> Option<String> {
    None
}
```

`cached_server_version` の本体の先頭(`if let Some(v) = state.server_version(account_id) {` の前)に追加する。

```rust
    if let Some(v) = debug_server_version_override() {
        return Ok(Some(v));
    }
```

- [ ] **Step 3: テストとリリースビルドの警告を確認する**

Run:
```bash
cd src-tauri && cargo test --lib parse_version_override
cargo test --lib 2>&1 | grep -E "^test result|FAILED"
cargo check --release 2>&1 | grep -E "warning: (unused|function .* is never used)|error" | head
```
Expected: テストは PASS。`cargo check --release` に、`parse_version_override` / `debug_server_version_override` に関する警告・エラーが出ない(`cargo check --release` は初回に時間がかかる。依存のビルドが重く、10 分を超える場合は、ここを飛ばして理由を報告する)。

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "feat: E2E用にデバッグビルド限定でサーバーバージョンを上書きできるようにする(#60 B)"
```

---

### Task 5: フロントの型ヘルパとストア

**Files:**
- Create: `frontend/src/lib/scheduledList.ts`
- Create: `frontend/src/lib/scheduledList.test.ts`
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: Task 3 のバインディング(`commands.scheduleNoteLocal`、`events.scheduledPostPosted` / `scheduledPostFailed`、型 `LocalScheduleStatus`、`DraftInput`、`ScheduledNote`)
- Produces:
  - `ScheduledListItem`(`{ origin: "server"; note: ScheduledNote } | { origin: "local"; note: ScheduledNote; status: LocalScheduleStatus; error: string | null }`)、`localStatusLabel(status: LocalScheduleStatus): string | null`、`LOCAL_SCHEDULE_NOTICE`(定数: 「アプリを起動している間だけ投稿されます」)
  - `app.scheduleNoteLocal(accountId: string, input: DraftInput, scheduledAt: number): Promise<void>`、`app.scheduledPostTick: number`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/scheduledList.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { LOCAL_SCHEDULE_NOTICE, localStatusLabel } from "./scheduledList";

describe("localStatusLabel", () => {
  it("待機中以外の状態を日本語にする(待機中は表示しない)", () => {
    expect(localStatusLabel("pending")).toBeNull();
    expect(localStatusLabel("posting")).toBe("投稿中");
    expect(localStatusLabel("expired")).toBe("期限切れ");
    expect(localStatusLabel("failed")).toBe("投稿に失敗");
  });
});

describe("LOCAL_SCHEDULE_NOTICE", () => {
  it("ローカル予約の制約を伝える文言", () => {
    expect(LOCAL_SCHEDULE_NOTICE).toBe("アプリを起動している間だけ投稿されます");
  });
});
```

Run: `cd frontend && npx vitest run src/lib/scheduledList.test.ts`
Expected: FAIL(`./scheduledList` が見つからない)

- [ ] **Step 2: 実装する**

`frontend/src/lib/scheduledList.ts`:

```ts
// 予約一覧(ScheduledModal)の行の型と、ローカル予約の表示用ヘルパ。Svelte に依存しない。
import type { LocalScheduleStatus, ScheduledNote } from "../bindings/tauri.gen";

/// 予約一覧の 1 行。サーバー予約(notes/drafts)とローカル予約(tsumugi が保持して投稿する)を同じ形で扱う。
export type ScheduledListItem =
  | { origin: "server"; note: ScheduledNote }
  | { origin: "local"; note: ScheduledNote; status: LocalScheduleStatus; error: string | null };

/// ローカル予約は、tsumugi が起動している間しか投稿できない。予約時と一覧で伝える。
export const LOCAL_SCHEDULE_NOTICE = "アプリを起動している間だけ投稿されます";

/// ローカル予約の状態の表示名。待機中(pending)は予約日時だけで足りるので表示しない。
export function localStatusLabel(status: LocalScheduleStatus): string | null {
  switch (status) {
    case "posting":
      return "投稿中";
    case "expired":
      return "期限切れ";
    case "failed":
      return "投稿に失敗";
    default:
      return null;
  }
}
```

Run: `cd frontend && npx vitest run src/lib/scheduledList.test.ts`
Expected: PASS

- [ ] **Step 3: ストアに追加する**

`frontend/src/lib/store.svelte.ts`:

(a) bindings の型 import(`ScheduleCapabilities,` を足した箇所)に `DraftInput,` が無ければ足す。

(b) `postNote` の直後、`scheduleNote` の次に追加する。

```ts
  /// クライアント側(ローカル)予約(Issue #60 B)。`scheduledAt` は epoch 秒。
  /// サーバーが予約に対応していないアカウント用。tsumugi が起動している間だけ投稿される。
  async scheduleNoteLocal(accountId: string, input: DraftInput, scheduledAt: number) {
    try {
      await unwrapAcc(accountId, commands.scheduleNoteLocal(accountId, input, scheduledAt));
      this.#log("success", "予約しました(アプリを起動している間だけ投稿されます)");
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

(c) クラスのフィールド宣言(`booting = $state(true);` の近く)に追加する。

```ts
  /// クライアント側予約の変化(投稿・失敗・期限切れ)のたびに増える。予約一覧(ScheduledModal)が、
  /// 開いている間にローカル予約を読み直す合図に使う。
  scheduledPostTick = $state(0);
```

(d) `#subscribe()` の、`events.columnNotification.listen(...)` ブロックの次(同じ `this.#unlisten.push(...)` の形)に追加する。

```ts
    this.#unlisten.push(
      await events.scheduledPostPosted.listen(() => {
        this.#log("success", "予約投稿を投稿しました");
        this.scheduledPostTick++;
      }),
    );
    this.#unlisten.push(
      await events.scheduledPostFailed.listen((e) => {
        this.#log("error", `予約投稿を投稿できませんでした: ${e.payload.message}`);
        this.scheduledPostTick++;
      }),
    );
```

- [ ] **Step 4: 検証してコミット**

Run: `cd frontend && npx vitest run src/lib/scheduledList.test.ts && pnpm check 2>&1 | tail -2 && pnpm test 2>&1 | tail -4`
Expected: PASS、`pnpm check` は 0 errors / 1 warning、全体 PASS(既存の 725 件超)。

```bash
git add frontend/src/lib/scheduledList.ts frontend/src/lib/scheduledList.test.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: ローカル予約の一覧用ヘルパとストアのラッパ・イベント購読を追加(#60 B)"
```

---

### Task 6: ComposeBar の経路の振り分け

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`
- Modify: `frontend/src/ui/ComposeBar.test.ts`

**Interfaces:**
- Consumes: Task 5 の `app.scheduleNoteLocal`、`LOCAL_SCHEDULE_NOTICE`、`app.getScheduleCapabilities`
- Produces: ComposeBar の状態 `scheduleServerSide: boolean | null`(null=確認中、true=サーバー予約、false=ローカル予約)。testid `compose-schedule-local-hint`(ローカル予約のときだけ出る注意書き)。Task 7 が `scheduleServerSide` を `ScheduledModal` に渡す。

- [ ] **Step 1: テストを新しい仕様に書き換える(RED)**

`frontend/src/ui/ComposeBar.test.ts`:

(a) `mockCaps` に、ローカル予約の command を足す。

```ts
      if (cmd === "schedule_note_local") return Promise.resolve({ note: { id: "l1" }, status: "pending", error: null });
```
(`if (cmd === "schedule_note") ...` の次に足す。)

(b) 「非対応サーバーでは予約ボタンを出さない」を、次に置き換える。

```ts
  it("非対応サーバーでも予約ボタンが出て、ローカル予約の注意書きが出る", async () => {
    mockCaps(false);
    const { findByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const hint = await findByTestId("compose-schedule-local-hint");
    expect(hint.textContent).toContain("アプリを起動している間だけ");
  });

  it("対応サーバーでは注意書きを出さない", async () => {
    mockCaps(true);
    const { findByTestId, queryByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_schedule_capabilities", { accountId: "acc1" }));
    expect(queryByTestId("compose-schedule-local-hint")).toBeNull();
  });

  it("非対応サーバーでは schedule_note_local を呼ぶ(schedule_note・post_note は呼ばない)", async () => {
    mockCaps(false);
    const { findByTestId, getByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const value = futureInput();
    await setDateTime(getByTestId("compose-schedule-input"), value);
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "ローカルで予約" } });
    await fireEvent.click(getByTestId("compose-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note_local", {
        accountId: "acc1",
        input: expect.objectContaining({ text: "ローカルで予約", visibility: "public", fileIds: [] }),
        scheduledAt: localInputToEpochSec(value),
      }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("post_note", expect.anything());
    // 成功したら作成欄が空になり、投稿ボタンも元に戻る
    await waitFor(() => expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe(""));
    expect(getByTestId("compose-submit").textContent).toContain("投稿");
  });
```

(c) 「期間指定の投票は予約日時を基準に締切を計算する」の直後に、ローカル経路の同じテストを足す。

```ts
  // Review Focus 5: ローカル予約でも、期間指定の投票は予約日時を基準にする
  it("ローカル予約でも、期間指定の投票は予約日時を基準に締切を計算する", async () => {
    mockCaps(false);
    const ui = render(ComposeBar);
    const value = futureInput();
    await setupPoll(ui, value);
    await fireEvent.click(ui.getByText("期間を指定"));
    await fireEvent.input(ui.container.querySelector("input[type=number]") as HTMLInputElement, {
      target: { value: "2" },
    });
    await fireEvent.click(ui.getByTestId("compose-submit"));

    const scheduledAt = localInputToEpochSec(value) as number;
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note_local", {
        accountId: "acc1",
        input: expect.objectContaining({
          poll: expect.objectContaining({ expiresAt: scheduledAt * 1000 + 2 * 3_600_000 }),
        }),
        scheduledAt,
      }),
    );
  });
```

(d) A の 2 つのアカウント切り替えテストを、次の 2 つに置き換える。

```ts
  it("予約日時はアカウントを切り替えても保たれ、経路は切り替え先の対応状況で決まる", async () => {
    app.accounts = [...app.accounts, acc2];
    invokeMock.mockImplementation((cmd: string, args?: { accountId?: string }) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: args?.accountId === "acc1" });
      if (cmd === "schedule_note_local") return Promise.resolve({ note: { id: "l1" }, status: "pending", error: null });
      return Promise.resolve(cmd === "list_drafts" ? [] : null);
    });
    const ui = render(ComposeBar);
    await fireEvent.click(await ui.findByTestId("compose-schedule-toggle"));
    const value = futureInput();
    await setDateTime(ui.getByTestId("compose-schedule-input"), value);
    expect(ui.queryByTestId("compose-schedule-local-hint")).toBeNull(); // acc1 はサーバー予約

    await switchAccount(ui, "acc2");
    // 日時は消えず、acc2 は非対応なのでローカル予約の注意書きが出る
    expect(await ui.findByTestId("compose-schedule-local-hint")).toBeTruthy();
    expect(ui.getByTestId("compose-submit").textContent).toContain("予約");

    await fireEvent.input(ui.getByTestId("compose-textarea"), { target: { value: "acc2 で予約" } });
    await fireEvent.click(ui.getByTestId("compose-submit"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note_local", expect.objectContaining({ accountId: "acc2" })),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("post_note", expect.anything());
  });

  // Review Focus 4: 方式が決まる前の予約日時つきの送信は、即時投稿にも予約にもしない
  it("切り替え先の予約方式を確認している間は、予約日時つきの送信を止める", async () => {
    app.accounts = [...app.accounts, acc2];
    invokeMock.mockImplementation((cmd: string, args?: { accountId?: string }) => {
      if (cmd === "get_schedule_capabilities")
        return args?.accountId === "acc1" ? Promise.resolve({ available: true }) : new Promise(() => {});
      return Promise.resolve(cmd === "list_drafts" ? [] : null);
    });
    const ui = render(ComposeBar);
    await fireEvent.click(await ui.findByTestId("compose-schedule-toggle"));
    await setDateTime(ui.getByTestId("compose-schedule-input"), futureInput());
    await fireEvent.input(ui.getByTestId("compose-textarea"), { target: { value: "x" } });

    await switchAccount(ui, "acc2");
    await fireEvent.click(ui.getByTestId("compose-submit"));

    expect(await ui.findByText(/方式を確認中/)).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("post_note", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note_local", expect.anything());
  });
```

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "予約"`
Expected: FAIL(経路の振り分けがまだ無いため。「注意書き」「schedule_note_local」「方式を確認中」のテストが落ちる)

- [ ] **Step 2: ComposeBar を実装する**

`frontend/src/ui/ComposeBar.svelte`:

(a) import に追加する(既存の `../lib/schedule` import の次)。

```ts
  import { LOCAL_SCHEDULE_NOTICE } from "../lib/scheduledList";
```

(b) 予約の状態宣言(`scheduleAvailable` から `scheduleCapsGen`、`$effect` まで)を、次に置き換える。

```ts
  // 予約投稿(Issue #60)。scheduleAt は datetime-local 文字列(空なら通常の投稿)。
  // scheduleServerSide: そのアカウントのサーバーがサーバー側予約に対応しているか。
  //   true → サーバー予約(notes/drafts)、false → ローカル予約(tsumugi が保持して投稿する)、
  //   null → 確認中(経路が決まらない間は、予約日時つきの送信を止める)。
  let scheduleServerSide = $state<boolean | null>(null);
  let scheduleAt = $state("");
  let showSchedulePicker = $state(false);
  let showScheduledModal = $state(false);
  let scheduleCapsGen = 0;
  $effect(() => {
    const id = accountId;
    const gen = ++scheduleCapsGen;
    scheduleServerSide = null;
    if (!id) return;
    // アカウント切り替え直後に古いアカウントの結果で上書きしないよう世代で弾く。
    // 取得に失敗したときは(getScheduleCapabilities が available:false を返すので)ローカル予約になる。
    void app.getScheduleCapabilities(id).then((c) => {
      if (gen !== scheduleCapsGen) return;
      scheduleServerSide = c?.available ?? false;
    });
  });
```

(c) `buildDraftInput` を、投票の締切の基準を渡せる形に変える。

```ts
  function buildDraftInput(pollBaseMs: number = Date.now()): DraftInput {
```
とし、本体の `expiresAt: computePollExpiresAt()` を `expiresAt: computePollExpiresAt(pollBaseMs)` にする。

(d) `submit()` の、予約方式の確認中のガードを置き換える。

```ts
    // 切り替え直後で予約方式(サーバー/ローカル)の確認が終わっていないのに予約日時が残っている場合、
    // 即時投稿にも予約にも倒さず止める。
    if (scheduleAt && scheduleServerSide === null) {
      err = "このアカウントの予約投稿の方式を確認中です。少し待ってからやり直してください";
      return;
    }
```

(e) `submit()` の送信部分(`if (scheduledAtSec !== null) await app.scheduleNote(...) else await app.postNote(...)`)を置き換える。

```ts
      if (scheduledAtSec === null) await app.postNote(accountId, draft);
      else if (scheduleServerSide) await app.scheduleNote(accountId, draft, scheduledAtSec);
      else await app.scheduleNoteLocal(accountId, buildDraftInput(scheduledAtSec * 1000), scheduledAtSec);
```

(f) マークアップ: 予約ボタンの出し分けを、`{#if scheduleAvailable}` から `{#if accountId}` に変える。ピッカー行の `DateTimeInput` の直後に、注意書きを足す。

```svelte
          {#if scheduleServerSide === false}
            <span class="text-xs text-muted-foreground" data-testid="compose-schedule-local-hint">{LOCAL_SCHEDULE_NOTICE}</span>
          {/if}
```

(g) 「予約一覧」ボタンは、方式が決まるまで押せないようにする(一覧がサーバー予約を含めるかが決まらないため)。`<Button ... data-testid="compose-scheduled-list" ...>` に `disabled={scheduleServerSide === null}` を足す。

(h) `showScheduledModal` を使う `<ScheduledModal ...>` の呼び出し(Task 7 で書き換える)は、このタスクでは変えない。ただし `scheduleAvailable` を参照している他の箇所が残っていないこと(`grep -n scheduleAvailable frontend/src/ui/ComposeBar.svelte` が 0 件)を確認する。

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts`
Expected: PASS(「予約」のテストと既存テスト全部)。`pnpm check` は Task 7 まで `ScheduledModal` の props が合わずエラーになる可能性があるため、このタスクでは実行しない(Task 7 の最後に実行する)。

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: ComposeBarで予約の経路をサーバー側とローカルに振り分ける(#60 B)"
```

---

### Task 7: 予約一覧モーダルの統合と「作成欄に戻す」

**Files:**
- Modify: `frontend/src/ui/ScheduledModal.svelte`(全面書き換え)
- Modify: `frontend/src/ui/ScheduledModal.test.ts`(全面書き換え)
- Modify: `frontend/src/ui/ComposeBar.svelte`(`restoreScheduled` と `<ScheduledModal>` の呼び出し)
- Modify: `frontend/src/ui/ComposeBar.test.ts`(ローカル予約の「作成欄に戻す」のテストを足す)

**Interfaces:**
- Consumes: Task 5 の `ScheduledListItem` / `localStatusLabel` / `LOCAL_SCHEDULE_NOTICE`、Task 3 の `commands.listLocalScheduledNotes` / `cancelLocalScheduledNote` / `postLocalScheduledNow`、Task 6 の `scheduleServerSide`
- Produces: `<ScheduledModal accountId serverSide reloadToken onrestore onclose />`(`onrestore: (item: ScheduledListItem) => void`)。testid: `scheduled-item-<id>`(行)、`scheduled-failed-<id>`(サーバー予約の失敗)、`scheduled-local-status-<id>`(ローカルの状態)、`scheduled-local-notice-<id>`(ローカルの注意書き)、`scheduled-run-now-<id>`(今すぐ投稿)、`scheduled-cancel-<id>`、`scheduled-restore-<id>`、`scheduled-more`、`scheduled-retry`、`scheduled-empty`

- [ ] **Step 1: モーダルのテストを全面的に書き換える(RED)**

全コマンド共通の配列を返す従来のモックは、ローカル一覧の追加で壊れる。コマンド別に返すヘルパに替える。`frontend/src/ui/ScheduledModal.test.ts` を次の内容にする。

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { LocalScheduledNote, LocalScheduleStatus, ScheduledNote } from "../bindings/tauri.gen";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: ScheduledModal } = await import("./ScheduledModal.svelte");

function note(id: string, over: Partial<ScheduledNote> = {}): ScheduledNote {
  return {
    id,
    scheduledAt: Math.floor(Date.now() / 1000) + 3600,
    text: `本文${id}`,
    cw: null,
    visibility: "public",
    localOnly: false,
    reactionAcceptance: "all",
    channelId: null,
    poll: null,
    fileIds: [],
    replyNote: null,
    quoteNote: null,
    ...over,
  };
}

function local(id: string, status: LocalScheduleStatus = "pending", over: Partial<ScheduledNote> = {}, error: string | null = null): LocalScheduledNote {
  return { note: note(id, over), status, error };
}

/// コマンドごとに返す値を決める。指定の無いコマンドは null(書き込み系は成功)。
function mockCommands(handlers: Record<string, (args?: unknown) => unknown>) {
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    const h = handlers[cmd];
    if (!h) return Promise.resolve(cmd.startsWith("list_") ? [] : null);
    try {
      return Promise.resolve(h(args));
    } catch (e) {
      return Promise.reject(e);
    }
  });
}

const props = (over: Record<string, unknown> = {}) => ({
  accountId: "acc1",
  serverSide: true,
  reloadToken: 0,
  onrestore: vi.fn(),
  onclose: vi.fn(),
  ...over,
});

beforeEach(() => {
  // 式本体にすると mock 自体が返り、vitest が beforeEach の返り値をクリーンアップ関数として呼んでしまう
  invokeMock.mockReset();
});
afterEach(() => cleanup());

describe("ScheduledModal(サーバー予約)", () => {
  it("予約を予約日時の昇順で表示し、過去のものは「投稿に失敗」と明示する", async () => {
    const now = Math.floor(Date.now() / 1000);
    mockCommands({
      list_scheduled_notes: () => [
        note("later", { scheduledAt: now + 7200 }),
        note("failed", { scheduledAt: now - 600 }),
        note("soon", { scheduledAt: now + 600 }),
      ],
    });
    const { findByTestId, container, queryByTestId } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-soon");
    const order = [...container.ownerDocument.querySelectorAll("[data-testid^='scheduled-item-']")].map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order).toEqual(["scheduled-item-failed", "scheduled-item-soon", "scheduled-item-later"]);
    expect(queryByTestId("scheduled-failed-failed")).not.toBeNull();
    expect(queryByTestId("scheduled-failed-soon")).toBeNull();
    expect(invokeMock).toHaveBeenCalledWith("list_scheduled_notes", { accountId: "acc1", untilId: null, limit: 30 });
  });

  it("予約が無いときは空表示", async () => {
    mockCommands({});
    const { findByTestId } = render(ScheduledModal, props());
    expect(await findByTestId("scheduled-empty")).toBeTruthy();
  });

  it("取り消しで cancel_scheduled_note を呼び、一覧から消す", async () => {
    mockCommands({ list_scheduled_notes: () => [note("a"), note("b")] });
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    await waitFor(() => expect(queryByTestId("scheduled-item-a")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "a" });
    expect(getByTestId("scheduled-item-b")).toBeTruthy();
  });

  it("取り消しに失敗したら一覧に残し、エラーを表示する", async () => {
    mockCommands({
      list_scheduled_notes: () => [note("a")],
      cancel_scheduled_note: () => {
        throw { kind: "network", message: "offline" };
      },
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    expect(await findByText(/offline/)).toBeTruthy();
    expect(getByTestId("scheduled-item-a")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にサーバー予約として渡す", async () => {
    const a = note("a");
    mockCommands({ list_scheduled_notes: () => [a] });
    const p = props();
    const { findByTestId } = render(ScheduledModal, p);
    await fireEvent.click(await findByTestId("scheduled-restore-a"));
    expect(p.onrestore).toHaveBeenCalledWith({ origin: "server", note: a });
  });

  it("1ページ分(30件)返ったら「さらに読み込む」を出し、最後の ID をカーソルに続きを取る", async () => {
    const page1 = Array.from({ length: 30 }, (_, i) => note(`p1_${i}`));
    let call = 0;
    mockCommands({ list_scheduled_notes: () => (call++ === 0 ? page1 : [note("p2_0")]) });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-more"));
    await findByTestId("scheduled-item-p2_0");
    expect(invokeMock).toHaveBeenCalledWith("list_scheduled_notes", { accountId: "acc1", untilId: "p1_29", limit: 30 });
    expect(queryByTestId("scheduled-more")).toBeNull();
  });

  it("1ページ目の取得に失敗したら、空表示は出さずエラーと再読み込みを出す", async () => {
    mockCommands({
      list_scheduled_notes: () => {
        throw { kind: "network", message: "offline" };
      },
    });
    const { findByText, findByTestId, queryByTestId } = render(ScheduledModal, props());
    expect(await findByText(/offline/)).toBeTruthy();
    expect(await findByTestId("scheduled-retry")).toBeTruthy();
    // 「予約なし」と誤解して二重に予約しないよう、空表示は出さない
    expect(queryByTestId("scheduled-empty")).toBeNull();
  });

  it("再読み込みに成功すると一覧が出てエラーが消える", async () => {
    let call = 0;
    mockCommands({
      list_scheduled_notes: () => {
        if (call++ === 0) throw { kind: "network", message: "offline" };
        return [note("a")];
      },
    });
    const { findByTestId, queryByText, queryByTestId } = render(ScheduledModal, props());
    await fireEvent.click(await findByTestId("scheduled-retry"));
    await findByTestId("scheduled-item-a");
    expect(queryByText(/offline/)).toBeNull();
    expect(queryByTestId("scheduled-retry")).toBeNull();
  });

  it("serverSide が false のときはサーバー予約を取得しない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1")] });
    const { findByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-l1");
    expect(invokeMock).not.toHaveBeenCalledWith("list_scheduled_notes", expect.anything());
  });
});

describe("ScheduledModal(ローカル予約)", () => {
  it("サーバー予約とローカル予約を、予約日時の昇順で 1 つの一覧にする", async () => {
    const now = Math.floor(Date.now() / 1000);
    mockCommands({
      list_scheduled_notes: () => [note("s1", { scheduledAt: now + 300 })],
      list_local_scheduled_notes: () => [local("l1", "pending", { scheduledAt: now + 100 }), local("l2", "pending", { scheduledAt: now + 900 })],
    });
    const { findByTestId, container } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-l2");
    const order = [...container.ownerDocument.querySelectorAll("[data-testid^='scheduled-item-']")].map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order).toEqual(["scheduled-item-l1", "scheduled-item-s1", "scheduled-item-l2"]);
  });

  it("待機中のローカル予約には注意書きを出し、状態ラベルは出さない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1", "pending")] });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    const notice = await findByTestId("scheduled-local-notice-l1");
    expect(notice.textContent).toContain("アプリを起動している間だけ");
    expect(queryByTestId("scheduled-local-status-l1")).toBeNull();
  });

  it("期限切れ・失敗は状態と理由を出し、「今すぐ投稿」「作成欄に戻す」「取り消し」を出す", async () => {
    mockCommands({
      list_local_scheduled_notes: () => [
        local("e1", "expired"),
        local("f1", "failed", {}, "通信がタイムアウトしました。投稿されたか確認してください"),
      ],
    });
    const { findByTestId, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-e1");
    expect(getByTestId("scheduled-local-status-e1").textContent).toContain("期限切れ");
    expect(getByTestId("scheduled-local-status-f1").textContent).toContain("投稿に失敗");
    expect(getByTestId("scheduled-item-f1").textContent).toContain("タイムアウト");
    for (const id of ["e1", "f1"]) {
      expect(getByTestId(`scheduled-run-now-${id}`)).toBeTruthy();
      expect(getByTestId(`scheduled-restore-${id}`)).toBeTruthy();
      expect(getByTestId(`scheduled-cancel-${id}`)).toBeTruthy();
    }
  });

  // Review Focus 2: 送信中の予約は、取り消しも今すぐ投稿も作成欄に戻すもできない
  it("投稿中のローカル予約は操作できない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("p1", "posting")] });
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-p1");
    expect(getByTestId("scheduled-local-status-p1").textContent).toContain("投稿中");
    for (const kind of ["run-now", "restore", "cancel"]) {
      expect(queryByTestId(`scheduled-${kind}-p1`)).toBeNull();
    }
  });

  it("待機中のローカル予約には「今すぐ投稿」を出さない", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1", "pending")] });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await findByTestId("scheduled-item-l1");
    expect(queryByTestId("scheduled-run-now-l1")).toBeNull();
  });

  it("ローカル予約の取り消しは cancel_local_scheduled_note を呼び、一覧から消す", async () => {
    mockCommands({ list_local_scheduled_notes: () => [local("l1"), local("l2")] });
    const { findByTestId, queryByTestId, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-cancel-l1"));
    await waitFor(() => expect(queryByTestId("scheduled-item-l1")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_local_scheduled_note", { accountId: "acc1", id: "l1" });
    expect(invokeMock).not.toHaveBeenCalledWith("cancel_scheduled_note", expect.anything());
    expect(getByTestId("scheduled-item-l2")).toBeTruthy();
  });

  it("「今すぐ投稿」で post_local_scheduled_now を呼び、ローカル一覧を読み直す", async () => {
    let listed = 0;
    mockCommands({
      list_local_scheduled_notes: () => (listed++ === 0 ? [local("e1", "expired")] : []),
    });
    const { findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-run-now-e1"));
    expect(invokeMock).toHaveBeenCalledWith("post_local_scheduled_now", { accountId: "acc1", id: "e1" });
    await waitFor(() => expect(queryByTestId("scheduled-item-e1")).toBeNull());
  });

  it("「今すぐ投稿」に失敗したらエラーを出し、一覧には残す", async () => {
    mockCommands({
      list_local_scheduled_notes: () => [local("e1", "expired")],
      post_local_scheduled_now: () => {
        throw { kind: "invalid", message: "投稿処理中です" };
      },
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, props({ serverSide: false }));
    await fireEvent.click(await findByTestId("scheduled-run-now-e1"));
    expect(await findByText(/投稿処理中です/)).toBeTruthy();
    expect(getByTestId("scheduled-item-e1")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にローカル予約として渡す", async () => {
    const l = local("l1", "expired", {}, null);
    mockCommands({ list_local_scheduled_notes: () => [l] });
    const p = props({ serverSide: false });
    const { findByTestId } = render(ScheduledModal, p);
    await fireEvent.click(await findByTestId("scheduled-restore-l1"));
    expect(p.onrestore).toHaveBeenCalledWith({ origin: "local", note: l.note, status: "expired", error: null });
  });

  it("reloadToken が変わるとローカル予約を読み直す(サーバー予約は読み直さない)", async () => {
    let listed = 0;
    mockCommands({
      list_scheduled_notes: () => [note("s1")],
      list_local_scheduled_notes: () => (listed++ === 0 ? [local("l1", "pending")] : [local("l1", "failed", {}, "理由")]),
    });
    const { findByTestId, rerender, getByTestId } = render(ScheduledModal, props());
    await findByTestId("scheduled-item-l1");
    await rerender(props({ reloadToken: 1 }));
    await waitFor(() => expect(getByTestId("scheduled-local-status-l1").textContent).toContain("投稿に失敗"));
    const serverCalls = invokeMock.mock.calls.filter((c) => c[0] === "list_scheduled_notes").length;
    expect(serverCalls).toBe(1);
  });

  it("ローカルだけ取得に失敗したときも、エラーを出して空表示にはしない", async () => {
    mockCommands({
      list_local_scheduled_notes: () => {
        throw { kind: "db", message: "broken" };
      },
    });
    const { findByText, findByTestId, queryByTestId } = render(ScheduledModal, props({ serverSide: false }));
    expect(await findByText(/broken/)).toBeTruthy();
    expect(await findByTestId("scheduled-retry")).toBeTruthy();
    expect(queryByTestId("scheduled-empty")).toBeNull();
  });
});
```

Run: `cd frontend && npx vitest run src/ui/ScheduledModal.test.ts`
Expected: FAIL(モーダルがまだ新しい props・command に対応していない)

- [ ] **Step 2: モーダルを実装する**

`frontend/src/ui/ScheduledModal.svelte` を次の内容にする。

```svelte
<script lang="ts">
  import { untrack } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import Modal from "./Modal.svelte";
  import { commands, unwrapAcc } from "../lib/ipc";
  import type { LocalScheduledNote, ScheduledNote } from "../bindings/tauri.gen";
  import { LOCAL_SCHEDULE_NOTICE, localStatusLabel, type ScheduledListItem } from "../lib/scheduledList";

  let {
    accountId,
    serverSide,
    reloadToken,
    onrestore,
    onclose,
  }: {
    accountId: string;
    /// サーバー予約(notes/drafts)も一覧に含めるか。含めないアカウントでも、ローカル予約は常に出す。
    serverSide: boolean;
    /// 値が変わったらローカル予約を読み直す(予約の投稿・失敗・期限切れのイベントで増える)。
    reloadToken: number;
    /// 「作成欄に戻す」。呼び出し側(ComposeBar)が内容の読み込みと予約の削除を行う。
    onrestore: (item: ScheduledListItem) => void;
    onclose: () => void;
  } = $props();

  // list_scheduled_notes の1回の取得件数。これだけ返ったら続きがあるとみなす。
  const PAGE_SIZE = 30;

  let serverItems = $state<ScheduledNote[]>([]);
  let localItems = $state<LocalScheduledNote[]>([]);
  let serverLoading = $state(false);
  let localLoading = $state(true);
  let hasMore = $state(false);
  let serverErr = $state<string | null>(null);
  let localErr = $state<string | null>(null);
  let actionErr = $state<string | null>(null);
  // カーソルは表示順(予約日時の昇順)ではなくサーバーが返した順の最後の ID。
  let cursor: string | null = null;
  let seenToken = untrack(() => reloadToken);

  const rows = $derived<ScheduledListItem[]>(
    [
      ...serverItems.map((note): ScheduledListItem => ({ origin: "server", note })),
      ...localItems.map(
        (l): ScheduledListItem => ({ origin: "local", note: l.note, status: l.status, error: l.error }),
      ),
    ].sort((a, b) => a.note.scheduledAt - b.note.scheduledAt),
  );
  const err = $derived(serverErr ?? localErr ?? actionErr);
  const loadFailed = $derived(serverErr !== null || localErr !== null);

  async function loadServer() {
    if (!serverSide) return;
    serverLoading = true;
    serverErr = null;
    try {
      const page = await unwrapAcc(accountId, commands.listScheduledNotes(accountId, cursor, PAGE_SIZE));
      serverItems = [...serverItems, ...(page ?? [])];
      hasMore = (page?.length ?? 0) >= PAGE_SIZE;
      if (page && page.length > 0) cursor = page[page.length - 1].id;
    } catch (e) {
      serverErr = String(e);
    } finally {
      serverLoading = false;
    }
  }

  async function loadLocal() {
    localLoading = true;
    localErr = null;
    try {
      localItems = (await unwrapAcc(accountId, commands.listLocalScheduledNotes(accountId))) ?? [];
    } catch (e) {
      localErr = String(e);
    } finally {
      localLoading = false;
    }
  }

  function reloadAll() {
    serverItems = [];
    cursor = null;
    void loadServer();
    void loadLocal();
  }

  void loadServer();
  void loadLocal();

  // 予約の投稿・失敗・期限切れ(イベント)で、ローカル予約だけを読み直す。
  $effect(() => {
    const t = reloadToken;
    if (t === seenToken) return;
    seenToken = t;
    void loadLocal();
  });

  async function cancel(item: ScheduledListItem) {
    actionErr = null;
    try {
      if (item.origin === "server") {
        await unwrapAcc(accountId, commands.cancelScheduledNote(accountId, item.note.id));
        serverItems = serverItems.filter((n) => n.id !== item.note.id);
      } else {
        await unwrapAcc(accountId, commands.cancelLocalScheduledNote(accountId, item.note.id));
        localItems = localItems.filter((l) => l.note.id !== item.note.id);
      }
    } catch (e) {
      actionErr = String(e);
    }
  }

  async function runNow(item: ScheduledListItem) {
    actionErr = null;
    try {
      await unwrapAcc(accountId, commands.postLocalScheduledNow(accountId, item.note.id));
    } catch (e) {
      actionErr = String(e);
    } finally {
      await loadLocal();
    }
  }

  /// サーバー予約で、予約時刻を過ぎても残っている=サーバーが投稿に失敗したもの。
  const isServerFailed = (n: ScheduledNote) => n.scheduledAt * 1000 <= Date.now();
  const formatAt = (n: ScheduledNote) => new Date(n.scheduledAt * 1000).toLocaleString();
  const VISIBILITY_LABEL: Record<string, string> = {
    public: "公開",
    home: "ホーム",
    followers: "フォロワー",
    specified: "ダイレクト",
  };
</script>

<Modal title="予約済みの投稿" {onclose} width="520px" maxHeight="80vh">
  {#snippet children()}
    {#if err}
      <p class="mb-2 mt-0 whitespace-pre-wrap break-words text-sm text-destructive">{err}</p>
    {/if}
    {#if (serverLoading || localLoading) && rows.length === 0}
      <div class="py-3 text-sm text-muted-foreground">読み込み中…</div>
    {:else if loadFailed && rows.length === 0}
      <!-- 取得失敗を「予約なし」と誤解して二重に予約しないよう、空表示は出さず再読み込みだけ出す -->
      <div class="flex justify-center py-2">
        <Button type="button" variant="outline" size="sm" data-testid="scheduled-retry" onclick={reloadAll}>再読み込み</Button>
      </div>
    {:else if rows.length === 0}
      <div class="py-3 text-sm text-muted-foreground" data-testid="scheduled-empty">予約済みの投稿はありません</div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto">
        {#each rows as item (item.origin + ":" + item.note.id)}
          {@const n = item.note}
          {@const statusLabel = item.origin === "local" ? localStatusLabel(item.status) : null}
          <div class="border-b border-border py-2 last:border-b-0" data-testid={`scheduled-item-${n.id}`}>
            <div class="mb-1 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
              <span>{formatAt(n)}</span>
              <span>{VISIBILITY_LABEL[n.visibility] ?? n.visibility}</span>
              {#if item.origin === "server" && isServerFailed(n)}
                <span class="font-semibold text-destructive" data-testid={`scheduled-failed-${n.id}`}>投稿に失敗</span>
              {/if}
              {#if statusLabel}
                <span class="font-semibold text-destructive" data-testid={`scheduled-local-status-${n.id}`}>{statusLabel}</span>
              {/if}
            </div>
            <div class="mb-1.5 line-clamp-3 whitespace-pre-wrap break-words text-sm text-foreground">{n.text.trim() || "(本文なし)"}</div>
            {#if item.origin === "local" && item.status === "pending"}
              <div class="mb-1.5 text-xs text-muted-foreground" data-testid={`scheduled-local-notice-${n.id}`}>{LOCAL_SCHEDULE_NOTICE}</div>
            {/if}
            {#if item.origin === "local" && item.error}
              <div class="mb-1.5 whitespace-pre-wrap break-words text-xs text-destructive">{item.error}</div>
            {/if}
            {#if !(item.origin === "local" && item.status === "posting")}
              <div class="flex justify-end gap-1.5">
                {#if item.origin === "local" && (item.status === "expired" || item.status === "failed")}
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    data-testid={`scheduled-run-now-${n.id}`}
                    onclick={() => runNow(item)}
                  >今すぐ投稿</Button>
                {/if}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  data-testid={`scheduled-restore-${n.id}`}
                  onclick={() => onrestore(item)}
                >作成欄に戻す</Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  data-testid={`scheduled-cancel-${n.id}`}
                  onclick={() => cancel(item)}
                >取り消し</Button>
              </div>
            {/if}
          </div>
        {/each}
        {#if hasMore}
          <div class="flex justify-center py-2">
            <Button type="button" variant="ghost" size="sm" disabled={serverLoading} data-testid="scheduled-more" onclick={loadServer}
              >さらに読み込む</Button
            >
          </div>
        {/if}
      </div>
    {/if}
  {/snippet}
</Modal>
```

Run: `cd frontend && npx vitest run src/ui/ScheduledModal.test.ts`
Expected: PASS(約 19 件)

- [ ] **Step 3: ComposeBar の「作成欄に戻す」とモーダル呼び出しを直す(RED → GREEN)**

先に、`frontend/src/ui/ComposeBar.test.ts` の「作成欄に戻す」の describe 内(`openScheduledList` ヘルパの近く)に、ローカル予約のテストを足す。

```ts
  const localScheduled = (over: Record<string, unknown> = {}) => ({
    note: scheduledNote(over),
    status: "pending",
    error: null,
  });

  it("ローカル予約の「作成欄に戻す」は、内容を読み込んで cancel_local_scheduled_note で削除する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: false });
      if (cmd === "list_local_scheduled_notes") return Promise.resolve([localScheduled({ id: "l1" })]);
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-l1"));

    await waitFor(() =>
      expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文"),
    );
    expect(invokeMock).toHaveBeenCalledWith("cancel_local_scheduled_note", { accountId: "acc1", id: "l1" });
    expect(invokeMock).not.toHaveBeenCalledWith("cancel_scheduled_note", expect.anything());
    // 未来の予約なので、予約日時も復元される
    expect(ui.getByTestId("compose-submit").textContent).toContain("予約");
  });

  it("ローカル予約を戻した後に削除が失敗したら、内容は残して重複の警告を出す", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: false });
      if (cmd === "list_local_scheduled_notes") return Promise.resolve([localScheduled({ id: "l1" })]);
      if (cmd === "cancel_local_scheduled_note") return Promise.reject({ kind: "network", message: "offline" });
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-l1"));
    expect(await ui.findByText(/重複/)).toBeTruthy();
    expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文");
  });
```

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "ローカル予約"`
Expected: FAIL(`restoreScheduled` が `ScheduledListItem` を受けず、`cancel_local_scheduled_note` を呼ばない)

`frontend/src/ui/ComposeBar.svelte`:

(a) 型 import: `ScheduledNote,` の型 import を、次の import に差し替える(`ScheduledNote` がこのファイルの他で使われていないことを `grep -n "ScheduledNote" frontend/src/ui/ComposeBar.svelte` で確認してから)。

```ts
  import type { ScheduledListItem } from "../lib/scheduledList";
```

(b) `restoreScheduled` を置き換える。

```ts
  /// 予約一覧の「作成欄に戻す」。内容を作成欄に読み込んでから、予約を削除する(サーバー予約は
  /// notes/drafts/delete、ローカル予約はストアから)。削除に失敗した場合、予約が残ったまま作成欄からも
  /// 投稿できてしまう(重複)ため警告する。
  async function restoreScheduled(item: ScheduledListItem) {
    if (!accountId) return;
    const acc = accountId;
    const s = item.note;
    await loadDraft({
      id: s.id,
      accountId: acc,
      kind: "auto",
      text: s.text,
      cw: s.cw,
      visibility: s.visibility,
      localOnly: s.localOnly,
      reactionAcceptance: s.reactionAcceptance,
      channelId: s.channelId,
      poll: s.poll,
      fileIds: s.fileIds,
      replyNote: s.replyNote,
      quoteNote: s.quoteNote,
      createdAt: 0,
      updatedAt: 0,
    });
    if (s.scheduledAt * 1000 > Date.now()) {
      scheduleAt = epochSecToLocalInput(s.scheduledAt);
      showSchedulePicker = true;
    }
    showScheduledModal = false;
    try {
      if (item.origin === "server") await unwrapAcc(acc, commands.cancelScheduledNote(acc, s.id));
      else await unwrapAcc(acc, commands.cancelLocalScheduledNote(acc, s.id));
    } catch (e) {
      err = `作成欄には戻しましたが、予約を取り消せませんでした。このまま投稿すると重複します。予約一覧から取り消してください。\n${String(e)}`;
    }
  }
```
(`epochSecToLocalInput` の import が既存の `../lib/schedule` import に含まれていることを確認し、無ければ足す。)

(c) モーダルの呼び出しを置き換える。

```svelte
{#if showScheduledModal && accountId}
  {#key accountId}
    <ScheduledModal
      {accountId}
      serverSide={scheduleServerSide === true}
      reloadToken={app.scheduledPostTick}
      onrestore={restoreScheduled}
      onclose={() => (showScheduledModal = false)}
    />
  {/key}
{/if}
```

(d) 既存の A のテスト(「作成欄に戻す」で…)で、`list_scheduled_notes` を返すモックに `get_schedule_capabilities: { available: true }` が含まれていること、`restore` 後の削除が `cancel_scheduled_note` であることを確認する(変更が必要なのは、モックが `list_local_scheduled_notes` に null を返す場合だけで、モーダルは `?? []` で受けるので不要なはず。落ちたら、テスト側のモックに `list_local_scheduled_notes` を足す)。

- [ ] **Step 4: 全体を確認してコミット**

Run:
```bash
cd frontend
npx vitest run src/ui/ScheduledModal.test.ts src/ui/ComposeBar.test.ts src/lib/scheduledList.test.ts
pnpm check 2>&1 | tail -2
pnpm test 2>&1 | tail -5
```
Expected: すべて PASS、`pnpm check` は 0 errors / 1 warning(既存)。

```bash
git add frontend/src/ui/ScheduledModal.svelte frontend/src/ui/ScheduledModal.test.ts frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: 予約一覧にローカル予約を統合し、今すぐ投稿と作成欄に戻すを対応する(#60 B)"
```

---

### Task 8: E2E シナリオ 1(ローカル予約の投稿・取り消し・作成欄に戻す)

**Files:**
- Modify: `e2e/helpers/misskeyApi.ts`(`listUserNotes`)
- Create: `e2e/helpers/scheduledUi.ts`(予約 UI の操作 helper。Task 9 も使う)
- Create: `e2e/wdio.scheduled.conf.ts`
- Create: `e2e/specs-scheduled/local-scheduled-post.e2e.ts`
- Modify: `e2e/package.json`(`e2e:scheduled`)

**Interfaces:**
- Consumes: Task 4 の `TSUMUGI_DEBUG_SERVER_VERSION`、Task 6・7 の testid、既存の `startMiauthBridge`・`addAccountViaUi`・`setWindow`・`until`・`signInAsSeededUser`・`getUserId`
- Produces: `listUserNotes(token, userId, limit?)`、`scheduledUi` の `openPicker` / `scheduleAt` / `openList` / `listRows` / `rowId` / `listText`(Task 9 が使う)、`pnpm e2e:scheduled`

- [ ] **Step 1: 前提を確認する(コードは変えない)**

```bash
cd e2e && sed -n 1,30p scripts/run-app.sh | head -30; grep -n "TSUMUGI_DEBUG_SERVER_VERSION" -r . ../src-tauri/src | head
```
`run-app.sh` が環境変数をアプリまで引き継ぐこと(`env -i` を使っていない。`exec xvfb-run`/`exec "$0"` は環境を引き継ぐ)を、コメントとコードで確認して報告する。引き継がれないと分かった場合は、`run-app.sh` で `TSUMUGI_DEBUG_SERVER_VERSION` を `export` して渡す変更を、このタスクに含める。

アプリのデバッグバイナリとフロントをビルドしておく(`e2e/README.md` の「実行手順」)。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && (cd frontend && pnpm build) && cd src-tauri && cargo build
```

- [ ] **Step 2: helper を足す**

`e2e/helpers/misskeyApi.ts` の末尾に追加する。

```ts
/**
 * `users/notes` でユーザーの最新ノートを返す(予約投稿が実際に投稿されたかを、アプリの画面ではなく
 * Misskey 側の事実として確認するために使う)。
 */
export async function listUserNotes(
  token: string,
  userId: string,
  limit = 30,
): Promise<{ id: string; text: string | null }[]> {
  const res = await fetch(`${BASE_URL}/api/users/notes`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ i: token, userId, limit }),
  });
  if (!res.ok) {
    throw new Error(`listUserNotes: users/notes failed ${res.status}: ${await res.text()}`);
  }
  return (await res.json()) as { id: string; text: string | null }[];
}
```

`e2e/helpers/scheduledUi.ts`:

```ts
// 予約投稿(Issue #60)の E2E で使う、予約 UI の操作 helper。セレクタは実際のコンポーネント
// (frontend/src/ui/ComposeBar.svelte、ScheduledModal.svelte)の data-testid。
// 日時は、カレンダーの操作ではなく flatpickr のインスタンスへ直接入れる(カレンダーのクリックは
// E2E では不安定になりやすく、この E2E の目的は予約の動作であってピッカーの操作ではないため)。

/** 予約の日時入力を開く(開いていれば何もしない)。 */
export async function openPicker(): Promise<void> {
  const input = await $('[data-testid="compose-schedule-input"]');
  if (!(await input.isDisplayed().catch(() => false))) {
    const toggle = await $('[data-testid="compose-schedule-toggle"]');
    await toggle.waitForClickable({ timeout: 15000 });
    await toggle.click();
  }
  await (await $('[data-testid="compose-schedule-input"]')).waitForDisplayed({ timeout: 10000 });
}

/** 本文と予約日時(ミリ秒)を入れて「予約」を押し、作成欄が空になる(=成功する)まで待つ。 */
export async function scheduleAt(text: string, atMs: number): Promise<void> {
  await openPicker();
  await browser.execute((ms: number) => {
    const el = document.querySelector('[data-testid="compose-schedule-input"]') as unknown as {
      _flatpickr: { setDate(d: Date, triggerChange: boolean): void };
    };
    el._flatpickr.setDate(new Date(ms), true);
  }, atMs);
  const textarea = await $('[data-testid="compose-textarea"]');
  await textarea.setValue(text);
  const submit = await $('[data-testid="compose-submit"]');
  await submit.waitForClickable({ timeout: 15000 });
  await submit.click();
  await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === "", {
    timeout: 20000,
    interval: 300,
    timeoutMsg: `compose box did not clear after scheduling "${text}"`,
  });
}

/** 予約一覧を開く。 */
export async function openList(): Promise<void> {
  await openPicker();
  const btn = await $('[data-testid="compose-scheduled-list"]');
  await btn.waitForClickable({ timeout: 15000 });
  await btn.click();
  await (await $('[data-testid^="scheduled-item-"], [data-testid="scheduled-empty"]')).waitForDisplayed({ timeout: 15000 });
}

export interface ListRow {
  testid: string;
  text: string;
}

/** 予約一覧の行(testid と表示テキスト)。 */
export async function listRows(): Promise<ListRow[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('[data-testid^="scheduled-item-"]')).map((e) => ({
      testid: e.getAttribute("data-testid") ?? "",
      text: (e as HTMLElement).textContent ?? "",
    })),
  );
}

/** 行の testid(`scheduled-item-<id>`)から予約の ID を取り出す。 */
export const rowId = (testid: string): string => testid.replace("scheduled-item-", "");

/** 一覧のうち、本文に `needle` を含む行。 */
export async function rowContaining(needle: string): Promise<ListRow | undefined> {
  return (await listRows()).find((r) => r.text.includes(needle));
}

/** 予約日時に使う時刻(ミリ秒): 少なくとも `marginMs` 先の、次の分の 0 秒。予約は分単位のため。 */
export function nextMinuteAfter(marginMs: number): number {
  return Math.ceil((Date.now() + marginMs) / 60_000) * 60_000;
}
```

- [ ] **Step 3: config とスクリプトを足す**

`e2e/wdio.scheduled.conf.ts`:

```ts
// クライアント側予約(Issue #60 B)の E2E 用 wdio 設定。デバッグビルド限定の環境変数
// TSUMUGI_DEBUG_SERVER_VERSION は package.json のスクリプトで設定する(この設定を直接使う場合も、
// 同じ環境変数を付けること)。
import type { Options } from "@wdio/types";
import { config as base } from "./wdio.conf";

export const config: Options.Testrunner = {
  ...base,
  specs: ["./specs-scheduled/**/*.e2e.ts"],
};
```

`e2e/package.json` の `scripts` の `e2e:mobile` の次に追加する。

```json
    "e2e:scheduled": "TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0 NODE_EXTRA_CA_CERTS=./certs/ca.pem wdio run wdio.scheduled.conf.ts",
```

- [ ] **Step 4: spec を書く**

`e2e/specs-scheduled/local-scheduled-post.e2e.ts`:

```ts
// クライアント側予約(Issue #60 B)の E2E シナリオ 1: ローカル予約の投稿・取り消し・作成欄に戻す。
//
// E2E の Misskey(2026.7.0)はサーバー側予約に対応しているが、TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0
// (デバッグビルド限定)でアプリに「非対応サーバー」と判定させ、ローカル予約の経路を通す。投稿は本物の
// notes/create で行われるので、投稿されたかは Misskey の API(users/notes)で確認する。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountViaUi, setWindow, until } from "../helpers/appInspect";
import { getUserId, listUserNotes, signInAsSeededUser } from "../helpers/misskeyApi";
import { listRows, nextMinuteAfter, openList, openPicker, rowContaining, rowId, scheduleAt } from "../helpers/scheduledUi";

const ADMIN = "e2etestadmin";
const RUN = Date.now();
const MARK = `e2esched${RUN}`;
const P1 = `${MARK} P1 posts`;
const P2 = `${MARK} P2 cancelled`;
const P3 = `${MARK} P3 restored`;

describe("local scheduled posts (client-side fallback)", () => {
  let bridge: MiauthBridge;
  let token: string;
  let userId: string;
  let dueMs: number;

  const postedTexts = async () =>
    (await listUserNotes(token, userId, 50)).map((n) => n.text ?? "").filter((t) => t.includes(MARK));

  before(async function () {
    this.timeout(60000);
    token = await signInAsSeededUser();
    userId = await getUserId(token, ADMIN);
    bridge = await startMiauthBridge();
  });
  after(async () => {
    await bridge?.teardown();
  });

  it("adds an account; the schedule button is available and shows the local-only notice", async function () {
    this.timeout(120000);
    await addAccountViaUi(bridge, ADMIN);
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 20000 });
    await setWindow();
    await openPicker();
    const hint = await $('[data-testid="compose-schedule-local-hint"]');
    await hint.waitForDisplayed({ timeout: 15000 }); // 機能判定は非同期なので待つ
    expect(await hint.getText()).toContain("アプリを起動している間だけ");
  });

  it("schedules three posts for the same minute (P1: post, P2: cancel, P3: restore)", async function () {
    this.timeout(180000);
    // 3 件の予約操作の間に時刻が過ぎても、すべて未来になるよう余裕を持たせる(次の分の 0 秒)
    dueMs = nextMinuteAfter(45_000);
    await scheduleAt(P1, dueMs);
    await scheduleAt(P2, dueMs);
    await scheduleAt(P3, dueMs);

    // 予約しただけでは投稿されていない
    expect(await postedTexts()).toEqual([]);

    await openList();
    await until(listRows, (rows) => rows.length >= 3, 15000, "three scheduled rows");
    const rows = await listRows();
    for (const t of [P1, P2, P3]) expect(rows.some((r) => r.text.includes(t))).toBe(true);
    // ローカル予約の注意書きが出ている
    expect(rows.every((r) => r.text.includes("アプリを起動している間だけ"))).toBe(true);
  });

  it("cancels P2 and restores P3 into the compose box", async function () {
    this.timeout(60000);
    const p2 = await rowContaining(P2);
    expect(p2).toBeTruthy();
    await (await $(`[data-testid="scheduled-cancel-${rowId(p2!.testid)}"]`)).click();
    await browser.waitUntil(async () => (await rowContaining(P2)) === undefined, { timeout: 10000, timeoutMsg: "P2 stayed in the list" });

    const p3 = await rowContaining(P3);
    expect(p3).toBeTruthy();
    await (await $(`[data-testid="scheduled-restore-${rowId(p3!.testid)}"]`)).click();
    // 内容が作成欄に戻り、(未来の予約なので)予約日時も復元されて投稿ボタンは「予約」のまま
    await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === P3, {
      timeout: 10000,
      timeoutMsg: "P3 was not restored to the compose box",
    });
    expect(await (await $('[data-testid="compose-submit"]')).getText()).toContain("予約");
    // 作成欄に戻したので、予約としては残らない(ここでは送信しない)
  });

  it("posts exactly P1 at the scheduled time; P2 and P3 are never posted", async function () {
    this.timeout(300000);
    // 予約時刻 + スケジューラの最大スリープ(30 秒)+ 余裕
    const waitMs = Math.max(0, dueMs - Date.now()) + 90_000;
    const posted = await until(postedTexts, (t) => t.some((x) => x.includes("P1")), waitMs, "P1 to be posted");
    expect(posted.filter((t) => t.includes("P1"))).toHaveLength(1);
    expect(posted.some((t) => t.includes("P2"))).toBe(false);
    expect(posted.some((t) => t.includes("P3"))).toBe(false);

    // 余分に待っても、P2・P3 は投稿されない(取り消した/作成欄に戻した予約は残っていない)
    await browser.pause(35_000);
    const later = await postedTexts();
    expect(later.filter((t) => t.includes("P1"))).toHaveLength(1);
    expect(later.some((t) => t.includes("P2") || t.includes("P3"))).toBe(false);
  });

  it("removes P1 from the list once it is posted", async function () {
    this.timeout(60000);
    // 作成欄に戻した P3 の内容が残っているが、一覧を開くだけなので影響しない
    await openList();
    await browser.waitUntil(async () => (await rowContaining(P1)) === undefined, {
      timeout: 15000,
      timeoutMsg: "P1 stayed in the scheduled list after being posted",
    });
    expect(await rowContaining(P2)).toBeUndefined();
    expect(await rowContaining(P3)).toBeUndefined();
  });
});
```

- [ ] **Step 5: ローカルで実行する**

```bash
cd e2e
./scripts/gen-ca.sh
docker compose up -d --wait
pnpm seed
xvfb-run -a pnpm e2e:scheduled 2>&1 | tail -40
```
Expected: 5 件すべて PASS(所要時間は 3〜5 分)。落ちた場合は、`wdio-logs/` のログとスクリーンショットから原因を調べる。**アプリが「非対応」と判定していない**(注意書きが出ない)場合は、環境変数がアプリまで届いていない(Step 1)。

後始末(PID 指定のみ。`pkill` 禁止):
```bash
docker compose down -v
```

- [ ] **Step 6: コミット**

```bash
git add e2e/helpers/misskeyApi.ts e2e/helpers/scheduledUi.ts e2e/wdio.scheduled.conf.ts e2e/specs-scheduled e2e/package.json
git commit -m "test: クライアント側予約の投稿・取り消し・作成欄に戻すのE2Eを追加(#60 B)"
```

---

### Task 9: E2E シナリオ 2(再起動をまたぐ期限切れ・「今すぐ投稿」・`Posting` の復旧)と CI・README

**Files:**
- Modify: `e2e/helpers/sessionHooks.ts`(`runPreSession` に事前フックを追加)
- Create: `e2e/specs-scheduled-restart/1-setup.e2e.ts`
- Create: `e2e/specs-scheduled-restart/2-restart.e2e.ts`
- Modify: `e2e/package.json`(`e2e:scheduled-restart`)
- Modify: `.github/workflows/test.yml`(CI の `e2e` ジョブにステップを追加)
- Modify: `e2e/README.md`

**Interfaces:**
- Consumes: Task 8 の helper、`scripts/run-reuse-home.sh`、`runPreSession` の流儀、ストアの JSON のキー(`posts[].scheduledAt` / `status` / `input.text`)

- [ ] **Step 1: 事前フックを足す**

`e2e/helpers/sessionHooks.ts`:

(a) `settingsPath()` の次に追加する。

```ts
function scheduledPostsPath(): string {
  const home = readFileSync(process.env.E2E_REUSE_HOME_FILE as string, "utf-8").trim();
  return join(home, "config", "com.onodai.tsumugi", "scheduled_posts.json");
}
```

(b) `runPreSession` の末尾(最後の `if` ブロックの後)に追加する。

```ts
  if (dir === "specs-scheduled-restart" && name.startsWith("2-")) {
    // 直前のセッションのアプリが完全に止まるのを待つ(アプリ停止中でないと、終了時の書き込みと競合する)
    await new Promise((r) => setTimeout(r, 4000));
    // アプリ停止中に、クライアント側予約のストア(scheduled_posts.json)を直接書き換える。
    // キー名と状態の値は src-tauri/src/store/scheduled_post.rs(`ScheduledPostEntry`)と同じ。変えたら両方直すこと。
    //  - A: 猶予(5 分)より十分前の過去にして、「アプリが起動していない間に予約時刻を過ぎた」状況にする
    //  - B: status を posting にして、「送信中に終了した」状況にする
    const p = scheduledPostsPath();
    const json = JSON.parse(readFileSync(p, "utf-8")) as {
      posts: { scheduledAt: number; status: string; input: { text: string } }[];
    };
    const nowSec = Math.floor(Date.now() / 1000);
    for (const post of json.posts) {
      if (post.input.text.includes(st.s.textA)) post.scheduledAt = nowSec - 3600;
      if (post.input.text.includes(st.s.textB)) post.status = "posting";
    }
    writeFileSync(p, JSON.stringify(json));
    log(`scheduled_posts.json: A → expired time, B → posting (${json.posts.length} posts)`);
  }
```

- [ ] **Step 2: セッション 1 の spec を書く**

`e2e/specs-scheduled-restart/1-setup.e2e.ts`:

```ts
// クライアント側予約(Issue #60 B)の E2E シナリオ 2: セッション 1。アカウントを追加し、1 時間後の
// ローカル予約を 2 件作ってアプリを終了する。セッションの合間に scheduled_posts.json が書き換えられる
// (helpers/sessionHooks.ts の runPreSession)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountViaUi, setWindow } from "../helpers/appInspect";
import { saveState } from "../helpers/sessionHooks";
import { openPicker, scheduleAt } from "../helpers/scheduledUi";

const RUN = Date.now();
const MARK = `e2esr${RUN}`;
const TEXT_A = `${MARK} A expires`;
const TEXT_B = `${MARK} B posting`;

describe("local scheduled posts across a restart: session 1", () => {
  let bridge: MiauthBridge;
  before(async () => {
    bridge = await startMiauthBridge();
  });
  after(async () => {
    await bridge?.teardown();
  });

  it("adds an account and schedules two posts one hour ahead", async function () {
    this.timeout(180000);
    await addAccountViaUi(bridge, "e2etestadmin");
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 20000 });
    await setWindow();
    await openPicker();
    await (await $('[data-testid="compose-schedule-local-hint"]')).waitForDisplayed({ timeout: 15000 });

    const inAnHour = Date.now() + 3_600_000;
    await scheduleAt(TEXT_A, inAnHour);
    await scheduleAt(TEXT_B, inAnHour);
    saveState({ s: { MARK, textA: TEXT_A, textB: TEXT_B } });
  });
});
```

- [ ] **Step 3: セッション 2 の spec を書く**

`e2e/specs-scheduled-restart/2-restart.e2e.ts`:

```ts
// クライアント側予約(Issue #60 B)の E2E シナリオ 2: セッション 2(再起動後)。
// 事前フックで、A は「アプリが起動していない間に予約時刻を過ぎた(猶予超過)」、B は「送信中に終了した」
// 状態に書き換えてある。期待:
//  - A は自動では投稿されず「期限切れ」として一覧に残る。「今すぐ投稿」でちょうど 1 件投稿される。
//  - B は再送されず「投稿に失敗」(結果不明)になる。
import { until } from "../helpers/appInspect";
import { getUserId, listUserNotes, signInAsSeededUser } from "../helpers/misskeyApi";
import { loadState } from "../helpers/sessionHooks";
import { listRows, openList, rowContaining, rowId } from "../helpers/scheduledUi";

describe("local scheduled posts across a restart: session 2", () => {
  let token: string;
  let userId: string;
  let textA: string;
  let textB: string;
  let mark: string;

  const postedTexts = async () =>
    (await listUserNotes(token, userId, 50)).map((n) => n.text ?? "").filter((t) => t.includes(mark));

  before(async function () {
    this.timeout(60000);
    const st = loadState();
    ({ textA, textB, MARK: mark } = st.s);
    token = await signInAsSeededUser();
    userId = await getUserId(token, "e2etestadmin");
    // アカウントは再利用する一時 HOME に残っているので、再ログイン(MiAuth のブリッジ)は不要
  });

  it("shows A as expired (not posted) and B as failed with an unknown result (not re-sent)", async function () {
    this.timeout(120000);
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 30000 });
    // 起動直後にスケジューラが処理するので、少し待ってから一覧を開く
    await browser.pause(3000);
    await openList();

    const a = await until(() => rowContaining(textA), (r) => r !== undefined, 15000, "row A");
    const b = await until(() => rowContaining(textB), (r) => r !== undefined, 15000, "row B");
    const idA = rowId(a!.testid);
    const idB = rowId(b!.testid);
    expect(await (await $(`[data-testid="scheduled-local-status-${idA}"]`)).getText()).toContain("期限切れ");
    expect(await (await $(`[data-testid="scheduled-local-status-${idB}"]`)).getText()).toContain("投稿に失敗");
    expect(b!.text).toContain("投稿結果が不明");

    // Review Focus 2・3: どちらも Misskey には投稿されていない(期限切れは自動投稿せず、送信中だったものは再送しない)
    expect(await postedTexts()).toEqual([]);
    await browser.pause(35_000); // スケジューラの周期(最大 30 秒)を 1 回以上またぐ
    expect(await postedTexts()).toEqual([]);
  });

  it("posts A exactly once with 'post now', and B is still never sent", async function () {
    this.timeout(120000);
    const a = await rowContaining(textA);
    expect(a).toBeTruthy();
    await (await $(`[data-testid="scheduled-run-now-${rowId(a!.testid)}"]`)).click();

    const posted = await until(postedTexts, (t) => t.some((x) => x.includes("A expires")), 60000, "A to be posted");
    expect(posted.filter((t) => t.includes("A expires"))).toHaveLength(1);
    expect(posted.some((t) => t.includes("B posting"))).toBe(false);

    await browser.waitUntil(async () => (await rowContaining(textA)) === undefined, {
      timeout: 15000,
      timeoutMsg: "A stayed in the list after 'post now'",
    });
    // B は失敗として残ったまま(自動では消えず、再送もされない)
    expect(await rowContaining(textB)).toBeTruthy();
    expect((await listRows()).length).toBe(1);
  });
});
```

セッション 2 では MiAuth のブリッジを使わない。既存の再起動 spec(`specs-ng-restart/2-unmute.e2e.ts`)の冒頭と同じ扱いか確認し、違いがあれば既存に合わせる。

- [ ] **Step 4: スクリプト・CI・README**

`e2e/package.json` の `scripts` の `e2e:scheduled` の次に追加する。

```json
    "e2e:scheduled-restart": "TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0 scripts/run-reuse-home.sh scheduled-restart ./specs-scheduled-restart/1-setup.e2e.ts ./specs-scheduled-restart/2-restart.e2e.ts",
```

`.github/workflows/test.yml` の `Run mobile E2E tests` ステップの次(`Dump Misskey stack logs on failure` の前)に追加する。

```yaml
      - name: Run scheduled-post E2E tests
        if: needs.changes.outputs.code == 'true'
        working-directory: e2e
        run: xvfb-run -a pnpm e2e:scheduled

      - name: Run scheduled-post restart E2E tests
        if: needs.changes.outputs.code == 'true'
        working-directory: e2e
        run: xvfb-run -a pnpm e2e:scheduled-restart
```

`e2e/README.md` の「## 再起動をまたぐ・キャッシュDBを検査するシナリオ」の節の前に、次の節を追加する。

```markdown
## クライアント側の予約投稿(Issue #60 B)

予約に対応していないサーバーのアカウントでは、tsumugi が予約をローカル(`scheduled_posts.json`)に保持し、
起動している間に自分で投稿する。E2E の Misskey(2026.7.0)は予約に**対応している**ため、
**デバッグビルド限定**の環境変数 `TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0` で、アプリが認識する
サーバーバージョンだけを上書きし、ローカル予約の経路を通す(投稿は本物の `notes/create`)。
リリースビルドにはこの環境変数の分岐は含まれない(`src-tauri/src/commands/column.rs` の
`debug_server_version_override`)。`pnpm e2e` には含まれず、専用のスクリプトで実行する。

- `pnpm e2e:scheduled` — `specs-scheduled/`。3 件(P1・P2・P3)を同じ分に予約し、P2 は取り消し、P3 は作成欄に戻す。
  予約時刻に P1 だけが**ちょうど 1 件**投稿されること、取り消した/戻した予約は投稿されないことを、Misskey の API
  (`users/notes`)で確認する。待ち時間は 2 分前後。
- `pnpm e2e:scheduled-restart` — `specs-scheduled-restart/`(`scripts/run-reuse-home.sh` の再起動シナリオ)。
  セッションの合間に、`helpers/sessionHooks.ts` の `runPreSession` が `scheduled_posts.json` を**直接書き換える**
  (A: 予約時刻を猶予より前の過去にする、B: `status` を `posting` にする)。再起動後、A は自動投稿されず
  「期限切れ」として残り、B は再送されず「投稿に失敗」(結果不明)になり、A は「今すぐ投稿」でちょうど 1 件投稿
  されることを確認する。待ち時間は短い。ストアの JSON のキー名・状態の値を変えたら、このフックも直すこと
  (`src-tauri/src/store/scheduled_post.rs` と相互に参照している)。
```

- [ ] **Step 5: ローカルで実行する**

```bash
cd e2e
docker compose down -v 2>/dev/null; docker compose up -d --wait
pnpm seed
xvfb-run -a pnpm e2e:scheduled-restart 2>&1 | tail -40
docker compose down -v
```
Expected: 2 つの spec が PASS(合計 2〜3 分)。落ちた場合は `wdio-logs/` で原因を調べる。`pnpm e2e:scheduled`(Task 8)が引き続き通ることも、必要なら確認する(同じインスタンスで続けて流す場合は、`docker compose down -v` で作り直す)。

- [ ] **Step 6: コミット**

```bash
git add e2e/helpers/sessionHooks.ts e2e/specs-scheduled-restart e2e/package.json e2e/README.md .github/workflows/test.yml
git commit -m "test: クライアント側予約の再起動をまたぐE2EとCIステップを追加(#60 B)"
```

---

### Task 10: ドキュメントと全体の最終確認

**Files:**
- Modify: `docs/guide/user-guide.md`(予約投稿の箇条書き)
- Modify: `CLAUDE.md`(`src-tauri/src` の構成に `scheduler.rs`)

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の「予約投稿」の箇条書きを、次の内容に置き換える(元の文面の「サーバー側に保存」「非対応のサーバーではアイコンが表示されません」を直す)。

```markdown
- **予約投稿**: 投稿バーのカレンダー時計アイコンから、投稿する日時を指定して予約できます。日時を入れると投稿ボタンが「予約」に変わります。サーバーが予約に対応している場合(Misskey 2025.10.0 以降)は、予約はサーバー側に保存されるため、tsumugi を閉じていても指定した日時に投稿されます。対応していないサーバーでは、tsumugi が予約を保持して自分で投稿するため、**tsumugi を起動している間だけ**投稿されます(予約時にその旨が表示されます。Android でバックグラウンドに回って止まった場合も同じです)。起動していない間に予約時刻を過ぎた予約は、自動では投稿されず、予約一覧に「期限切れ」として残ります。「予約一覧」から、予約済みの投稿の確認・取り消し・「作成欄に戻す」(内容と日時を投稿バーに読み込んで予約を取り消す)ができ、ローカルの予約が期限切れ・失敗した場合は「今すぐ投稿」もできます。サーバー側予約が予約時刻に投稿に失敗した場合(返信先が削除された等)は、一覧に「投稿に失敗」と表示されて残ります。投票の締切を「期間指定」にした場合は、予約日時からの期間になります。
```

- [ ] **Step 2: `CLAUDE.md` に構成を足す**

`CLAUDE.md` の「### src-tauri/src layout」の `session/` の項目の次に追加する。

```markdown
- `scheduler.rs` — client-side scheduled posts (Issue #60 B, for servers without server-side scheduling): a resident task posts due entries from `store/scheduled_post.rs` via `create_note`, only while the app is running. Entries older than `MISSED_GRACE_SEC` are marked `Expired` and never auto-posted; a request that may have reached the server (timeout etc.) is never retried, to avoid duplicate posts. `TSUMUGI_DEBUG_SERVER_VERSION` (debug builds only) overrides the detected server version so the E2E can exercise this path (see `e2e/README.md`).
```

- [ ] **Step 3: 全体の自動検証**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"
cd ../frontend && pnpm test 2>&1 | tail -5 && pnpm check 2>&1 | tail -2 && pnpm build >/dev/null 2>&1; echo build=$?
cd .. && git status --short
```
Expected: すべて成功。`cargo test` が `tauri.gen.ts` を再生成しても差分が出ない(`git status` がクリーン)。E2E は Task 8・9 で実行済み。変更が E2E に影響しうるため、**最後にもう一度** `pnpm e2e:scheduled` と `pnpm e2e:scheduled-restart` を通して(`docker compose down -v` で作り直してから)、結果を報告する(既存の `pnpm e2e` / `pnpm e2e:mobile` は、ComposeBar の変更(予約ボタンが全アカウントで出る)の影響を受けないか、`xvfb-run -a pnpm e2e` を 1 回通して確認する。時間がかかるため、落ちたら原因を報告する)。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md CLAUDE.md
git commit -m "docs: クライアント側予約のユーザーガイドと構成を追記(#60 B)"
```
