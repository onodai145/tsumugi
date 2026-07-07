use serde::{Deserialize, Serialize};
use specta::Type;

/// タブ = 受信ソース + フィルタ（1タイムライン）。視覚的なカラム(ColumnGroup)に属する。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub id: String,
    pub account_id: String,
    pub kind: ColumnKind,
    /// グループ内のタブ順
    pub order: i32,
    pub filter: FilterQuery,
    pub notify_sound: bool,
    pub notify_desktop: bool,
    /// 所属する視覚カラム(ColumnGroup)の id
    pub group_id: String,
}

/// 視覚的なカラム（タブの集合）。幅と並び順を持つ。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnGroup {
    pub id: String,
    pub order: i32,
    pub width: i32,
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
    List {
        #[serde(rename = "listId")]
        list_id: String,
    },
    Search {
        query: String,
    },
}

impl ColumnKind {
    /// Streaming チャンネル名 + 接続パラメータ（`connect` の body.params）。
    /// ストリーミングを持たないソース（Search など）は None（＝REST のみ）。
    pub fn stream_request(&self) -> Option<(&'static str, serde_json::Value)> {
        use serde_json::json;
        Some(match self {
            ColumnKind::Home => ("homeTimeline", json!({})),
            ColumnKind::Local => ("localTimeline", json!({})),
            ColumnKind::Global => ("globalTimeline", json!({})),
            ColumnKind::Hybrid => ("hybridTimeline", json!({})),
            ColumnKind::List { list_id } => ("userList", json!({ "listId": list_id })),
            // Search/Notifications はストリーミング無し（REST のみ）/後続
            _ => return None,
        })
    }

    /// 初期ページ/過去ページ取得の REST エンドポイント + ボディ（未対応ソースは None）。
    pub fn rest_request(&self, limit: u32, until_id: Option<&str>) -> Option<(&'static str, serde_json::Value)> {
        use serde_json::json;
        let mut body = json!({ "limit": limit });
        if let Some(u) = until_id {
            body["untilId"] = json!(u);
        }
        Some(match self {
            ColumnKind::Home => ("notes/timeline", body),
            ColumnKind::Local => ("notes/local-timeline", body),
            ColumnKind::Global => ("notes/global-timeline", body),
            ColumnKind::Hybrid => ("notes/hybrid-timeline", body),
            ColumnKind::List { list_id } => {
                body["listId"] = json!(list_id);
                ("notes/user-list-timeline", body)
            }
            ColumnKind::Search { query } => {
                body["query"] = json!(query);
                ("notes/search", body)
            }
            ColumnKind::Notifications => return None,
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
