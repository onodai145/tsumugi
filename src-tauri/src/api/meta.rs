//! インスタンスメタ情報（カスタム絵文字一覧・ユーザリスト）。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::{EmojiDef, SourceItem, User, UserList};
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

/// 自分のアンテナ一覧（Antenna カラムのソース選択用）。
pub async fn fetch_antennas(client: &MisskeyClient) -> Result<Vec<SourceItem>> {
    #[derive(Deserialize)]
    struct RawAntenna {
        id: String,
        #[serde(default)]
        name: String,
    }
    let raw: Vec<RawAntenna> = client.post("antennas/list", &json!({})).await?;
    Ok(raw
        .into_iter()
        .map(|a| SourceItem { id: a.id, name: a.name })
        .collect())
}

/// フォロー中チャンネル一覧（Channel カラムのソース選択用）。
pub async fn fetch_followed_channels(client: &MisskeyClient) -> Result<Vec<SourceItem>> {
    #[derive(Deserialize)]
    struct RawChannel {
        id: String,
        #[serde(default)]
        name: String,
    }
    // フォロー中を全件（ページングは省略・上限 100）。
    let raw: Vec<RawChannel> = client
        .post("channels/followed", &json!({ "limit": 100 }))
        .await?;
    Ok(raw
        .into_iter()
        .map(|c| SourceItem { id: c.id, name: c.name })
        .collect())
}

/// acct（"@user@host" / "user@host" / "user"）から User を解決する（User カラム用）。
pub async fn resolve_user(client: &MisskeyClient, acct: &str) -> Result<User> {
    let t = acct.trim().trim_start_matches('@');
    let (username, host) = match t.split_once('@') {
        Some((u, h)) if !h.is_empty() => (u.to_string(), Some(h.to_string())),
        _ => (t.to_string(), None),
    };
    let mut body = json!({ "username": username });
    if let Some(h) = host {
        body["host"] = json!(h);
    }
    let raw: RawUser = client.post("users/show", &body).await?;
    Ok(raw.into())
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
