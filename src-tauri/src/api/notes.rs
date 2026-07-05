//! ノート系 REST（Phase 2: home timeline の初期取得・過去ページ取得）。

use crate::api::normalize::RawNote;
use crate::api::MisskeyClient;
use crate::domain::Note;
use crate::error::Result;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineArgs {
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    until_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since_id: Option<String>,
}

/// homeTimeline を取得する。`until_id` を渡すとそれより古いノート（上スクロールの過去ページ）。
/// 返りは Misskey 準拠で新しい順。
pub async fn home_timeline(
    client: &MisskeyClient,
    limit: u32,
    until_id: Option<String>,
) -> Result<Vec<Note>> {
    let args = TimelineArgs {
        limit: limit.clamp(1, 100),
        until_id,
        since_id: None,
    };
    let raw: Vec<RawNote> = client.post("notes/timeline", &args).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
