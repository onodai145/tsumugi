//! インスタンスメタ情報（カスタム絵文字一覧・ユーザリスト）。

use crate::api::MisskeyClient;
use crate::domain::{EmojiDef, UserList};
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

/// 自分のユーザリスト一覧（List カラムのソース選択用）。
pub async fn fetch_user_lists(client: &MisskeyClient) -> Result<Vec<UserList>> {
    #[derive(Deserialize)]
    struct RawList {
        id: String,
        #[serde(default)]
        name: String,
    }
    let raw: Vec<RawList> = client.post("users/lists/list", &json!({})).await?;
    Ok(raw
        .into_iter()
        .map(|l| UserList {
            id: l.id,
            name: l.name,
        })
        .collect())
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
