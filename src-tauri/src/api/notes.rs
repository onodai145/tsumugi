//! ノート系 REST（timeline 取得・投稿・削除・リアクション）。

use crate::api::normalize::{RawNote, RawReactionUser};
use crate::api::MisskeyClient;
use crate::domain::{Note, ReactionUser, Translation, User, Visibility};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// 指定エンドポイントへ任意ボディを POST してノート配列を得る（timeline/list/search 共通）。
pub async fn fetch_notes(
    client: &MisskeyClient,
    endpoint: &str,
    body: &serde_json::Value,
) -> Result<Vec<Note>> {
    let raw: Vec<RawNote> = client.post(endpoint, body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// `notes/search` の検索条件（サーバーサイド検索、Issue #430）。日時はミリ秒の epoch。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchParams {
    pub query: String,
    pub user_id: Option<String>,
    /// ローカルは `"."`（Misskey の仕様）。
    pub host: Option<String>,
    pub since_date_ms: Option<u64>,
    pub until_date_ms: Option<u64>,
    pub until_id: Option<String>,
    pub limit: u32,
}

/// `notes/search` のリクエストボディ。`None` の条件はキーごと出さない
/// （対応していない古いサーバーに余計なパラメータを送らないため）。
pub fn build_search_body(p: &SearchParams) -> serde_json::Value {
    let mut body = json!({ "query": p.query, "limit": p.limit });
    if let Some(v) = &p.user_id {
        body["userId"] = json!(v);
    }
    if let Some(v) = &p.host {
        body["host"] = json!(v);
    }
    if let Some(v) = p.since_date_ms {
        body["sinceDate"] = json!(v);
    }
    if let Some(v) = p.until_date_ms {
        body["untilDate"] = json!(v);
    }
    if let Some(v) = &p.until_id {
        body["untilId"] = json!(v);
    }
    body
}

/// サーバーサイド検索（`notes/search`）。
pub async fn search_notes(client: &MisskeyClient, p: &SearchParams) -> Result<Vec<Note>> {
    fetch_notes(client, "notes/search", &build_search_body(p)).await
}

/// ノート翻訳（`notes/translate`、Issue #440）。結果なし（204、本文が空）は `Ok(None)`。
/// `client.post` は空ボディを `null` として読むので、戻り値を `Option` にすれば 204 を受けられる。
/// サーバーで翻訳が未設定だと 400 `UNAVAILABLE` で、`Error::Api` の detail にそのコードが入る。
pub async fn translate_note(
    client: &MisskeyClient,
    note_id: &str,
    target_lang: &str,
) -> Result<Option<Translation>> {
    client
        .post(
            "notes/translate",
            &json!({ "noteId": note_id, "targetLang": target_lang }),
        )
        .await
}


/// `notes/create` 等の入力。フロントの NoteDraft をそのまま受ける想定。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct NoteDraft {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<PollInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renote_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub local_only: bool,
    /// `All`（既定）はフィールドごと省略し、Misskey 側のデフォルト（`null`＝全員）に委ねる。
    #[serde(skip_serializing_if = "reaction_acceptance_is_default")]
    pub reaction_acceptance: Option<ReactionAcceptanceInput>,
}

fn reaction_acceptance_is_default(v: &Option<ReactionAcceptanceInput>) -> bool {
    matches!(v, None | Some(ReactionAcceptanceInput::All))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VisibilityInput {
    #[default]
    Public,
    Home,
    Followers,
    /// direct（Misskey では specified）
    Specified,
}

/// `notes/create` の `reactionAcceptance`。`All` は送信時 `null` 相当（フィールド省略）として扱う。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReactionAcceptanceInput {
    #[default]
    All,
    LikeOnly,
    LikeOnlyForRemote,
    NonSensitiveOnly,
    NonSensitiveOnlyForLocalLikeOnlyForRemote,
}

impl From<VisibilityInput> for Visibility {
    fn from(v: VisibilityInput) -> Self {
        match v {
            VisibilityInput::Public => Visibility::Public,
            VisibilityInput::Home => Visibility::Home,
            VisibilityInput::Followers => Visibility::Followers,
            VisibilityInput::Specified => Visibility::Specified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PollInput {
    pub choices: Vec<String>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<specta_typescript::Number>)]
    pub expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct CreatedNote {
    #[serde(rename = "createdNote")]
    created_note: RawNote,
}

/// 投稿（本文・CW・可視性・添付・投票・返信/引用/Renote）。Misskey `notes/create`。
pub async fn create_note(client: &MisskeyClient, draft: &NoteDraft) -> Result<Note> {
    let res: CreatedNote = client.post("notes/create", draft).await?;
    Ok(res.created_note.into())
}

/// 純粋 Renote（本文なし・renoteId のみ）。
pub async fn renote(
    client: &MisskeyClient,
    note_id: &str,
    visibility: VisibilityInput,
) -> Result<Note> {
    let draft = NoteDraft {
        renote_id: Some(note_id.to_string()),
        visibility,
        ..Default::default()
    };
    create_note(client, &draft).await
}

/// ノート削除。`notes/delete` は 204 を返す。
pub async fn delete_note(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/delete", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}

/// リアクション付与。`reaction` は Misskey 形式キー（Unicode生 or `:name@host:`）。
pub async fn create_reaction(client: &MisskeyClient, note_id: &str, reaction: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post(
            "notes/reactions/create",
            &json!({ "noteId": note_id, "reaction": reaction }),
        )
        .await?;
    Ok(())
}

/// リアクション解除。
pub async fn delete_reaction(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/reactions/delete", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}

/// リアクション付与ユーザー一覧取得。`reaction_type` を指定すると絵文字キーで絞り込む。最大100件。
pub async fn get_reactions(
    client: &MisskeyClient,
    note_id: &str,
    reaction_type: Option<&str>,
) -> Result<Vec<ReactionUser>> {
    let mut body = json!({ "noteId": note_id, "limit": 100 });
    if let Some(t) = reaction_type {
        body["type"] = json!(t);
    }
    let raw: Vec<RawReactionUser> = client.post("notes/reactions", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// Renoteしたユーザー一覧取得。最大100件。
/// `notes/renotes` は1 Renoteにつき1件返す（同一ユーザーが複数回Renoteした場合は重複しうる）ため、
/// `id` でユーザーを重複排除する（初出順を保持）。
pub async fn get_renotes(client: &MisskeyClient, note_id: &str) -> Result<Vec<User>> {
    let raw: Vec<RawNote> = client
        .post("notes/renotes", &json!({ "noteId": note_id, "limit": 100 }))
        .await?;
    Ok(dedupe_users_by_id(
        raw.into_iter().map(|n| n.user.into()).collect(),
    ))
}

/// ユーザー一覧を `id` で重複排除する。初出順を保持する。
fn dedupe_users_by_id(users: Vec<User>) -> Vec<User> {
    let mut seen = std::collections::HashSet::new();
    users
        .into_iter()
        .filter(|u| seen.insert(u.id.clone()))
        .collect()
}

/// お気に入り登録。`notes/favorites/create`。
pub async fn create_favorite(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/favorites/create", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}

/// お気に入り解除。`notes/favorites/delete`。
pub async fn delete_favorite(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/favorites/delete", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}

/// 投票（choice は 0-based index）。
pub async fn vote_poll(client: &MisskeyClient, note_id: &str, choice: u32) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/polls/vote", &json!({ "noteId": note_id, "choice": choice }))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_serializes_only_present_fields() {
        let d = NoteDraft {
            text: Some("hi".into()),
            visibility: VisibilityInput::Home,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["text"], "hi");
        assert_eq!(v["visibility"], "home");
        assert!(v.get("cw").is_none());
        assert!(v.get("replyId").is_none());
        assert!(v.get("renoteId").is_none());
        assert!(v.get("channelId").is_none());
        // 空の fileIds は送らない
        assert!(v.get("fileIds").is_none());
    }

    #[test]
    fn channel_post_serializes_channel_id() {
        let d = NoteDraft {
            text: Some("channel post".into()),
            channel_id: Some("ch1".into()),
            visibility: VisibilityInput::Public,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["text"], "channel post");
        assert_eq!(v["channelId"], "ch1");
    }

    #[test]
    fn quote_has_text_and_renote_id() {
        let d = NoteDraft {
            text: Some("nice".into()),
            renote_id: Some("n1".into()),
            visibility: VisibilityInput::Public,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["text"], "nice");
        assert_eq!(v["renoteId"], "n1");
        assert_eq!(v["visibility"], "public");
    }

    #[test]
    fn reaction_acceptance_omitted_by_default() {
        let d = NoteDraft {
            text: Some("hi".into()),
            visibility: VisibilityInput::Public,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert!(v.get("reactionAcceptance").is_none());
    }

    #[test]
    fn reaction_acceptance_serializes_non_default_choice() {
        let d = NoteDraft {
            text: Some("hi".into()),
            visibility: VisibilityInput::Public,
            reaction_acceptance: Some(ReactionAcceptanceInput::LikeOnly),
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["reactionAcceptance"], "likeOnly");
    }

    #[test]
    fn poll_input_serializes() {
        let d = NoteDraft {
            text: Some("q".into()),
            visibility: VisibilityInput::Public,
            poll: Some(PollInput {
                choices: vec!["a".into(), "b".into()],
                multiple: true,
                expires_at: None,
            }),
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["poll"]["choices"][0], "a");
        assert_eq!(v["poll"]["multiple"], true);
        assert!(v["poll"].get("expiresAt").is_none());
    }

    #[test]
    fn rest_request_builds_bodies() {
        use crate::domain::ColumnKind;
        let (ep, body) = ColumnKind::Home.rest_request(20, None).unwrap();
        assert_eq!(ep, "notes/timeline");
        assert_eq!(body["limit"], 20);
        assert!(body.get("untilId").is_none());

        let (ep, body) = ColumnKind::List { list_id: "L1".into() }.rest_request(20, Some("n9")).unwrap();
        assert_eq!(ep, "notes/user-list-timeline");
        assert_eq!(body["listId"], "L1");
        assert_eq!(body["untilId"], "n9");

        let (ep, body) = ColumnKind::Search { query: "rust".into() }.rest_request(20, None).unwrap();
        assert_eq!(ep, "notes/search");
        assert_eq!(body["query"], "rust");

        // Search はストリーミング無し
        assert!(ColumnKind::Search { query: "x".into() }.stream_request().is_none());
        assert_eq!(ColumnKind::List { list_id: "L".into() }.stream_request().unwrap().0, "userList");
    }

    fn make_user(id: &str, username: &str) -> User {
        User {
            id: id.into(),
            username: username.into(),
            host: None,
            name: None,
            avatar_url: None,
            is_bot: false,
            is_cat: false,
            followers_count: 0,
            following_count: 0,
            notes_count: 0,
            emojis: std::collections::HashMap::new(),
            bio: None,
            banner_url: None,
            avatar_blurhash: None,
            instance: None,
        }
    }

    #[test]
    fn dedupe_users_by_id_preserves_first_occurrence_order() {
        let users = vec![
            make_user("u1", "alice"),
            make_user("u2", "bob"),
            make_user("u1", "alice-dup"),
        ];
        let deduped = dedupe_users_by_id(users);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].id, "u1");
        assert_eq!(deduped[0].username, "alice");
        assert_eq!(deduped[1].id, "u2");
        assert_eq!(deduped[1].username, "bob");
    }

    #[test]
    fn build_search_body_includes_every_given_condition() {
        let body = build_search_body(&SearchParams {
            query: "rust".into(),
            user_id: Some("u9".into()),
            host: Some("example.com".into()),
            since_date_ms: Some(1_700_000_000_000),
            until_date_ms: Some(1_800_000_000_000),
            until_id: Some("n5".into()),
            limit: 20,
        });
        assert_eq!(
            body,
            json!({
                "query": "rust",
                "limit": 20,
                "userId": "u9",
                "host": "example.com",
                "sinceDate": 1_700_000_000_000u64,
                "untilDate": 1_800_000_000_000u64,
                "untilId": "n5",
            })
        );
    }

    #[test]
    fn build_search_body_omits_conditions_that_are_none() {
        let body = build_search_body(&SearchParams { query: "x".into(), limit: 10, ..Default::default() });
        assert_eq!(body, json!({ "query": "x", "limit": 10 }));
    }

    fn mock_client(uri: String) -> MisskeyClient {
        MisskeyClient::new_with_api_base(reqwest::Client::new(), uri, None)
    }

    #[tokio::test]
    async fn translate_note_parses_200() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .and(body_partial_json(json!({ "noteId": "n1", "targetLang": "ja" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sourceLang": "EN",
                "text": "こんにちは"
            })))
            .mount(&mock)
            .await;

        let t = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap();
        assert_eq!(
            t,
            Some(crate::domain::Translation {
                source_lang: "EN".into(),
                text: "こんにちは".into()
            })
        );
    }

    #[tokio::test]
    async fn translate_note_returns_none_on_204() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;

        let t = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap();
        assert_eq!(t, None);
    }

    #[tokio::test]
    async fn translate_note_unavailable_is_api_error_with_code() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "code": "UNAVAILABLE", "message": "Translate of notes unavailable." }
            })))
            .mount(&mock)
            .await;

        let err = translate_note(&mock_client(mock.uri()), "n1", "ja").await.unwrap_err();
        match err {
            crate::error::Error::Api(detail) => assert!(detail.contains("UNAVAILABLE"), "{detail}"),
            other => panic!("expected Api, got {other:?}"),
        }
    }
}
