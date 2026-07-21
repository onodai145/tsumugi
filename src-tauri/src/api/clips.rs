//! クリップ系 REST（一覧取得・作成・ノート追加）。

use crate::api::MisskeyClient;
use crate::domain::Clip;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

/// Misskey の Clip オブジェクトの生型。`favoritedCount`/`user`/`lastClippedAt` 等
/// 今回使わないフィールドは受けない。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawClip {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    is_public: bool,
    notes_count: i64,
}

impl From<RawClip> for Clip {
    fn from(r: RawClip) -> Self {
        Clip {
            id: r.id,
            name: r.name,
            description: r.description,
            is_public: r.is_public,
            notes_count: r.notes_count,
        }
    }
}

/// 自分のクリップ一覧。`clips/list`（引数なし）。
pub async fn list_clips(client: &MisskeyClient) -> Result<Vec<Clip>> {
    let raw: Vec<RawClip> = client.post("clips/list", &json!({})).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// クリップ新規作成。`clips/create`。isPublic/description は既定値(非公開・なし)のまま。
pub async fn create_clip(client: &MisskeyClient, name: &str) -> Result<Clip> {
    let raw: RawClip = client.post("clips/create", &json!({ "name": name })).await?;
    Ok(raw.into())
}

/// クリップへノートを追加。`clips/add-note`。
pub async fn add_note_to_clip(client: &MisskeyClient, clip_id: &str, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post(
            "clips/add-note",
            &json!({ "clipId": clip_id, "noteId": note_id }),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_clip_converts_to_domain() {
        let raw = RawClip {
            id: "c1".into(),
            name: "あとで読む".into(),
            description: Some("desc".into()),
            is_public: false,
            notes_count: 3,
        };
        let clip: Clip = raw.into();
        assert_eq!(clip.id, "c1");
        assert_eq!(clip.name, "あとで読む");
        assert_eq!(clip.description.as_deref(), Some("desc"));
        assert!(!clip.is_public);
        assert_eq!(clip.notes_count, 3);
    }
}
