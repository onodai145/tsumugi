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
