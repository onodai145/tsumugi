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

/// 設計書§8.2 の MVP スコープ。Antenna/Channel/User/Tag/Cache は将来拡張（TQL §2）。
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

impl ColumnKind {
    /// Misskey Streaming のチャンネル名（未対応ソースは None）。
    pub fn stream_channel(&self) -> Option<&'static str> {
        Some(match self {
            ColumnKind::Home => "homeTimeline",
            ColumnKind::Local => "localTimeline",
            ColumnKind::Global => "globalTimeline",
            ColumnKind::Hybrid => "hybridTimeline",
            // List/Search/Notifications は追加パラメータや別扱いが要るため後続で対応
            _ => return None,
        })
    }

    /// 初期ページ取得の REST エンドポイント（未対応ソースは None）。
    pub fn rest_endpoint(&self) -> Option<&'static str> {
        Some(match self {
            ColumnKind::Home => "notes/timeline",
            ColumnKind::Local => "notes/local-timeline",
            ColumnKind::Global => "notes/global-timeline",
            ColumnKind::Hybrid => "notes/hybrid-timeline",
            _ => return None,
        })
    }
}

/// MVP=Keywords のみ。Phase 4 で Tql を有効化（filter/ast.rs の Query を保持）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum FilterQuery {
    /// 部分一致キーワード（OR）。空 Vec = 素通し。
    Keywords(Vec<String>),
    /// Phase 4: TQL クエリ文字列（保存形）。
    Tql(String),
}
