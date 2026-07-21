use serde::{Deserialize, Serialize};
use specta::Type;

/// Misskey のクリップ（名前付きノート集合）。今回のスコープでは一覧表示と
/// ノート追加にしか使わないため、フィールドは最小限。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_public: bool,
    pub notes_count: u32,
}
