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
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri_specta::Event as _;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 確実に届いていない。再試行してよい。
    Retry,
    /// 再試行しない。理由をユーザーに見せる。
    Fatal(String),
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

/// 1 回の送信を待つ上限。応答が返らない送信でスケジューラ全体(全アカウントの予約)が止まらないようにする。
pub const SEND_TIMEOUT: Duration = Duration::from_secs(60);

/// 1 件を送る。`Posting` への遷移に勝った場合だけ送る(None なら誰かが送信中、または存在しない)。
/// `allow_retry` のとき、再試行できる失敗は `Pending` に戻して None を返す。
/// 送信は `timeout` で打ち切る。時間切れは「届いたか分からない」ので再送せず、結果不明の `Failed` にする。
/// 再試行の待ち時間は、送信が終わった後の時刻(`clock()`)を基準にする。
async fn attempt(
    state: &AppState,
    id: &str,
    clock: &(dyn Fn() -> i64 + Sync),
    timeout: Duration,
    allow_retry: bool,
) -> Option<Outcome> {
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
        Ok(client) => {
            match tokio::time::timeout(timeout, create_note(&client, &to_note_draft(&entry.input))).await {
                Ok(r) => r,
                // `error.rs` の `From<reqwest::Error>` と同じ接頭辞にして、`classify` の timeout 経路に乗せる
                Err(_) => Err(Error::Network(format!("timeout: no response within {}s", timeout.as_secs()))),
            }
        }
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
                if let Err(err) = state.scheduled_posts.finish_retry(id, entry.attempts + 1, clock() + delay) {
                    log::warn!("scheduled post {id}: failed to schedule retry: {err}");
                }
                None
            }
            Failure::Retry => fail(format!("再試行しても投稿できませんでした: {e}")),
            Failure::Fatal(message) => fail(message),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Posted { account_id: String, id: String, note_id: String },
    Failed { account_id: String, id: String, message: String },
    Expired { account_id: String, id: String },
}

/// 期限が来た予約を処理する。猶予を超えていれば `Expired`、そうでなければ送信する。
/// `now` は処理を始めた時刻。1 件ごとの送信に時間がかかるため、予約ごとの判定には
/// 処理開始からの経過秒を足した時刻を使う(`process_due_with`)。
pub async fn process_due(state: &AppState, now: i64) -> Vec<Outcome> {
    let start = Instant::now();
    process_due_with(state, &|| now + start.elapsed().as_secs() as i64, SEND_TIMEOUT).await
}

/// `process_due` の本体。時計(`clock`)と送信のタイムアウトを引数に取る(テストで差し替える)。
/// 猶予の判定・再試行の基準は、予約 1 件ごとに時計を読み直した時刻を使う。
async fn process_due_with(state: &AppState, clock: &(dyn Fn() -> i64 + Sync), timeout: Duration) -> Vec<Outcome> {
    let mut out = Vec::new();
    for e in state.scheduled_posts.due(clock()) {
        if clock() - e.scheduled_at > MISSED_GRACE_SEC {
            if state.scheduled_posts.mark_expired(&e.id).unwrap_or(false) {
                out.push(Outcome::Expired { account_id: e.account_id.clone(), id: e.id.clone() });
            }
            continue;
        }
        if let Some(o) = attempt(state, &e.id, clock, timeout, true).await {
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
    attempt(state, id, &|| now, SEND_TIMEOUT, false)
        .await
        .ok_or_else(|| Error::Invalid("投稿処理中です".into()))
}

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

    // ---- 送信のタイムアウトと、予約 1 件ごとの時刻 ----

    /// Review Important 1: 応答が返らない送信はタイムアウトで打ち切り、結果不明の Failed にする(再送しない)。
    /// 打ち切った後の別の予約は処理が続き、その時点で猶予を過ぎていれば投稿せず Expired にする。
    #[tokio::test]
    async fn a_hanging_post_times_out_without_resending_and_later_entries_use_a_fresh_now() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(created_note()).set_delay(Duration::from_secs(3)))
            .mount(&mock)
            .await;
        let s = state();
        s.set_test_api_base(mock.uri());
        let first = s.scheduled_posts.add("acc1", NOW - 10, test_input("first"), 0).unwrap();
        let second = s.scheduled_posts.add("acc1", NOW - 5, test_input("second"), 0).unwrap();

        // タイムアウト(200ms)が過ぎたら、時計が猶予より大きく進む
        let timeout = Duration::from_millis(200);
        let start = std::time::Instant::now();
        let clock = move || if start.elapsed() >= timeout { NOW + 2 * MISSED_GRACE_SEC } else { NOW };
        let out = process_due_with(&s, &clock, timeout).await;

        assert_eq!(out.len(), 2, "{out:?}");
        let Outcome::Failed { id, message, .. } = &out[0] else { panic!("{out:?}") };
        assert_eq!(*id, first.id);
        assert!(message.contains("タイムアウト") && message.contains("投稿されたか確認"), "{message}");
        assert_eq!(out[1], Outcome::Expired { account_id: "acc1".into(), id: second.id.clone() });

        let got = s.scheduled_posts.get(&first.id).unwrap();
        assert_eq!((got.status, got.attempts), (LocalScheduleStatus::Failed, 0), "not scheduled for retry");
        assert_eq!(s.scheduled_posts.get(&second.id).unwrap().status, LocalScheduleStatus::Expired);
        // 再送もされず、2 件目も送られていない
        assert_eq!(mock.received_requests().await.unwrap().len(), 1);
        assert!(process_due_with(&s, &|| NOW + 4 * MISSED_GRACE_SEC, timeout).await.is_empty());
        assert_eq!(mock.received_requests().await.unwrap().len(), 1);
    }

    /// 再試行の待ち時間(`next_attempt_at`)は、送信が終わった後の時刻を基準にする。
    #[tokio::test]
    async fn a_retry_is_scheduled_relative_to_the_time_after_the_attempt() {
        let s = state();
        s.set_test_api_base("http://127.0.0.1:1".into()); // 接続拒否
        let e = s.scheduled_posts.add("acc1", NOW - 1, test_input("x"), 0).unwrap();
        let calls = std::sync::atomic::AtomicI64::new(0);
        let clock = || NOW + 10 * (calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1); // 呼ぶたびに進む
        let n = || calls.load(std::sync::atomic::Ordering::SeqCst);
        assert!(process_due_with(&s, &clock, SEND_TIMEOUT).await.is_empty());
        let got = s.scheduled_posts.get(&e.id).unwrap();
        // due 判定・猶予判定・再試行の基準で、時計は少なくとも 2 回以上読まれ、後の読み取りが基準になる
        assert!(n() >= 2);
        assert_eq!(got.next_attempt_at, Some(NOW + 10 * n() + RETRY_DELAYS_SEC[0]));
    }

    /// `process_due(state, now)` の公開シグネチャは、処理開始からの経過秒で `now` を進める時計に委譲する。
    #[tokio::test]
    async fn process_due_keeps_its_public_signature() {
        let mock = mock_create(200, created_note()).await;
        let s = state();
        s.set_test_api_base(mock.uri());
        s.scheduled_posts.add("acc1", NOW - 10, test_input("hello"), 0).unwrap();
        assert!(matches!(process_due(&s, NOW).await.as_slice(), [Outcome::Posted { .. }]));
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
