//! 通知の REST 取得（i/notifications）。

use crate::api::normalize::RawNotification;
use crate::api::MisskeyClient;
use crate::domain::Notification;
use crate::error::Result;
use serde_json::json;

/// 通知一覧を取得する。`until_id` で過去ページ。新しい順。
pub async fn fetch_notifications(
    client: &MisskeyClient,
    limit: u32,
    until_id: Option<&str>,
) -> Result<Vec<Notification>> {
    let mut body = json!({ "limit": limit.clamp(1, 100) });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    let raw: Vec<RawNotification> = client.post("i/notifications", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}
