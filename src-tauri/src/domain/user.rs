use serde::{Deserialize, Serialize};
use specta::Type;

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
}

impl User {
    /// "@user" または "@user@host"
    #[allow(dead_code)] // Phase 2/4: 表示・NQL の user.acct 評価で使用
    pub fn acct(&self) -> String {
        match &self.host {
            Some(h) => format!("@{}@{}", self.username, h),
            None => format!("@{}", self.username),
        }
    }
}
