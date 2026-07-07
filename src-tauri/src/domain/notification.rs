use super::{Note, User};
use serde::{Deserialize, Serialize};
use specta::Type;

/// 通知（Notifications カラム用）。Note とは別物で、フォロー/メンション/リアクション等を表す。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    /// epoch秒
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    /// 種別: follow / mention / reply / renote / quote / reaction / pollEnded /
    /// receiveFollowRequest / followRequestAccepted / achievementEarned / app 等
    #[serde(rename = "type")]
    pub kind: String,
    /// 通知を起こしたユーザ（follow/reaction 等）
    pub user: Option<User>,
    /// 対象ノート（mention/reply/renote/quote/reaction/pollEnded）
    pub note: Option<Note>,
    /// リアクション種別（kind == "reaction" のとき）
    pub reaction: Option<String>,
}
