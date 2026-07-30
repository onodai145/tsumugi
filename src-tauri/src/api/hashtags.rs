//! ハッシュタグ検索 REST（ハッシュタグ補完用）。

use crate::api::MisskeyClient;
use crate::error::Result;
use serde_json::json;

pub async fn search_hashtags(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<String>> {
    let body = json!({
        "query": query,
        "limit": limit,
    });
    client.post("hashtags/search", &body).await
}
