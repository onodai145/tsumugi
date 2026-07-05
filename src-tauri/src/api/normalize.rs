//! Misskey の生 JSON レスポンスを domain 型へ正規化する。
//! Phase 1 では User のみ（Note の正規化は Phase 2 で追加）。

use crate::domain::User;
use serde::Deserialize;

/// Misskey の User オブジェクト（`i` / `me` / miauth の user）を受ける生型。
/// UserLite にしか無いフィールドもあるため、集計値は `#[serde(default)]`。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_cat: bool,
    #[serde(default)]
    pub followers_count: u32,
    #[serde(default)]
    pub following_count: u32,
    #[serde(default)]
    pub notes_count: u32,
}

impl From<RawUser> for User {
    fn from(r: RawUser) -> Self {
        User {
            id: r.id,
            username: r.username,
            host: r.host,
            name: r.name,
            avatar_url: r.avatar_url,
            is_bot: r.is_bot,
            is_cat: r.is_cat,
            followers_count: r.followers_count,
            following_count: r.following_count,
            notes_count: r.notes_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_user_and_defaults_missing_counts() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"a1","username":"alice","host":null,"name":"Alice","isBot":false}"#,
        )
        .unwrap();
        let u: User = raw.into();
        assert_eq!(u.id, "a1");
        assert_eq!(u.username, "alice");
        assert!(u.host.is_none());
        assert_eq!(u.acct(), "@alice");
        assert_eq!(u.notes_count, 0); // 欠損は 0
    }

    #[test]
    fn parses_remote_user_with_counts() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"b2","username":"bob","host":"remote.example","name":null,
                "followersCount":12,"followingCount":3,"notesCount":45,"isCat":true}"#,
        )
        .unwrap();
        let u: User = raw.into();
        assert_eq!(u.acct(), "@bob@remote.example");
        assert_eq!(u.followers_count, 12);
        assert!(u.is_cat);
    }
}
