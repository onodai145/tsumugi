use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// 投稿元インスタンスの表示情報（Instance Ticker用、Issue #103）。
/// リモートユーザーは Misskey の `UserLite.instance` から、ローカルユーザーは
/// 接続先インスタンスの `/api/meta`（[`Account::instance`]）から埋める。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub theme_color: Option<String>,
}

/// docs/design/filter-dsl-design.md §7。`host` が None ならローカルユーザ。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    /// @なしのユーザ名
    pub username: String,
    /// None=ローカル
    pub host: Option<String>,
    /// 表示名
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_bot: bool,
    pub is_cat: bool,
    pub followers_count: u32,
    pub following_count: u32,
    pub notes_count: u32,
    /// 表示名(`name`)中のカスタム絵文字ショートコード解決用 {name: url}。
    /// 既存キャッシュ済みJSON(このフィールド追加前に保存されたもの)との後方互換のため default。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    /// 自己紹介（Misskeyの`description`）。UserLiteコンテキスト（ノート本文の著者等）では取得されない。
    #[serde(default)]
    pub bio: Option<String>,
    /// バナー画像URL。同上、UserLiteコンテキストでは取得されない。
    #[serde(default)]
    pub banner_url: Option<String>,
    /// 投稿元インスタンス情報。リモートユーザーのみ Some（Misskeyがローカルユーザーには
    /// このフィールドを付与しない）。追加前に保存されたキャッシュ済みJSONとの後方互換のため default。
    #[serde(default)]
    pub instance: Option<InstanceInfo>,
}

impl User {
    /// "@user" または "@user@host"
    #[allow(dead_code)] // Phase 2/4: 表示・TQL の user.acct 評価で使用
    pub fn acct(&self) -> String {
        match &self.host {
            Some(h) => format!("@{}@{}", self.username, h),
            None => format!("@{}", self.username),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// emojis フィールド追加前に保存されたキャッシュ済みJSON(SQLiteのnote_cache等)を
    /// 読み込めること。#[serde(default)] が無いと deserialize エラーになる。
    #[test]
    fn deserializes_without_emojis_field_for_backward_compat() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let u: User = serde_json::from_str(json).unwrap();
        assert!(u.emojis.is_empty());
    }

    /// bio/bannerUrl フィールド追加前に保存されたキャッシュ済みJSONを読み込めること。
    #[test]
    fn deserializes_without_bio_or_banner_for_backward_compat() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let u: User = serde_json::from_str(json).unwrap();
        assert_eq!(u.bio, None);
        assert_eq!(u.banner_url, None);
    }
}
