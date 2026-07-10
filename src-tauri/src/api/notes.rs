//! ノート系 REST（timeline 取得・投稿・削除・リアクション）。

use crate::api::normalize::RawNote;
use crate::api::MisskeyClient;
use crate::domain::{Note, Visibility};
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
    #[serde(default)]
    pub local_only: bool,
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
        // 空の fileIds は送らない
        assert!(v.get("fileIds").is_none());
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
}
