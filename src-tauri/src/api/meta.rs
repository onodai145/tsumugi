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

/// 接続先インスタンスの名前・アイコン・テーマカラー（Instance Ticker用、Issue #103）。
/// `/api/meta` は認証不要だが、他エンドポイントと同じ経路(`client.post`)で叩く。
/// `detail: false` で軽量なレスポンス(MetaLite相当)にする。
pub async fn fetch_meta(client: &MisskeyClient) -> Result<crate::domain::InstanceInfo> {
    let raw: RawMeta = client.post("meta", &json!({ "detail": false })).await?;
    let info: crate::domain::InstanceInfo = raw.into();
    Ok(info.with_favicon_fallback(client.host()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    theme_color: Option<String>,
}

impl From<RawMeta> for crate::domain::InstanceInfo {
    fn from(r: RawMeta) -> Self {
        crate::domain::InstanceInfo {
            name: r.name,
            icon_url: r.icon_url,
            theme_color: r.theme_color,
        }
    }
}

#[cfg(test)]
mod meta_info_tests {
    use super::*;

    #[test]
    fn raw_meta_maps_all_fields() {
        let raw: RawMeta = serde_json::from_str(
            r##"{"name":"Misskey.io","iconUrl":"https://misskey.io/icon.png","themeColor":"#86b300"}"##,
        )
        .unwrap();
        let info: crate::domain::InstanceInfo = raw.into();
        assert_eq!(info.name, Some("Misskey.io".to_string()));
        assert_eq!(info.icon_url, Some("https://misskey.io/icon.png".to_string()));
        assert_eq!(info.theme_color, Some("#86b300".to_string()));
    }

    #[test]
    fn raw_meta_defaults_missing_fields_to_none() {
        let raw: RawMeta = serde_json::from_str(r#"{}"#).unwrap();
        let info: crate::domain::InstanceInfo = raw.into();
        assert_eq!(info.name, None);
        assert_eq!(info.icon_url, None);
        assert_eq!(info.theme_color, None);
    }

    /// fetch_meta の実体は「RawMeta→InstanceInfo変換 + favicon フォールバック」の合成。
    /// ネットワーク呼び出し自体はこのプロジェクトの慣例上モックしないため（api/drive.rs等
    /// 参照）、この合成が実際に適用先で使う形と一致することをここで確認する。
    #[test]
    fn instance_info_from_meta_without_icon_falls_back_to_host_favicon() {
        let raw: RawMeta = serde_json::from_str(
            r##"{"name":"しーもハウス","iconUrl":null,"themeColor":null}"##,
        )
        .unwrap();
        let info: crate::domain::InstanceInfo = raw.into();
        let info = info.with_favicon_fallback("misskey.omhnc.net");
        assert_eq!(info.icon_url, Some("https://misskey.omhnc.net/favicon.ico".to_string()));
        assert_eq!(info.theme_color, None);
    }
}
