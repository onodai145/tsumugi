//! ノート系 REST（timeline 取得・投稿・削除・リアクション）。

use crate::api::normalize::RawNote;
use crate::api::MisskeyClient;
use crate::domain::{Note, Visibility};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineArgs {
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    until_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since_id: Option<String>,
}

/// 指定エンドポイントのタイムラインを取得する。`until_id` で過去ページ。返りは新しい順。
pub async fn fetch_timeline(
    client: &MisskeyClient,
    endpoint: &str,
    limit: u32,
    until_id: Option<String>,
) -> Result<Vec<Note>> {
    let args = TimelineArgs {
        limit: limit.clamp(1, 100),
        until_id,
        since_id: None,
    };
    let raw: Vec<RawNote> = client.post(endpoint, &args).await?;
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
    fn timeline_args_omit_none_cursors() {
        let v = serde_json::to_value(TimelineArgs {
            limit: 20,
            until_id: None,
            since_id: None,
        })
        .unwrap();
        assert_eq!(v["limit"], 20);
        assert!(v.get("untilId").is_none());
        assert!(v.get("sinceId").is_none());

        let v2 = serde_json::to_value(TimelineArgs {
            limit: 20,
            until_id: Some("n1".into()),
            since_id: None,
        })
        .unwrap();
        assert_eq!(v2["untilId"], "n1");
    }
}
