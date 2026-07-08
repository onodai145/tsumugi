use super::user::User;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// docs/filter-dsl-design.md §7 / 設計書§5.1。フィルタ評価の対象そのもの。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// aid/aidx。数値比較しない
    pub id: String,
    /// epoch秒。時間比較はこれ。TS へは number で出す（2^53 に収まり精度損失なし）
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    /// MFM原文。純Renoteは None
    pub text: Option<String>,
    pub cw: Option<String>,
    pub visibility: Visibility,
    pub local_only: bool,
    pub user: User,
    pub reply_id: Option<String>,
    pub renote_id: Option<String>,
    /// 引用/Renote先（浅く保持）
    pub renote: Option<Box<Note>>,
    pub files: Vec<DriveFile>,
    pub poll: Option<Poll>,
    pub tags: Vec<String>,
    /// メンション先 userId
    pub mentions: Vec<String>,
    /// カスタム絵文字 name -> url（本文 MFM の `:name:` とリアクション絵文字の描画に使う）
    pub emojis: HashMap<String, String>,
    pub channel_id: Option<String>,
    pub via: Option<String>,
    pub lang: Option<String>,

    // 可変集計部（noteUpdated で更新。値は更新するが出入りはしない）
    /// キー=Misskey形式（Unicode生 or :name@host:）
    pub reactions: HashMap<String, u32>,
    pub reaction_count: u32,
    pub renote_count: u32,
    pub reply_count: u32,
    pub my_reaction: Option<String>,
    pub is_renoted_by_me: bool,
    pub is_favorited_by_me: bool,
    pub is_pinned: bool,
}

/// Specified = direct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    Public,
    Home,
    Followers,
    Specified,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    /// "image/png" 等。file_types はここから category 化
    pub mime_type: String,
    pub is_sensitive: bool,
    pub url: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    pub choices: Vec<PollChoice>,
    pub multiple: bool,
    #[specta(type = Option<specta_typescript::Number>)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PollChoice {
    pub text: String,
    pub votes: u32,
    pub is_voted: bool,
}
