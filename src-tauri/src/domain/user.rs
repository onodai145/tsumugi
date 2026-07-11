use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// docs/filter-dsl-design.md §7。`host` が None ならローカルユーザ。
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
}
