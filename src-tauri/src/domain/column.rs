use serde::{Deserialize, Serialize};
use specta::Type;

/// カラム = 受信ソース + フィルタ。設計書§5 / phase0-scaffold §2.6。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub id: String,
    pub account_id: String,
    pub kind: ColumnKind,
    pub order: i32,
    pub width: i32,
    pub filter: FilterQuery,
    pub notify_sound: bool,
    pub notify_desktop: bool,
}

/// 設計書§8.2 の MVP スコープ。Antenna/Channel/User/Tag/Cache は将来拡張（NQL §2）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ColumnKind {
    Home,
    Local,
    Global,
    Hybrid,
    Notifications,
    List { list_id: String },
    Search { query: String },
}

/// MVP=Keywords のみ。Phase 4 で Nql を有効化（filter/ast.rs の Query を保持）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum FilterQuery {
    /// 部分一致キーワード（OR）。空 Vec = 素通し。
    Keywords(Vec<String>),
    /// Phase 4: NQL クエリ文字列（保存形）。
    Nql(String),
}
