use serde::{Deserialize, Serialize};
use specta::Type;

/// ユーザリスト（List カラムのソース選択用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub id: String,
    pub name: String,
}

/// 「リストに追加」メニュー用。自分のリスト1件と、対象ユーザーがそのリストに入っているか。
/// `has_user` は Rust 側で `users/lists/list` の `userIds` から計算し、`userIds` 自体は IPC に載せない。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserListMembership {
    pub id: String,
    pub name: String,
    pub has_user: bool,
}

/// id + 表示名の軽量参照（アンテナ/チャンネル等のソース選択用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: String,
    pub name: String,
}
