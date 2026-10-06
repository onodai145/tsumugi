# 予約投稿(サーバー側) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 本家 Misskey(2025.10.0〜)の予約投稿 API(`notes/drafts/*`)を使い、ComposeBar から日時を指定して予約でき、予約済みの投稿を専用モーダルで一覧・取り消し・作成欄に戻せるようにする(Issue #60 のサブプロジェクト A)。

**Architecture:** Rust 側は `api/drafts.rs`(HTTP ラッパ)・`domain/{schedule,scheduled}.rs`(型と機能判定)・`commands/scheduled.rs`(Tauri command)を追加するのみで、既存の型・シグネチャは変えない。サーバーバージョンによる機能判定は既存の `search_capabilities_for` と同じキャッシュ経路を共有する。フロントは ComposeBar の `submit()` を「予約日時があれば `schedule_note`、なければ `post_note`」に分岐させ、`ScheduledModal.svelte` を新設する。

**Tech Stack:** Rust(tauri v2、tauri-specta、reqwest、wiremock)、Svelte 5(runes)、Vitest + @testing-library/svelte。

**Spec:** `docs/superpowers/specs/2026-10-07-scheduled-post-design.md`

## Global Constraints

- 対象は本家 Misskey の `notes/drafts/{create,list,delete}` のみ。MisskeyIO 方式(`notes/create` の `scheduledAt`、`notes/scheduled/*`)は実装しない。
- 機能判定: サーバーバージョン **≥ 2025.10.0** で予約可能。バージョン不明・解析不能は不可(予約ボタンを出さない)。
- 予約日時は Rust の command 引数・`ScheduledNote.scheduled_at` では **epoch 秒**(既存の `Note.created_at`・`search_server_notes` の日時引数と同じ)。HTTP 境界(`api/drafts.rs`)でのみミリ秒に変換する。
- 取り消しは `notes/drafts/delete`。`notes/drafts/update` は使わない。
- 既存の型・関数シグネチャは変更しない(追加のみ)。例外: `computePollExpiresAt()` に省略可能な引数を足す(Task 5)、`search_capabilities_for` の内部実装を共通ヘルパ呼び出しに置き換える(Task 3、挙動は変えない)。
- コミットメッセージは件名のみ(本文なし)。末尾の Co-Authored-By トレーラは別途付与される。`--no-verify` は使わない。
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。`cd src-tauri && cargo test generates_frontend_bindings` で再生成し、差分をコミットする。
- 新規 UI の角丸・文字サイズ・アイコンサイズは `docs/design/style-guide.md` と既存 ComposeBar のクラスに合わせる(一回限りの `rounded-[Npx]` 等を作らない)。
- 実 UI の確認は Xvfb 越しで行い(`WAYLAND_DISPLAY` も unset、`dbus-run-session` 必須)、実アカウントへの予約作成の前にユーザーの了承を取る。起動した dev サーバーはタスク完了前に自分で kill する(PID 指定のみ、`pkill`/`killall` 禁止)。

## Review Focus

仕様が暗黙に含むが、各タスクの主テストでは踏まれにくい入力・状況。各行は括弧内のタスクのテストで固定する。

1. サーバーが返す予約下書きで、`text` が null・`poll`/`reply`/`renote`/`channelId` が無い・`reactionAcceptance` が null でも一覧がパースできる(Task 2)。
2. 一覧に `scheduledAt` が null の要素が混ざっても、その要素だけ捨てて全体は失敗させない(Task 2)。
3. ピッカーに入れた日時が、送信ボタンを押す時点では過去になっている(入力後に時間が経過した)場合は、サーバーに送らずエラー表示する(Task 4・5)。
4. 期間指定(「N 時間後」)の投票は、予約日時を基準に締切を計算する。そうしないと投稿直後に期限切れの投票になる(Task 5)。
5. 「作成欄に戻す」でサーバー側の削除だけ失敗した場合、作成欄の内容は残し、重複投稿の恐れを警告する(Task 6)。

---

### Task 0: 実装ブランチの準備

**Files:** なし(git のみ)

- [ ] **Step 1: ブランチ名を実装用に変更する**

spec/plan のコミットは未 push の `docs/issue-60-scheduled-post-spec` にある。実装も同じ PR に載せるため、ブランチ名だけ変える(履歴は変えない)。

```bash
git status --short            # 空であること
git branch --show-current     # docs/issue-60-scheduled-post-spec
git branch -m feat/issue-60-scheduled-post
git branch --show-current     # feat/issue-60-scheduled-post
```

---

### Task 1: 機能判定(`ScheduleCapabilities`)

**Files:**
- Create: `src-tauri/src/domain/schedule.rs`
- Modify: `src-tauri/src/domain/mod.rs`(`mod schedule;` と re-export を追加)

**Interfaces:**
- Produces: `crate::domain::ScheduleCapabilities { pub available: bool }`、`crate::domain::schedule_capabilities(version: Option<&str>) -> ScheduleCapabilities`(Task 3 が使う)

- [ ] **Step 1: 失敗するテストを書く(関数本体は未実装)**

`src-tauri/src/domain/schedule.rs` を作成する。

```rust
//! 予約投稿(Issue #60)で、接続先サーバーが対応するかの判定。

use super::search::parse_misskey_version;
use serde::Serialize;
use specta::Type;

/// 本家 Misskey で予約投稿(`notes/drafts` の `scheduledAt`)が入った最初のリリース。
/// 上流 PR #16577(2025-09-26 マージ)は 2025.10.0-alpha.0 から含まれ、2025.9.0 には含まれない。
const SCHEDULE_MIN_VERSION: (u32, u32, u32) = (2025, 10, 0);

/// 接続先サーバーが対応する予約投稿機能。フロントはこれを見て予約ボタンの出し分けをする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleCapabilities {
    /// 予約投稿が使えるか。
    pub available: bool,
}

/// バージョンから対応可否を決める。None・パース不能は安全側(非対応)に倒す。
pub fn schedule_capabilities(version: Option<&str>) -> ScheduleCapabilities {
    let _ = version;
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_from_2025_10_0() {
        assert!(!schedule_capabilities(Some("2025.9.0")).available);
        assert!(schedule_capabilities(Some("2025.10.0")).available);
        assert!(schedule_capabilities(Some("2025.10.0-alpha.0")).available);
        assert!(schedule_capabilities(Some("2026.9.1")).available);
        assert!(schedule_capabilities(Some("2026.10.0")).available);
    }

    #[test]
    fn misskeyio_fork_version_is_unavailable() {
        // MisskeyIO フォークの旧版は notes/drafts の予約方式を持たない
        assert!(!schedule_capabilities(Some("2025.4.1-io.12b-fb6fbea074")).available);
    }

    #[test]
    fn unknown_or_unparseable_version_is_unavailable() {
        assert!(!schedule_capabilities(None).available);
        for v in ["", "unknown", "2025", "2025.10", "v2025.10.0"] {
            assert!(!schedule_capabilities(Some(v)).available, "{v:?} should be unavailable");
        }
    }
}
```

`src-tauri/src/domain/mod.rs` の `mod reaction;` の次に `mod schedule;` を、`pub use reaction::...` の次に下の 1 行を追加する。

```rust
pub use schedule::{schedule_capabilities, ScheduleCapabilities};
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib domain::schedule`
Expected: FAIL(`not implemented` の panic)

- [ ] **Step 3: 最小実装**

`schedule_capabilities` の本体を置き換える。

```rust
pub fn schedule_capabilities(version: Option<&str>) -> ScheduleCapabilities {
    let available = version
        .and_then(parse_misskey_version)
        .is_some_and(|v| v >= SCHEDULE_MIN_VERSION);
    ScheduleCapabilities { available }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib domain::schedule`
Expected: PASS(3 tests)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/domain/schedule.rs src-tauri/src/domain/mod.rs
git commit -m "feat: 予約投稿のサーバー対応判定を追加(#60)"
```

---

### Task 2: `ScheduledNote` 型と予約投稿 API ラッパ(`api/drafts.rs`)

**Files:**
- Create: `src-tauri/src/domain/scheduled.rs`
- Create: `src-tauri/src/api/drafts.rs`
- Modify: `src-tauri/src/domain/mod.rs`(`mod scheduled;` と re-export)
- Modify: `src-tauri/src/api/mod.rs`(`pub mod drafts;`)

**Interfaces:**
- Consumes: `crate::api::notes::{NoteDraft, VisibilityInput, ReactionAcceptanceInput}`、`crate::store::draft::{PollDraftSnapshot, DraftNoteSnapshot}`、`MisskeyClient::post`(`api/client.rs`)
- Produces:
  - `crate::domain::ScheduledNote`(下記)
  - `pub async fn create_scheduled(client: &MisskeyClient, draft: &NoteDraft, scheduled_at_sec: i64) -> Result<ScheduledNote>`
  - `pub async fn list_scheduled(client: &MisskeyClient, until_id: Option<&str>, limit: u32) -> Result<Vec<ScheduledNote>>`
  - `pub async fn delete_draft(client: &MisskeyClient, draft_id: &str) -> Result<()>`

- [ ] **Step 1: ドメイン型を作る**

`src-tauri/src/domain/scheduled.rs`:

```rust
//! サーバー側の予約投稿(Issue #60)。本家 Misskey の `NoteDraft` のうち、予約中のもの。
//! 作成欄に戻せるよう、フィールドは `store::draft::Draft` と揃えている。

use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
use crate::store::draft::{DraftNoteSnapshot, PollDraftSnapshot};
use serde::Serialize;
use specta::Type;

// PartialEq は付けない(PollDraftSnapshot / DraftNoteSnapshot が持たないため。テストはフィールド単位で比較する)。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledNote {
    /// サーバー側の下書き ID(取り消しに使う)。
    pub id: String,
    /// 予約日時(epoch 秒)。現在より過去なら、投稿に失敗して残っているもの。
    #[specta(type = specta_typescript::Number)]
    pub scheduled_at: i64,
    pub text: String,
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    pub local_only: bool,
    pub reaction_acceptance: ReactionAcceptanceInput,
    pub channel_id: Option<String>,
    pub poll: Option<PollDraftSnapshot>,
    pub file_ids: Vec<String>,
    pub reply_note: Option<DraftNoteSnapshot>,
    pub quote_note: Option<DraftNoteSnapshot>,
}
```

`src-tauri/src/domain/mod.rs` に `mod scheduled;`(`mod schedule;` の次)と `pub use scheduled::ScheduledNote;`(`pub use schedule::...` の次)を追加する。

- [ ] **Step 2: 失敗するテストを書く(API ラッパは未実装)**

`src-tauri/src/api/mod.rs` に `pub mod drafts;` を追加し、`src-tauri/src/api/drafts.rs` を作る。まず関数を `unimplemented!()` にし、テストだけ先に書く。

```rust
//! 本家 Misskey(2025.10.0〜)の予約投稿 API(`notes/drafts/*`、Issue #60)。
//!
//! 予約投稿は「`scheduledAt` と `isActuallyScheduled: true` を付けた下書き」。
//! 時刻は HTTP ではミリ秒、tsumugi の内部(domain/commands)では秒で扱う。

use crate::api::client::MisskeyClient;
use crate::api::notes::{NoteDraft, ReactionAcceptanceInput, VisibilityInput};
use crate::domain::ScheduledNote;
use crate::error::{Error, Result};
use crate::store::draft::{DraftNoteSnapshot, PollDraftSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub async fn create_scheduled(
    _client: &MisskeyClient,
    _draft: &NoteDraft,
    _scheduled_at_sec: i64,
) -> Result<ScheduledNote> {
    unimplemented!()
}

pub async fn list_scheduled(
    _client: &MisskeyClient,
    _until_id: Option<&str>,
    _limit: u32,
) -> Result<Vec<ScheduledNote>> {
    unimplemented!()
}

pub async fn delete_draft(_client: &MisskeyClient, _draft_id: &str) -> Result<()> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::notes::{PollInput, VisibilityInput};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(mock: &MockServer) -> MisskeyClient {
        MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None)
    }

    /// 必須項目だけの最小の予約下書き(`text` null・`poll`/`reply` 等なし)。
    fn minimal_draft_json(id: &str, scheduled_at_ms: i64) -> serde_json::Value {
        json!({
            "id": id,
            "createdAt": "2026-10-07T00:00:00.000Z",
            "text": null,
            "cw": null,
            "userId": "u1",
            "replyId": null,
            "renoteId": null,
            "visibility": "public",
            "visibleUserIds": [],
            "fileIds": [],
            "hashtag": null,
            "poll": null,
            "channelId": null,
            "localOnly": false,
            "reactionAcceptance": null,
            "scheduledAt": scheduled_at_ms,
            "isActuallyScheduled": true
        })
    }

    #[tokio::test]
    async fn create_sends_schedule_fields_in_milliseconds() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/create"))
            .and(body_partial_json(json!({
                "text": "hi",
                "visibility": "home",
                "scheduledAt": 1_760_000_000_000_i64,
                "isActuallyScheduled": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "createdDraft": {
                    "id": "d1",
                    "createdAt": "2026-10-07T00:00:00.000Z",
                    "text": "hi",
                    "cw": null,
                    "userId": "u1",
                    "replyId": null,
                    "renoteId": null,
                    "visibility": "home",
                    "visibleUserIds": [],
                    "fileIds": [],
                    "hashtag": null,
                    "poll": null,
                    "channelId": null,
                    "localOnly": false,
                    "reactionAcceptance": null,
                    "scheduledAt": 1_760_000_000_000_i64,
                    "isActuallyScheduled": true
                }
            })))
            .mount(&mock)
            .await;

        let draft = NoteDraft {
            text: Some("hi".into()),
            visibility: VisibilityInput::Home,
            ..Default::default()
        };
        let created = create_scheduled(&client(&mock), &draft, 1_760_000_000).await.unwrap();
        assert_eq!(created.id, "d1");
        assert_eq!(created.scheduled_at, 1_760_000_000);
        assert_eq!(created.text, "hi");
    }

    #[tokio::test]
    async fn create_keeps_poll_in_request() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/create"))
            .and(body_partial_json(json!({
                "poll": { "choices": ["a", "b"], "multiple": false, "expiresAt": 1_760_100_000_000_i64 }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "createdDraft": minimal_draft_json("d2", 1_760_000_000_000)
            })))
            .mount(&mock)
            .await;

        let draft = NoteDraft {
            text: Some("vote".into()),
            poll: Some(PollInput {
                choices: vec!["a".into(), "b".into()],
                multiple: false,
                expires_at: Some(1_760_100_000_000),
            }),
            ..Default::default()
        };
        create_scheduled(&client(&mock), &draft, 1_760_000_000).await.unwrap();
    }

    #[tokio::test]
    async fn list_requests_scheduled_only_with_cursor() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/list"))
            .and(body_partial_json(json!({ "scheduled": true, "limit": 30, "untilId": "d9" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&mock)
            .await;
        let out = list_scheduled(&client(&mock), Some("d9"), 30).await.unwrap();
        assert!(out.is_empty());
    }

    /// Review Focus 1: 任意項目が null/欠落でもパースできる。
    #[tokio::test]
    async fn list_parses_minimal_draft_with_nulls() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([minimal_draft_json("d1", 1_760_000_000_000)])))
            .mount(&mock)
            .await;
        let out = list_scheduled(&client(&mock), None, 30).await.unwrap();
        assert_eq!(out.len(), 1);
        let n = &out[0];
        assert_eq!(n.text, "");
        assert!(n.cw.is_none() && n.poll.is_none() && n.channel_id.is_none());
        assert!(n.reply_note.is_none() && n.quote_note.is_none());
        assert_eq!(n.reaction_acceptance, ReactionAcceptanceInput::All);
        assert_eq!(n.scheduled_at, 1_760_000_000);
    }

    /// Review Focus 2: `scheduledAt` が null の要素だけ捨て、全体は失敗させない。
    #[tokio::test]
    async fn list_skips_entries_without_scheduled_at() {
        let mock = MockServer::start().await;
        let mut unscheduled = minimal_draft_json("d_plain", 0);
        unscheduled["scheduledAt"] = serde_json::Value::Null;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                unscheduled,
                minimal_draft_json("d_sched", 1_760_000_000_000)
            ])))
            .mount(&mock)
            .await;
        let out = list_scheduled(&client(&mock), None, 30).await.unwrap();
        assert_eq!(out.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["d_sched"]);
    }

    #[tokio::test]
    async fn list_maps_poll_reply_renote_and_files() {
        let mock = MockServer::start().await;
        let mut d = minimal_draft_json("d3", 1_760_000_000_500);
        d["text"] = json!("本文");
        d["cw"] = json!("注意");
        d["visibility"] = json!("followers");
        d["localOnly"] = json!(true);
        d["reactionAcceptance"] = json!("likeOnly");
        d["channelId"] = json!("ch1");
        d["fileIds"] = json!(["f1", "f2"]);
        d["poll"] = json!({
            "choices": ["x", "y"],
            "multiple": true,
            "expiresAt": "2026-10-08T00:00:00.000Z",
            "expiredAfter": null
        });
        d["replyId"] = json!("r1");
        d["reply"] = json!({ "id": "r1", "text": "返信先", "user": { "username": "alice" } });
        d["renoteId"] = json!("q1");
        d["renote"] = json!({ "id": "q1", "text": null, "user": { "username": "bob" } });
        Mock::given(method("POST"))
            .and(path("/notes/drafts/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([d])))
            .mount(&mock)
            .await;

        let n = list_scheduled(&client(&mock), None, 30).await.unwrap().remove(0);
        assert_eq!(n.scheduled_at, 1_760_000_000); // 500ms は切り捨て
        assert_eq!(n.text, "本文");
        assert_eq!(n.cw.as_deref(), Some("注意"));
        assert_eq!(n.visibility, VisibilityInput::Followers);
        assert!(n.local_only);
        assert_eq!(n.reaction_acceptance, ReactionAcceptanceInput::LikeOnly);
        assert_eq!(n.channel_id.as_deref(), Some("ch1"));
        assert_eq!(n.file_ids, vec!["f1", "f2"]);
        let poll = n.poll.unwrap();
        assert_eq!(poll.choices, vec!["x", "y"]);
        assert!(poll.multiple);
        // 2026-10-08T00:00:00Z のミリ秒
        assert_eq!(poll.expires_at, Some(1_791_417_600_000));
        let reply = n.reply_note.unwrap();
        assert_eq!((reply.id.as_str(), reply.username.as_str(), reply.text.as_deref()), ("r1", "alice", Some("返信先")));
        let quote = n.quote_note.unwrap();
        assert_eq!((quote.id.as_str(), quote.username.as_str(), quote.text), ("q1", "bob", None));
    }

    #[tokio::test]
    async fn delete_posts_draft_id_and_accepts_204() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/delete"))
            .and(body_partial_json(json!({ "draftId": "d1" })))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;
        delete_draft(&client(&mock), "d1").await.unwrap();
    }

    #[tokio::test]
    async fn api_error_code_reaches_the_caller() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/drafts/create"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {
                    "message": "You cannot create scheduled notes any more.",
                    "code": "TOO_MANY_SCHEDULED_NOTES",
                    "id": "22ae69eb-09e3-4541-a850-773cfa45e693"
                }
            })))
            .mount(&mock)
            .await;
        let draft = NoteDraft { text: Some("x".into()), ..Default::default() };
        let err = create_scheduled(&client(&mock), &draft, 1_760_000_000).await.unwrap_err();
        assert!(err.to_string().contains("TOO_MANY_SCHEDULED_NOTES"), "{err}");
    }
}
```

注: `2026-10-08T00:00:00Z` の epoch ミリ秒は `1_791_417_600_000`。テストが落ちたら `date -u -d '2026-10-08T00:00:00Z' +%s` で秒を確認して 1000 倍した値に直す(実装の誤りではなくテスト定数の誤りの可能性が高い)。

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib api::drafts`
Expected: FAIL(`not implemented` の panic。コンパイルエラーなら未使用 import の警告ではなく型の不一致を直す)

- [ ] **Step 4: 実装する**

`api/drafts.rs` の `unimplemented!()` の 3 関数を、次のコードで置き換える(`use` 群・`tests` モジュールは Step 2 のまま)。関数の前に生型と変換を足す。

```rust
/// `notes/drafts/create` のリクエスト。`NoteDraft`(通常の投稿と同じ項目)に予約項目を足す。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateScheduledBody<'a> {
    #[serde(flatten)]
    draft: &'a NoteDraft,
    /// ミリ秒。
    scheduled_at: i64,
    is_actually_scheduled: bool,
}

#[derive(Deserialize)]
struct CreatedDraft {
    #[serde(rename = "createdDraft")]
    created_draft: RawDraft,
}

/// `NoteDraft` のうち tsumugi が使う項目だけを受ける。サーバーが足す項目は無視する。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDraft {
    id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    cw: Option<String>,
    visibility: VisibilityInput,
    #[serde(default)]
    local_only: bool,
    #[serde(default)]
    reaction_acceptance: Option<ReactionAcceptanceInput>,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    poll: Option<RawDraftPoll>,
    #[serde(default)]
    file_ids: Vec<String>,
    #[serde(default)]
    reply: Option<RawContextNote>,
    #[serde(default)]
    renote: Option<RawContextNote>,
    /// ミリ秒。予約でない下書きでは null。
    #[serde(default)]
    scheduled_at: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDraftPoll {
    #[serde(default)]
    choices: Vec<String>,
    #[serde(default)]
    multiple: bool,
    #[serde(default)]
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawContextNote {
    id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    user: RawContextUser,
}

#[derive(Debug, Default, Deserialize)]
struct RawContextUser {
    #[serde(default)]
    username: String,
}

impl From<RawContextNote> for DraftNoteSnapshot {
    fn from(n: RawContextNote) -> Self {
        DraftNoteSnapshot { id: n.id, username: n.user.username, text: n.text }
    }
}

fn rfc3339_millis(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

impl RawDraft {
    /// `scheduledAt` を持たない(予約でない)下書きは None。
    fn into_scheduled(self) -> Option<ScheduledNote> {
        let scheduled_ms = self.scheduled_at?;
        Some(ScheduledNote {
            id: self.id,
            scheduled_at: (scheduled_ms / 1000.0).floor() as i64,
            text: self.text.unwrap_or_default(),
            cw: self.cw,
            visibility: self.visibility,
            local_only: self.local_only,
            reaction_acceptance: self.reaction_acceptance.unwrap_or_default(),
            channel_id: self.channel_id,
            poll: self.poll.map(|p| PollDraftSnapshot {
                choices: p.choices,
                multiple: p.multiple,
                expires_at: p.expires_at.as_deref().and_then(rfc3339_millis),
            }),
            file_ids: self.file_ids,
            reply_note: self.reply.map(Into::into),
            quote_note: self.renote.map(Into::into),
        })
    }
}

/// 予約投稿を作る。`scheduled_at_sec` は epoch 秒(HTTP へはミリ秒で送る)。
pub async fn create_scheduled(
    client: &MisskeyClient,
    draft: &NoteDraft,
    scheduled_at_sec: i64,
) -> Result<ScheduledNote> {
    let body = CreateScheduledBody {
        draft,
        scheduled_at: scheduled_at_sec * 1000,
        is_actually_scheduled: true,
    };
    let res: CreatedDraft = client.post("notes/drafts/create", &body).await?;
    res.created_draft
        .into_scheduled()
        .ok_or_else(|| Error::Api("notes/drafts/create: response has no scheduledAt".into()))
}

/// 予約中の投稿一覧(`scheduled: true`)。`until_id` より古い下書きから `limit` 件。
pub async fn list_scheduled(
    client: &MisskeyClient,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<ScheduledNote>> {
    let mut body = json!({ "scheduled": true, "limit": limit });
    if let Some(id) = until_id {
        body["untilId"] = json!(id);
    }
    let raws: Vec<RawDraft> = client.post("notes/drafts/list", &body).await?;
    Ok(raws.into_iter().filter_map(RawDraft::into_scheduled).collect())
}

/// 下書き(予約)の削除。予約の取り消しにも使う。
pub async fn delete_draft(client: &MisskeyClient, draft_id: &str) -> Result<()> {
    client.post("notes/drafts/delete", &json!({ "draftId": draft_id })).await
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib api::drafts`
Expected: PASS(8 tests)。あわせて `cargo build` が警告なしで通ること(`domain` は `#![allow(dead_code, unused_imports)]` だが `api/drafts.rs` の未使用 import は警告になるので、`Error` 等を使っていることを確認)。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/domain/scheduled.rs src-tauri/src/domain/mod.rs src-tauri/src/api/drafts.rs src-tauri/src/api/mod.rs
git commit -m "feat: 本家Misskeyの予約投稿APIラッパとScheduledNote型を追加(#60)"
```

---

### Task 3: Tauri command と TS バインディング

**Files:**
- Create: `src-tauri/src/commands/scheduled.rs`
- Modify: `src-tauri/src/commands/mod.rs`(`pub mod scheduled;`)
- Modify: `src-tauri/src/commands/column.rs`(`cached_server_version` を切り出す。`search_capabilities_for` は 1914 行付近)
- Modify: `src-tauri/src/lib.rs`(`specta_builder()` への登録と `generates_frontend_bindings` のアサーション)
- Modify: `frontend/src/bindings/tauri.gen.ts`(再生成のみ)
- Modify: `CLAUDE.md`(`commands/` のグループ列挙に `scheduled` を足す)

**Interfaces:**
- Consumes: Task 1 の `schedule_capabilities`、Task 2 の `create_scheduled` / `list_scheduled` / `delete_draft`
- Produces(TS 側 `commands.*`):
  - `getScheduleCapabilities(accountId: string): Result<ScheduleCapabilities>`
  - `scheduleNote(accountId: string, draft: NoteDraft, scheduledAt: number): Result<ScheduledNote>`(`scheduledAt` は epoch 秒)
  - `listScheduledNotes(accountId: string, untilId: string | null, limit: number): Result<ScheduledNote[]>`
  - `cancelScheduledNote(accountId: string, draftId: string): Result<null>`

- [ ] **Step 1: 失敗するバインディングのアサーションを足す**

`src-tauri/src/lib.rs` の `generates_frontend_bindings` 内、`getSearchCapabilities` のアサーションの直後に追加する。

```rust
        // 予約投稿(Issue #60)のコマンドと型
        assert!(ts.contains("scheduleNote"), "missing scheduleNote command");
        assert!(ts.contains("listScheduledNotes"), "missing listScheduledNotes command");
        assert!(ts.contains("cancelScheduledNote"), "missing cancelScheduledNote command");
        assert!(ts.contains("getScheduleCapabilities"), "missing getScheduleCapabilities command");
        assert!(
            ts.contains("scheduledAt: number") || ts.contains("scheduledAt:number"),
            "ScheduledNote.scheduled_at should export as number (camelCase)"
        );
```

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: FAIL(`missing scheduleNote command`)

- [ ] **Step 2: 共通ヘルパを切り出す(挙動は変えない)**

`src-tauri/src/commands/column.rs` の `search_capabilities_for` を次のように置き換える。

```rust
/// アカウントの接続先サーバーの Misskey バージョン。`AppState` にキャッシュし、未取得なら
/// `/api/meta` から取得する。取得失敗は None(呼び出し側で非対応扱い)で、失敗はキャッシュしない
/// ため次回また取りに行く。未登録アカウントだけはエラーを返す。
pub(crate) async fn cached_server_version(
    state: &AppState,
    account_id: &str,
) -> Result<Option<String>> {
    if let Some(v) = state.server_version(account_id) {
        return Ok(Some(v));
    }
    let client = state.client_for(account_id)?;
    match fetch_server_version(&client).await {
        Ok(Some(v)) => {
            state.set_server_version(account_id, v.clone());
            Ok(Some(v))
        }
        Ok(None) | Err(_) => Ok(None),
    }
}

/// アカウントの接続先サーバーが対応する検索機能。取得失敗は非対応扱い（日時欄を隠す側に倒す）。
async fn search_capabilities_for(state: &AppState, account_id: &str) -> Result<SearchCapabilities> {
    let version = cached_server_version(state, account_id).await?;
    Ok(search_capabilities(version.as_deref()))
}
```

- [ ] **Step 3: command を実装する**

`src-tauri/src/commands/scheduled.rs`:

```rust
//! 予約投稿(サーバー側、Issue #60)の Tauri command。

use crate::api::drafts::{create_scheduled, delete_draft, list_scheduled};
use crate::api::notes::NoteDraft;
use crate::commands::column::cached_server_version;
use crate::domain::{schedule_capabilities, ScheduleCapabilities, ScheduledNote};
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// アカウントの接続先サーバーが予約投稿に対応するか。取得失敗は非対応扱い。
#[tauri::command]
#[specta::specta]
pub async fn get_schedule_capabilities(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<ScheduleCapabilities> {
    let version = cached_server_version(&state, &account_id).await?;
    Ok(schedule_capabilities(version.as_deref()))
}

/// 予約投稿を作る。`scheduled_at` は epoch 秒。添付は呼び出し側でアップロード済みの `fileIds`。
#[tauri::command]
#[specta::specta]
pub async fn schedule_note(
    state: State<'_, AppState>,
    account_id: String,
    draft: NoteDraft,
    scheduled_at: u32,
) -> Result<ScheduledNote> {
    let client = state.client_for(&account_id)?;
    create_scheduled(&client, &draft, i64::from(scheduled_at)).await
}

/// 予約中の投稿一覧。`until_id` は前のページの最後の ID。`limit` は 1〜100 に丸める。
#[tauri::command]
#[specta::specta]
pub async fn list_scheduled_notes(
    state: State<'_, AppState>,
    account_id: String,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<ScheduledNote>> {
    let client = state.client_for(&account_id)?;
    list_scheduled(&client, until_id.as_deref(), limit.clamp(1, 100)).await
}

/// 予約の取り消し(サーバー側の下書きを削除する)。
#[tauri::command]
#[specta::specta]
pub async fn cancel_scheduled_note(
    state: State<'_, AppState>,
    account_id: String,
    draft_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_draft(&client, &draft_id).await
}
```

`src-tauri/src/commands/mod.rs` の `pub mod note;` の次に `pub mod scheduled;` を追加する。

`src-tauri/src/lib.rs` の `commands::column::search_server_notes,` の次に追加する。

```rust
            commands::scheduled::get_schedule_capabilities,
            commands::scheduled::schedule_note,
            commands::scheduled::list_scheduled_notes,
            commands::scheduled::cancel_scheduled_note,
```

- [ ] **Step 4: テストが通り、バインディングが再生成されることを確認する**

Run:
```bash
cd src-tauri && cargo test generates_frontend_bindings
cargo test search_capabilities
cargo test
cd .. && git diff --stat frontend/src/bindings/tauri.gen.ts
```
Expected: すべて PASS。`tauri.gen.ts` に `scheduleNote` / `listScheduledNotes` / `cancelScheduledNote` / `getScheduleCapabilities` / `ScheduledNote` / `ScheduleCapabilities` が増える。`scheduledAt: number` で出ること(BigInt になる場合は `domain/scheduled.rs` の `#[specta(type = ...)]` を確認)。

- [ ] **Step 5: `CLAUDE.md` を更新してコミット**

`CLAUDE.md` の `commands/` の行(`` `account`, `app`, `cache_backend`, `clip`, `column`, `draft`, `haptics`, `mute`, `note`, `sound`, `user` ``)の `note` と `sound` の間に `` `scheduled` `` を足す。

```bash
git add src-tauri/src/commands/scheduled.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts CLAUDE.md
git commit -m "feat: 予約投稿のTauri commandとバインディングを追加(#60)"
```

---

### Task 4: フロントの純粋関数(`lib/schedule.ts`)とストアのラッパ

**Files:**
- Create: `frontend/src/lib/schedule.ts`
- Create: `frontend/src/lib/schedule.test.ts`
- Modify: `frontend/src/lib/store.svelte.ts`(`postNote` の直後に `scheduleNote`、`getSearchCapabilities` の直後に `getScheduleCapabilities`)

**Interfaces:**
- Consumes: Task 3 の `commands.scheduleNote` / `commands.getScheduleCapabilities`
- Produces:
  - `localInputToEpochSec(value: string): number | null`
  - `epochSecToLocalInput(sec: number): string`
  - `validateSchedule(scheduledAtSec: number, nowMs: number, pollExpiresAtMs: number | null): { ok: true } | { ok: false; message: string }`
  - `scheduleErrorMessage(message: string): string`
  - `app.getScheduleCapabilities(accountId: string): Promise<ScheduleCapabilities>`(失敗時は `{ available: false }`)
  - `app.scheduleNote(accountId: string, draft: NoteDraft, scheduledAt: number): Promise<void>`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/schedule.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  epochSecToLocalInput,
  localInputToEpochSec,
  scheduleErrorMessage,
  validateSchedule,
} from "./schedule";

describe("localInputToEpochSec / epochSecToLocalInput", () => {
  it("datetime-local 文字列をローカルタイムゾーンの epoch 秒に変換する", () => {
    const sec = localInputToEpochSec("2026-10-08T09:30");
    expect(sec).toBe(Math.floor(new Date(2026, 9, 8, 9, 30).getTime() / 1000));
  });

  it("空文字・不正な文字列は null", () => {
    expect(localInputToEpochSec("")).toBeNull();
    expect(localInputToEpochSec("not a date")).toBeNull();
  });

  it("epoch 秒からローカル成分の datetime-local 文字列に往復できる", () => {
    const sec = Math.floor(new Date(2026, 0, 2, 3, 4).getTime() / 1000);
    expect(epochSecToLocalInput(sec)).toBe("2026-01-02T03:04");
    expect(localInputToEpochSec(epochSecToLocalInput(sec))).toBe(sec);
  });
});

describe("validateSchedule", () => {
  const now = Date.UTC(2026, 9, 7, 0, 0, 0);
  const sec = (offsetMs: number) => Math.floor((now + offsetMs) / 1000);

  it("未来の日時は ok", () => {
    expect(validateSchedule(sec(60_000), now, null)).toEqual({ ok: true });
  });

  // Review Focus 3: 入力後に時間が経って過去になった場合
  it("現在以前の日時は拒否する", () => {
    const r = validateSchedule(sec(-1000), now, null);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.message).toContain("現在より後");
    expect(validateSchedule(sec(0), now, null).ok).toBe(false);
  });

  it("投票の締切が予約日時以前なら拒否する", () => {
    const at = sec(3_600_000);
    const r = validateSchedule(at, now, at * 1000);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.message).toContain("投票の締切");
    expect(validateSchedule(at, now, at * 1000 - 1).ok).toBe(false);
    expect(validateSchedule(at, now, at * 1000 + 1).ok).toBe(true);
  });
});

describe("scheduleErrorMessage", () => {
  it("本家のエラーコードを日本語にする", () => {
    expect(
      scheduleErrorMessage("Error: api: notes/drafts/create: TOO_MANY_SCHEDULED_NOTES You cannot create scheduled notes any more."),
    ).toContain("上限");
    expect(scheduleErrorMessage("api: notes/drafts/create: SCHEDULED_AT_MUST_BE_IN_FUTURE x")).toContain("現在より後");
    expect(scheduleErrorMessage("api: notes/drafts/create: SCHEDULED_AT_REQUIRED x")).toContain("日時");
  });

  it("未知のエラーはそのまま返す", () => {
    expect(scheduleErrorMessage("Error: network error: boom")).toBe("Error: network error: boom");
  });
});
```

Run: `cd frontend && npx vitest run src/lib/schedule.test.ts`
Expected: FAIL(`./schedule` が見つからない)

- [ ] **Step 2: 実装する**

`frontend/src/lib/schedule.ts`:

```ts
// 予約投稿(Issue #60)の日時変換・検証・エラーメッセージ。Svelte に依存しない純粋関数。

/// `<input type="datetime-local">` の値(ローカルタイムゾーン)を epoch 秒にする。空・不正は null。
export function localInputToEpochSec(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/// epoch 秒を `<input type="datetime-local">` の値にする。toISOString()(UTC)ではなく
/// ローカル成分から組み立てる(でないとタイムゾーン分ずれる)。
export function epochSecToLocalInput(sec: number): string {
  const dt = new Date(sec * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${dt.getFullYear()}-${pad(dt.getMonth() + 1)}-${pad(dt.getDate())}T${pad(dt.getHours())}:${pad(dt.getMinutes())}`;
}

export type ScheduleCheck = { ok: true } | { ok: false; message: string };

/// 送信前の検証。`pollExpiresAtMs` は投票の締切(ms)、投票なし・無期限は null。
/// 締切が予約日時以前だと、投稿された瞬間に期限切れの投票になるため拒否する。
export function validateSchedule(
  scheduledAtSec: number,
  nowMs: number,
  pollExpiresAtMs: number | null,
): ScheduleCheck {
  const scheduledMs = scheduledAtSec * 1000;
  if (scheduledMs <= nowMs) {
    return { ok: false, message: "予約日時は現在より後にしてください" };
  }
  if (pollExpiresAtMs != null && pollExpiresAtMs <= scheduledMs) {
    return { ok: false, message: "投票の締切が予約日時以前です。締切を予約日時より後にしてください" };
  }
  return { ok: true };
}

/// 本家 Misskey の予約まわりのエラーコードを日本語にする。該当しなければ元の文字列を返す。
export function scheduleErrorMessage(message: string): string {
  if (message.includes("TOO_MANY_SCHEDULED_NOTES")) {
    return "予約できる投稿数の上限に達しています。予約一覧から取り消してください";
  }
  if (message.includes("SCHEDULED_AT_MUST_BE_IN_FUTURE")) {
    return "予約日時は現在より後にしてください";
  }
  if (message.includes("SCHEDULED_AT_REQUIRED")) {
    return "予約日時を指定してください";
  }
  return message;
}
```

- [ ] **Step 3: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/schedule.test.ts`
Expected: PASS

- [ ] **Step 4: ストアのラッパを追加する**

`frontend/src/lib/store.svelte.ts` の `getSearchCapabilities` の直後に追加する(`ScheduleCapabilities` 型は同ファイルの bindings import に足す)。

```ts
  /// 予約投稿(Issue #60)に対応するサーバーか。取得に失敗しても非対応(false)にし、
  /// ComposeBar を開くたびに誤ってエラーモーダルを出さないよう #logFailure は呼ばない。
  async getScheduleCapabilities(accountId: string): Promise<ScheduleCapabilities> {
    try {
      return (await unwrapAcc(accountId, commands.getScheduleCapabilities(accountId))) ?? { available: false };
    } catch {
      return { available: false };
    }
  }
```

`postNote` の直後に追加する(`draft` の型は `postNote` と同じ `NoteDraft`)。

```ts
  /// 予約投稿(Issue #60)。`scheduledAt` は epoch 秒。
  async scheduleNote(accountId: string, draft: NoteDraft, scheduledAt: number) {
    try {
      await unwrapAcc(accountId, commands.scheduleNote(accountId, draft, scheduledAt));
      this.#log("success", "予約しました");
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

- [ ] **Step 5: 型チェックとコミット**

Run: `cd frontend && pnpm check && npx vitest run src/lib/schedule.test.ts`
Expected: エラーなし(既存の診断で本変更と無関係のものがあれば、`git stash` で差分の有無を確かめて切り分ける)

```bash
git add frontend/src/lib/schedule.ts frontend/src/lib/schedule.test.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: 予約投稿の日時変換・検証・ストアのラッパを追加(#60)"
```

---

### Task 5: ComposeBar の予約設定と送信の分岐

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`
- Test: `frontend/src/ui/ComposeBar.test.ts`(末尾に `describe("ComposeBar 予約投稿", ...)` を追加)

**Interfaces:**
- Consumes: Task 4 の `localInputToEpochSec` / `validateSchedule` / `scheduleErrorMessage` / `app.getScheduleCapabilities` / `app.scheduleNote`
- Produces: ComposeBar の状態 `scheduleAt: string`(datetime-local)と `showScheduledModal: boolean`(Task 6 が ScheduledModal の表示に使う)。testid: `compose-schedule-toggle`、`compose-schedule-input`、`compose-schedule-clear`、`compose-scheduled-list`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の末尾に追加する(冒頭の import に `import { epochSecToLocalInput, localInputToEpochSec } from "../lib/schedule";` を足す)。

```ts
describe("ComposeBar 予約投稿", () => {
  function mockCaps(available: boolean) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available });
      if (cmd === "schedule_note") return Promise.resolve({ id: "d1" });
      return Promise.resolve(null);
    });
  }
  const futureInput = () => epochSecToLocalInput(Math.floor((Date.now() + 86_400_000) / 1000));

  it("非対応サーバーでは予約ボタンを出さない", async () => {
    mockCaps(false);
    const { queryByTestId } = render(ComposeBar);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_schedule_capabilities", { accountId: "acc1" }));
    expect(queryByTestId("compose-schedule-toggle")).toBeNull();
  });

  it("対応サーバーで日時を設定すると投稿ボタンが「予約」になり、schedule_note を呼ぶ(post_note は呼ばない)", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const value = futureInput();
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value } });
    expect(getByTestId("compose-submit").textContent).toContain("予約");

    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "あとで投稿" } });
    await fireEvent.click(getByTestId("compose-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note", {
        accountId: "acc1",
        draft: expect.objectContaining({ text: "あとで投稿" }),
        scheduledAt: localInputToEpochSec(value),
      }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("post_note", expect.anything());
    // 成功したら作成欄と予約日時がクリアされ、投稿ボタンも元に戻る
    await waitFor(() => expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe(""));
    expect(getByTestId("compose-submit").textContent).toContain("投稿");
  });

  // Review Focus 3: 入力後に時間が経って過去になった場合はサーバーに送らない
  it("過去の日時ではエラーを出し、schedule_note を呼ばない", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId, findByText } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const past = epochSecToLocalInput(Math.floor((Date.now() - 86_400_000) / 1000));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: past } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "x" } });
    await fireEvent.click(getByTestId("compose-submit"));
    expect(await findByText("予約日時は現在より後にしてください")).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
  });

  it("予約日時を解除すると通常の投稿に戻る", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: futureInput() } });
    await fireEvent.click(getByTestId("compose-schedule-clear"));
    expect(getByTestId("compose-submit").textContent).toContain("投稿");
  });

  it("サーバーの上限エラーは日本語で表示する", async () => {
    mockCaps(true);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "schedule_note")
        return Promise.reject({ kind: "api", message: "notes/drafts/create: TOO_MANY_SCHEDULED_NOTES x" });
      return Promise.resolve(cmd === "list_drafts" ? [] : null);
    });
    const { findByTestId, getByTestId, findByText } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: futureInput() } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "x" } });
    await fireEvent.click(getByTestId("compose-submit"));
    expect(await findByText(/予約できる投稿数の上限/)).toBeTruthy();
    // 失敗したので作成欄は残る
    expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("x");
  });
});
```

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "予約投稿"`
Expected: FAIL(`compose-schedule-toggle` が見つからない)

- [ ] **Step 2: ComposeBar を実装する**

`frontend/src/ui/ComposeBar.svelte` に次の変更を入れる。

(a) import:
- `import { FileText, ImagePlus, SmilePlus, X } from "@lucide/svelte";` → `import { CalendarClock, FileText, ImagePlus, SmilePlus, X } from "@lucide/svelte";`
- `import { shouldInterceptPaste } from "../lib/pasteIntent";` の次に `import { localInputToEpochSec, scheduleErrorMessage, validateSchedule } from "../lib/schedule";` を追加する。

(b) 状態(`let manualDrafts = $state<Draft[]>([]);` の近く):

```ts
  // 予約投稿(Issue #60)。scheduleAt は datetime-local 文字列(空なら通常の投稿)。
  let scheduleAvailable = $state(false);
  let scheduleAt = $state("");
  let showSchedulePicker = $state(false);
  let scheduleCapsGen = 0;
  $effect(() => {
    const id = accountId;
    const gen = ++scheduleCapsGen;
    scheduleAvailable = false;
    if (!id) return;
    // アカウント切り替え直後に古いアカウントの結果で上書きしないよう世代で弾く
    void app.getScheduleCapabilities(id).then((c) => {
      if (gen === scheduleCapsGen) scheduleAvailable = c?.available ?? false;
    });
  });
```

(c) `computePollExpiresAt` に基準時刻の引数を足す(期間指定の投票を、予約日時を基準に計算するため):

```ts
  function computePollExpiresAt(baseMs: number = Date.now()): number | null {
    if (pollExpiryMode === "at" && pollExpiresAt) return new Date(pollExpiresAt).getTime();
    if (pollExpiryMode === "after") return baseMs + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    return null;
  }
```

(d) `submit()`: `const expiresAt = computePollExpiresAt();` の行を次で置き換える。

```ts
    const scheduledAtSec = scheduleAt ? localInputToEpochSec(scheduleAt) : null;
    if (scheduleAt && scheduledAtSec === null) {
      err = "予約日時が不正です";
      return;
    }
    // 期間指定(「N 時間後」)の投票は、予約なら予約日時を基準にする(投稿直後に期限切れにしない)
    const expiresAt = computePollExpiresAt(scheduledAtSec !== null ? scheduledAtSec * 1000 : Date.now());
    if (scheduledAtSec !== null) {
      const check = validateSchedule(
        scheduledAtSec,
        Date.now(),
        usePoll && choices.length >= 2 ? expiresAt : null,
      );
      if (!check.ok) {
        err = check.message;
        return;
      }
    }
```

`await app.postNote(accountId, draft);` を次で置き換える。

```ts
      if (scheduledAtSec !== null) await app.scheduleNote(accountId, draft, scheduledAtSec);
      else await app.postNote(accountId, draft);
```

成功後のリセット(`quoteOf = undefined;` の次の行、`onPosted?.();` の前)に追加する。

```ts
      scheduleAt = "";
      showSchedulePicker = false;
```

`catch (e) { err = String(e); }` を次に置き換える(予約のときだけ日本語メッセージに変換)。

```ts
    } catch (e) {
      err = scheduledAtSec !== null ? scheduleErrorMessage(String(e)) : String(e);
    } finally {
```

(`finally` の中身は変えない。`scheduledAtSec` は `try` の外で宣言されているので catch から見える。)

(e) markup。下書きボタンの直前(`<Button ... title="下書き" ...>` の前、`<div class="flex flex-none flex-wrap items-center gap-1.5">` の中)に追加する。

```svelte
      {#if scheduleAvailable}
        <Button
          type="button"
          variant={scheduleAt ? "default" : "outline"}
          size="icon-sm"
          title="予約投稿"
          data-testid="compose-schedule-toggle"
          onclick={() => (showSchedulePicker = !showSchedulePicker)}
          disabled={busy || !accountId}
        ><CalendarClock size={16} class="size-4" /></Button>
        {#if showSchedulePicker}
          <input
            type="datetime-local"
            bind:value={scheduleAt}
            data-testid="compose-schedule-input"
            class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground"
          />
          {#if scheduleAt}
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              class="flex-none text-muted-foreground"
              title="予約を解除"
              data-testid="compose-schedule-clear"
              onclick={() => {
                scheduleAt = "";
                showSchedulePicker = false;
              }}
            ><X size={12} /></Button>
          {/if}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid="compose-scheduled-list"
            onclick={() => (showScheduledModal = true)}
          >予約一覧</Button>
        {/if}
      {/if}
```

投稿ボタンのラベルを変える。

```svelte
      <Button type="button" size="sm" disabled={busy} onclick={submit} data-testid="compose-submit">{busy ? "…" : scheduleAt ? "予約" : "投稿"}</Button>
```

(f) `showScheduledModal` の宣言を (b) の状態宣言に足す(Task 6 で使う。この Task ではまだ参照だけが先にあるため先に宣言する)。

```ts
  let showScheduledModal = $state(false);
```

- [ ] **Step 3: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts && pnpm check`
Expected: PASS(既存テスト含む。`get_schedule_capabilities` を返さない既存テストでは `null` が返り、予約ボタンは出ないだけ)

- [ ] **Step 4: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: ComposeBarに予約日時の設定と予約送信を追加(#60)"
```

---

### Task 6: 予約一覧モーダル(`ScheduledModal.svelte`)と作成欄に戻す

**Files:**
- Create: `frontend/src/ui/ScheduledModal.svelte`
- Create: `frontend/src/ui/ScheduledModal.test.ts`
- Modify: `frontend/src/ui/ComposeBar.svelte`(モーダルの表示と `restoreScheduled`)
- Test: `frontend/src/ui/ComposeBar.test.ts`(「作成欄に戻す」のテストを `describe("ComposeBar 予約投稿", ...)` に追加)

**Interfaces:**
- Consumes: Task 3 の `commands.listScheduledNotes` / `commands.cancelScheduledNote`、Task 5 の `showScheduledModal`
- Produces: `<ScheduledModal accountId={string} onrestore={(s: ScheduledNote) => void} onclose={() => void} />`。testid: `scheduled-item-<id>`、`scheduled-failed-<id>`、`scheduled-cancel-<id>`、`scheduled-restore-<id>`、`scheduled-more`、`scheduled-empty`

- [ ] **Step 1: モーダルの失敗するテストを書く**

`frontend/src/ui/ScheduledModal.test.ts`(モーダルはストアに依存しないため、`@tauri-apps/api/core` のモックだけでよい):

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { ScheduledNote } from "../bindings/tauri.gen";

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

beforeEach(() => invokeMock.mockReset());
afterEach(() => cleanup());

describe("ScheduledModal", () => {
  it("予約を予約日時の昇順で表示し、過去のものは「投稿に失敗」と明示する", async () => {
    const now = Math.floor(Date.now() / 1000);
    invokeMock.mockResolvedValue([
      note("later", { scheduledAt: now + 7200 }),
      note("failed", { scheduledAt: now - 600 }),
      note("soon", { scheduledAt: now + 600 }),
    ]);
    const { findByTestId, container, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
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
    invokeMock.mockResolvedValue([]);
    const { findByTestId } = render(ScheduledModal, { accountId: "acc1", onrestore: vi.fn(), onclose: vi.fn() });
    expect(await findByTestId("scheduled-empty")).toBeTruthy();
  });

  it("取り消しで cancel_scheduled_note を呼び、一覧から消す", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_scheduled_notes" ? Promise.resolve([note("a"), note("b")]) : Promise.resolve(null),
    );
    const { findByTestId, getByTestId, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    await waitFor(() => expect(queryByTestId("scheduled-item-a")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "a" });
    expect(getByTestId("scheduled-item-b")).toBeTruthy();
  });

  it("取り消しに失敗したら一覧に残し、エラーを表示する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_scheduled_notes") return Promise.resolve([note("a")]);
      return Promise.reject({ kind: "network", message: "offline" });
    });
    const { findByTestId, findByText, getByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-cancel-a"));
    expect(await findByText(/offline/)).toBeTruthy();
    expect(getByTestId("scheduled-item-a")).toBeTruthy();
  });

  it("「作成欄に戻す」で onrestore にその予約を渡す", async () => {
    const a = note("a");
    invokeMock.mockResolvedValue([a]);
    const onrestore = vi.fn();
    const { findByTestId } = render(ScheduledModal, { accountId: "acc1", onrestore, onclose: vi.fn() });
    await fireEvent.click(await findByTestId("scheduled-restore-a"));
    expect(onrestore).toHaveBeenCalledWith(a);
  });

  it("1ページ分(30件)返ったら「さらに読み込む」を出し、最後の ID をカーソルに続きを取る", async () => {
    const page1 = Array.from({ length: 30 }, (_, i) => note(`p1_${i}`));
    invokeMock.mockResolvedValueOnce(page1).mockResolvedValueOnce([note("p2_0")]);
    const { findByTestId, queryByTestId } = render(ScheduledModal, {
      accountId: "acc1",
      onrestore: vi.fn(),
      onclose: vi.fn(),
    });
    await fireEvent.click(await findByTestId("scheduled-more"));
    await findByTestId("scheduled-item-p2_0");
    expect(invokeMock).toHaveBeenLastCalledWith("list_scheduled_notes", {
      accountId: "acc1",
      untilId: "p1_29",
      limit: 30,
    });
    // 2ページ目は30件未満なので、もう「さらに読み込む」は出ない
    expect(queryByTestId("scheduled-more")).toBeNull();
  });
});
```

Run: `cd frontend && npx vitest run src/ui/ScheduledModal.test.ts`
Expected: FAIL(`./ScheduledModal.svelte` が見つからない)

- [ ] **Step 2: モーダルを実装する**

`frontend/src/ui/ScheduledModal.svelte`:

```svelte
<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import Modal from "./Modal.svelte";
  import { commands, unwrapAcc } from "../lib/ipc";
  import type { ScheduledNote } from "../bindings/tauri.gen";

  let {
    accountId,
    onrestore,
    onclose,
  }: {
    accountId: string;
    /// 「作成欄に戻す」。呼び出し側(ComposeBar)が内容の読み込みとサーバー側の予約削除を行う。
    onrestore: (s: ScheduledNote) => void;
    onclose: () => void;
  } = $props();

  // list_scheduled_notes の1回の取得件数。これだけ返ったら続きがあるとみなす。
  const PAGE_SIZE = 30;

  let items = $state<ScheduledNote[]>([]);
  let loading = $state(true);
  let hasMore = $state(false);
  let err = $state<string | null>(null);
  // カーソルは表示順(予約日時の昇順)ではなくサーバーが返した順の最後の ID。
  let cursor: string | null = null;

  const sorted = $derived([...items].sort((a, b) => a.scheduledAt - b.scheduledAt));

  async function load() {
    loading = true;
    try {
      const page = await unwrapAcc(accountId, commands.listScheduledNotes(accountId, cursor, PAGE_SIZE));
      items = [...items, ...page];
      hasMore = page.length >= PAGE_SIZE;
      if (page.length > 0) cursor = page[page.length - 1].id;
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }
  void load();

  async function cancel(id: string) {
    err = null;
    try {
      await unwrapAcc(accountId, commands.cancelScheduledNote(accountId, id));
      items = items.filter((n) => n.id !== id);
    } catch (e) {
      err = String(e);
    }
  }

  /// 予約時刻を過ぎても残っている=サーバーが投稿に失敗したもの。
  const isFailed = (n: ScheduledNote) => n.scheduledAt * 1000 <= Date.now();
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
    {#if loading && items.length === 0}
      <div class="py-3 text-sm text-muted-foreground">読み込み中…</div>
    {:else if items.length === 0}
      <div class="py-3 text-sm text-muted-foreground" data-testid="scheduled-empty">予約済みの投稿はありません</div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto">
        {#each sorted as n (n.id)}
          <div class="border-b border-border py-2 last:border-b-0" data-testid={`scheduled-item-${n.id}`}>
            <div class="mb-1 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
              <span>{formatAt(n)}</span>
              <span>{VISIBILITY_LABEL[n.visibility] ?? n.visibility}</span>
              {#if isFailed(n)}
                <span class="font-semibold text-destructive" data-testid={`scheduled-failed-${n.id}`}>投稿に失敗</span>
              {/if}
            </div>
            <div class="mb-1.5 line-clamp-3 whitespace-pre-wrap break-words text-sm text-foreground">{n.text.trim() || "(本文なし)"}</div>
            <div class="flex justify-end gap-1.5">
              <Button
                type="button"
                variant="outline"
                size="sm"
                data-testid={`scheduled-restore-${n.id}`}
                onclick={() => onrestore(n)}
              >作成欄に戻す</Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                data-testid={`scheduled-cancel-${n.id}`}
                onclick={() => cancel(n.id)}
              >取り消し</Button>
            </div>
          </div>
        {/each}
        {#if hasMore}
          <div class="flex justify-center py-2">
            <Button type="button" variant="ghost" size="sm" disabled={loading} data-testid="scheduled-more" onclick={load}
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
Expected: PASS(6 tests)。`text-destructive` クラスが無い場合は `docs/design/style-guide.md` と他コンポーネントのエラー表示クラスに合わせる(`grep -rn "text-destructive\|text-red" frontend/src/ui | head`)。

- [ ] **Step 3: ComposeBar 統合の失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の `describe("ComposeBar 予約投稿", ...)` 内に追加する。

```ts
  const scheduledNote = (over: Record<string, unknown> = {}) => ({
    id: "s1",
    scheduledAt: Math.floor(Date.now() / 1000) + 3600,
    text: "戻したい本文",
    cw: null,
    visibility: "home",
    localOnly: false,
    reactionAcceptance: "all",
    channelId: null,
    poll: null,
    fileIds: [],
    replyNote: null,
    quoteNote: null,
    ...over,
  });

  async function openScheduledList(ui: ReturnType<typeof render>) {
    await fireEvent.click(await ui.findByTestId("compose-schedule-toggle"));
    await fireEvent.click(ui.getByTestId("compose-scheduled-list"));
  }

  it("「作成欄に戻す」で内容を作成欄に読み込み、サーバー側の予約を削除する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "list_scheduled_notes") return Promise.resolve([scheduledNote()]);
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-s1"));

    await waitFor(() =>
      expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文"),
    );
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "s1" });
    // モーダルは閉じる
    expect(ui.queryByTestId("scheduled-item-s1")).toBeNull();
  });

  // Review Focus 5: 削除だけ失敗しても作成欄の内容は残し、重複投稿の恐れを警告する
  it("戻した後にサーバー側の削除が失敗したら、内容は残して重複の警告を出す", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "list_scheduled_notes") return Promise.resolve([scheduledNote()]);
      if (cmd === "cancel_scheduled_note") return Promise.reject({ kind: "network", message: "offline" });
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-s1"));

    expect(await ui.findByText(/重複/)).toBeTruthy();
    expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文");
  });
```

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "作成欄に戻す|削除が失敗"`
Expected: FAIL(モーダルがまだ ComposeBar に組み込まれていない)

- [ ] **Step 4: ComposeBar に組み込む**

`frontend/src/ui/ComposeBar.svelte`:

(a) import に追加:

```ts
  import ScheduledModal from "./ScheduledModal.svelte";
```

型 import(`DraftNoteSnapshot,` の次)に `ScheduledNote,` を追加する。

(b) `loadDraft` の直後に追加する。`loadDraft` は `Draft` を受けるため、`ScheduledNote` を `Draft` の形(`kind: "auto"` なので `loadedDraftId` は null のまま)に包んで再利用する。

```ts
  /// 予約一覧の「作成欄に戻す」。内容を作成欄に読み込んでから、サーバー側の予約を削除する。
  /// 削除に失敗した場合、予約が残ったまま作成欄からも投稿できてしまう(重複)ため警告する。
  async function restoreScheduled(s: ScheduledNote) {
    if (!accountId) return;
    const acc = accountId;
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
    showScheduledModal = false;
    try {
      await unwrapAcc(acc, commands.cancelScheduledNote(acc, s.id));
    } catch (e) {
      err = `作成欄には戻しましたが、サーバー側の予約を取り消せませんでした。このまま投稿すると重複します。予約一覧から取り消してください。\n${String(e)}`;
    }
  }
```

(c) markup。`{#if showDrivePicker && accountId}` ブロックの直前に追加する。

```svelte
{#if showScheduledModal && accountId}
  <ScheduledModal {accountId} onrestore={restoreScheduled} onclose={() => (showScheduledModal = false)} />
{/if}
```

- [ ] **Step 5: すべてのフロントテストと型チェックを通す**

Run: `cd frontend && npx vitest run src/ui/ScheduledModal.test.ts src/ui/ComposeBar.test.ts && pnpm test && pnpm check`
Expected: PASS(既存の ComposeBar テストが壊れていないこと)

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/ScheduledModal.svelte frontend/src/ui/ScheduledModal.test.ts frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: 予約一覧モーダルと作成欄に戻す機能を追加(#60)"
```

---

### Task 7: ドキュメント・spec の追従・実機確認

**Files:**
- Modify: `docs/guide/user-guide.md`(「## 投稿」の箇条書きに追加)
- Modify: `docs/superpowers/specs/2026-10-07-scheduled-post-design.md`(実装で確定した 3 点を反映)

- [ ] **Step 1: ユーザーガイドに追記する**

`docs/guide/user-guide.md` の「## 投稿」の最後の箇条書き(`**返信・引用**`)の次に追加する。

```markdown
- **予約投稿**: 投稿バーのカレンダー時計アイコンから、投稿する日時を指定して予約できます(Misskey 2025.10.0 以降のサーバーのみ。非対応のサーバーではアイコンが表示されません)。日時を入れると投稿ボタンが「予約」に変わります。予約はサーバー側に保存されるため、tsumugi を閉じていても指定した日時に投稿されます。「予約一覧」から、予約済みの投稿の確認・取り消し・「作成欄に戻す」(内容を投稿バーに読み込んで予約を取り消す)ができます。予約時刻になっても投稿に失敗した場合(返信先が削除された等)は、一覧に「投稿に失敗」と表示されて残ります。投票の締切を「期間指定」にした場合は、予約日時からの期間になります。
```

- [ ] **Step 2: spec を実装に合わせて更新する**

`docs/superpowers/specs/2026-10-07-scheduled-post-design.md` に次の 3 点を反映する。

1. 「Rust」節の `create_scheduled` の説明に「Rust の command 引数と `ScheduledNote.scheduled_at` は epoch 秒(既存の `Note.created_at`・`search_server_notes` と同じ)、HTTP 境界でミリ秒に変換する」を足す。調査結果の表(`scheduledAt`(ミリ秒の整数))はサーバー仕様なので変えない。
2. 「予約一覧」節の「アカウント切り替えは既存の `AccountSelect` を使う」を「ComposeBar で選択中のアカウントの予約を表示する(切り替えは ComposeBar 側で行う)」に直す。
3. 「予約の設定」節の「クライアント側の検証」に「投票の期間指定(N 時間後)は、予約日時を基準に締切を計算する」を足す。

- [ ] **Step 3: 全体の自動検証**

Run:
```bash
cd src-tauri && cargo test && cargo clippy -- -D warnings 2>&1 | tail -5
cd ../frontend && pnpm check && pnpm test
```
Expected: すべて成功。clippy が既存コードの警告で落ちる場合は、本変更の差分に起因するものだけを直す(`git stash` で確認)。

- [ ] **Step 4: 実機確認の方法を決め、了承を取る**

**ここで実装を止め、ユーザーに確認する。** 実機確認には実アカウント(`dev.misskey.omhnc.net`)で未来時刻の予約を 1 件作る操作が含まれる。方法の候補は次の 2 つで、どちらにするかもユーザーに決めてもらう。

- (a) Xvfb + `dbus-run-session` の隔離セッションで `cargo tauri dev` を起動する。隔離セッションにはユーザーのキーリング(アカウントのトークン)が無いので、dev インスタンスで MiAuth の再ログインが必要になる(ブラウザ操作を伴うため、ユーザーの協力が要る)。ユーザーの実画面・実データには触れない。
- (b) ユーザーが既に起動している `cargo tauri dev` に debug bridge(`curl --unix-socket`)で接続して確認する。ユーザーの実データ(`~/.cache/com.onodai.tsumugi/cache.db`)と実アカウントを使うため、このブランチのコードがホットリビルドで実行される点に注意する。

了承・方法の指定が出るまで次へ進まない。出なければ、実機確認を行っていない旨を PR 本文に明記する。

- [ ] **Step 5: 実機確認(了承後)**

Step 4 で決めた方法で行う。(a) の場合は、過去の取り決めどおり `WAYLAND_DISPLAY` を unset し `DISPLAY` は Xvfb のものにして、リポジトリルートから `dbus-run-session -- cargo tauri dev` を起動する(`src-tauri` の中ではなくリポジトリルート)。起動した Xvfb・`cargo tauri dev` は、確認後に `ps aux` で PID を特定して `kill <pid>` で止める(`pkill`/`killall` 禁止)。

確認項目:
1. dev インスタンスのアカウントで予約ボタンが出る。非対応(`misskey.omhnc.net` は 2026.9.1 なので対応、MisskeyIO 等の旧版があればそちらで非表示)。
2. 未来時刻で予約 → 作成欄がクリアされ、「予約しました」が出る。
3. 「予約一覧」に表示される(日時・公開範囲・本文)。
4. 「作成欄に戻す」で内容が戻り、一覧から消える。再度予約 → 「取り消し」で消える。
5. 確認用に作った予約が dev インスタンスに残っていないこと(一覧が空)。

起動した `cargo tauri dev` と Xvfb は、PID を `ps aux` で特定して `kill <pid>` で止める(`pkill`/`killall` 禁止)。

- [ ] **Step 6: コミットと PR**

```bash
git add docs/guide/user-guide.md docs/superpowers/specs/2026-10-07-scheduled-post-design.md
git commit -m "docs: 予約投稿のユーザーガイドを追加しspecを実装に合わせて更新(#60)"
git push -u origin feat/issue-60-scheduled-post
```

`gh pr create` はテンプレート(`.github/pull_request_template.md`)の構造に手で沿って書く。本文に `Fixes` は **書かない**(#60 は B(クライアント側フォールバック)が残るため、参照のみ: `Refs #60`)。影響範囲は「TSバインディング生成に影響あり」にチェックを入れる。PR 末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。push 後は CI を待たない。
