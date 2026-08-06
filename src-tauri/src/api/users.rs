//! ユーザー検索・プロフィール取得・フォロー操作 REST。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>> {
    let body = json!({
        "query": query,
        "limit": limit,
        "origin": "combined",
        "detail": false,
    });
    let raw: Vec<RawUser> = client.post("users/search", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// `users/show` のレスポンス。UserDetailedNotMe は `RawUser` にないフォロー関係フラグを
/// 追加で持つため、`#[serde(flatten)]` で `RawUser` の全フィールド + 関係フラグを一度に受ける。
/// 自分自身を対象にした場合（`MeDetailed`）は関係フラグが存在せず `None` になる。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUserShow {
    #[serde(flatten)]
    pub user: RawUser,
    #[serde(default)]
    pub is_following: Option<bool>,
}

/// ユーザーIDからプロフィール詳細（フォロー関係フラグ込み）を取得する。
pub async fn show(client: &MisskeyClient, user_id: &str) -> Result<RawUserShow> {
    client.post("users/show", &json!({ "userId": user_id })).await
}

/// フォローする。
pub async fn follow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/create", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// フォロー解除する。
pub async fn unfollow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}
