use serde::{Deserialize, Serialize};
use specta::Type;

/// ユーザリスト（List カラムのソース選択用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub id: String,
    pub name: String,
}

/// id + 表示名の軽量参照（アンテナ/チャンネル等のソース選択用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: String,
    pub name: String,
}
