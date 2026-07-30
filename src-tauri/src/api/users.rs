//! ユーザー検索 REST（メンション補完用）。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
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
