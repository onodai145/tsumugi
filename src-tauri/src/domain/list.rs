use serde::{Deserialize, Serialize};
use specta::Type;

/// ユーザリスト（List カラムのソース選択用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub id: String,
    pub name: String,
}
