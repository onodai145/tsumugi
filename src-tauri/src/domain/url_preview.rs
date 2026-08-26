//! summalyプロキシから取得するリンクプレビュー（OGP相当）情報。
//! Issue #9: 投稿本文中のURLにタイトル・説明・サムネイルのカードを添える。

use serde::{Deserialize, Serialize};
use specta::Type;

/// プレビュー結果。`title`以下は summaly の応答でいずれも欠落しうるため `Option`。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UrlPreview {
    /// プレビュー対象のURL（summalyが返したものを優先し、無ければ要求したURLをそのまま使う）。
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub icon: Option<String>,
    pub sitename: Option<String>,
    /// センシティブ判定。フィールド自体が無い応答は false 扱い。
    #[serde(default)]
    pub sensitive: bool,
    pub player: Option<UrlPlayer>,
}

/// 動画/音声プレイヤー埋め込み情報（YouTube等のoEmbed player）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UrlPlayer {
    pub url: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}
