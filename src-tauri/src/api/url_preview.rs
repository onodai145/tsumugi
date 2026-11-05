//! summalyプロキシ（Misskeyインスタンス組み込みの `/url` ルート、または任意のカスタムプロキシ）
//! を叩いてリンクプレビュー(OGP相当)を取得する（Issue #9）。
//!
//! `/url` は Misskey の `/api/*` REST APIコマンドではなく、認証不要のプレーンなWebルート
//! （summalyプロキシの公開口）という理解。現行スナップショット `openapi/misskey-api-doc.json`
//! には掲載がない（非APIルートのため）。本モジュール下部の `#[ignore]` テストで実インスタンス
//! に対する実際のパス・レスポンス形を確認できる。

use crate::domain::{UrlPlayer, UrlPreview};
use crate::error::{Error, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUrlPreview {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    sitename: Option<String>,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    player: Option<RawPlayer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlayer {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    width: Option<i32>,
    #[serde(default)]
    height: Option<i32>,
}

/// `raw` を `UrlPreview` へ正規化する。`raw.url` が欠落していれば要求した `target_url` を使う。
/// `player` は `url` が無ければ埋め込みようがないため丸ごと `None` に落とす。
fn normalize(raw: RawUrlPreview, target_url: &str) -> UrlPreview {
    UrlPreview {
        url: raw.url.unwrap_or_else(|| target_url.to_string()),
        title: raw.title,
        description: raw.description,
        thumbnail: raw.thumbnail,
        icon: raw.icon,
        sitename: raw.sitename,
        sensitive: raw.sensitive,
        player: raw.player.and_then(|p| {
            p.url.map(|url| UrlPlayer {
                url,
                width: p.width,
                height: p.height,
            })
        }),
    }
}

/// 取得先が `net_guard` に拒否されたときのメッセージ(Issue #549)。
pub(crate) const BLOCKED_MESSAGE: &str = "取得先が許可されていません(ローカル/社内ネットワークのアドレス)。\
インポートした設定の取得先は、設定の外部連携で保存し直すと使えます";

/// `proxy_base`（例: `"https://misskey.io/url"` またはユーザ設定のカスタムプロキシURL）に
/// `?url=<target_url>` を付与してGETし、応答を [`UrlPreview`] へ正規化する。
/// 認証不要（トークンは付与しない）。
///
/// `http` は `net_guard::build_fetch_client` で作った取得専用クライアントを渡す。IPリテラルの
/// 取得先はDNSリゾルバを通らないため、ここで送信前に `validate_url` で検査する。ホスト名は
/// 接続時(DNS解決後)と、リダイレクトの各ホップで検査される。
pub async fn fetch_url_preview(
    http: &reqwest::Client,
    allow: &crate::net_guard::FetchAllowlist,
    proxy_base: &str,
    target_url: &str,
) -> Result<UrlPreview> {
    let base = url::Url::parse(proxy_base).map_err(|e| Error::Invalid(format!("invalid url: {e}")))?;
    if let Err(blocked) = crate::net_guard::validate_url(&base, allow) {
        log::warn!("url preview blocked before request: {blocked}");
        return Err(Error::Invalid(BLOCKED_MESSAGE.to_string()));
    }
    let resp = http
        .get(base)
        .query(&[("url", target_url)])
        .send()
        .await
        .map_err(|e| {
            if crate::net_guard::is_blocked_error(&e) {
                Error::Invalid(BLOCKED_MESSAGE.to_string())
            } else {
                Error::from(e)
            }
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Api(format!("url preview: HTTP {status}")));
    }
    let raw: RawUrlPreview = resp.json().await?;
    Ok(normalize(raw, target_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_full_response() {
        let raw: RawUrlPreview = serde_json::from_str(
            r#"{
                "url": "https://example.com/article",
                "title": "記事タイトル",
                "description": "説明文",
                "thumbnail": "https://example.com/thumb.png",
                "icon": "https://example.com/favicon.ico",
                "sitename": "Example",
                "sensitive": true,
                "player": {"url": "https://example.com/embed", "width": 640, "height": 360}
            }"#,
        )
        .unwrap();
        let preview = normalize(raw, "https://example.com/article");
        assert_eq!(preview.url, "https://example.com/article");
        assert_eq!(preview.title.as_deref(), Some("記事タイトル"));
        assert_eq!(preview.description.as_deref(), Some("説明文"));
        assert_eq!(preview.thumbnail.as_deref(), Some("https://example.com/thumb.png"));
        assert_eq!(preview.icon.as_deref(), Some("https://example.com/favicon.ico"));
        assert_eq!(preview.sitename.as_deref(), Some("Example"));
        assert!(preview.sensitive);
        let player = preview.player.unwrap();
        assert_eq!(player.url, "https://example.com/embed");
        assert_eq!(player.width, Some(640));
        assert_eq!(player.height, Some(360));
    }

    #[test]
    fn falls_back_to_target_url_when_url_field_missing() {
        let raw: RawUrlPreview = serde_json::from_str(r#"{}"#).unwrap();
        let preview = normalize(raw, "https://example.com/no-og");
        assert_eq!(preview.url, "https://example.com/no-og");
        assert!(preview.title.is_none());
        assert!(!preview.sensitive);
        assert!(preview.player.is_none());
    }

    #[test]
    fn drops_player_without_url() {
        let raw: RawUrlPreview =
            serde_json::from_str(r#"{"player": {"width": 640, "height": 360}}"#).unwrap();
        let preview = normalize(raw, "https://example.com/x");
        assert!(preview.player.is_none());
    }

    /// 実インスタンスの `/url` エンドポイントに対する疎通確認。
    /// パス・レスポンス形が想定と異なる場合、このテストの失敗内容を見て本モジュールを直す。
    /// ネットワーク依存のため既定では実行しない: `cargo test -- --ignored real_url_preview`
    #[ignore]
    #[tokio::test]
    async fn real_url_preview_from_misskey_io() {
        let http = reqwest::Client::new();
        let allow = crate::net_guard::FetchAllowlist::default();
        let preview = fetch_url_preview(&http, &allow, "https://misskey.io/url", "https://misskey.io/")
            .await
            .expect("fetch_url_preview should succeed against misskey.io");
        assert!(
            preview.title.as_deref().is_some_and(|t| !t.is_empty()),
            "expected a non-empty title, got {:?}",
            preview.title
        );
    }
}
