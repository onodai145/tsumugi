//! インスタンスメタ情報（Phase 3: カスタム絵文字一覧）。

use crate::api::MisskeyClient;
use crate::domain::EmojiDef;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEmoji {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct EmojisResponse {
    emojis: Vec<RawEmoji>,
}

/// インスタンスのローカルカスタム絵文字一覧。`emojis` は認証不要だが共通経路で叩く。
pub async fn list_emojis(client: &MisskeyClient) -> Result<Vec<EmojiDef>> {
    let res: EmojisResponse = client.post("emojis", &json!({})).await?;
    Ok(res
        .emojis
        .into_iter()
        .map(|e| EmojiDef {
            name: e.name,
            host: None, // ローカル
            url: e.url,
            category: e.category,
            aliases: e.aliases,
        })
        .collect())
}
