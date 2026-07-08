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
    /// ユーザ設定のタブ名。None なら種別から自動生成した名前を使う。
    pub title: Option<String>,
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
    Antenna {
        #[serde(rename = "antennaId")]
        antenna_id: String,
    },
    Channel {
        #[serde(rename = "channelId")]
        channel_id: String,
    },
    User {
        #[serde(rename = "userId")]
        user_id: String,
    },
    Tag {
        tag: String,
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
            ColumnKind::Antenna { antenna_id } => ("antenna", json!({ "antennaId": antenna_id })),
            ColumnKind::Channel { channel_id } => ("channel", json!({ "channelId": channel_id })),
            // Search/Notifications/User/Tag はストリーミング無し（REST のみ）
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
            ColumnKind::Antenna { antenna_id } => {
                body["antennaId"] = json!(antenna_id);
                ("antennas/notes", body)
            }
            ColumnKind::Channel { channel_id } => {
                body["channelId"] = json!(channel_id);
                ("channels/timeline", body)
            }
            ColumnKind::User { user_id } => {
                body["userId"] = json!(user_id);
                ("users/notes", body)
            }
            ColumnKind::Tag { tag } => {
                body["tag"] = json!(tag);
                ("notes/search-by-tag", body)
            }
            ColumnKind::Notifications => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antenna_channel_have_stream_and_rest() {
        let a = ColumnKind::Antenna { antenna_id: "a1".into() };
        let (ch, p) = a.stream_request().unwrap();
        assert_eq!(ch, "antenna");
        assert_eq!(p["antennaId"], "a1");
        let (ep, body) = a.rest_request(20, None).unwrap();
        assert_eq!(ep, "antennas/notes");
        assert_eq!(body["antennaId"], "a1");

        let c = ColumnKind::Channel { channel_id: "c1".into() };
        assert_eq!(c.stream_request().unwrap().0, "channel");
        assert_eq!(c.rest_request(20, None).unwrap().0, "channels/timeline");
    }

    #[test]
    fn user_and_tag_are_rest_only() {
        let u = ColumnKind::User { user_id: "u1".into() };
        assert!(u.stream_request().is_none());
        let (ep, body) = u.rest_request(20, Some("x")).unwrap();
        assert_eq!(ep, "users/notes");
        assert_eq!(body["userId"], "u1");
        assert_eq!(body["untilId"], "x");

        let t = ColumnKind::Tag { tag: "misskey".into() };
        assert!(t.stream_request().is_none());
        let (ep, body) = t.rest_request(20, None).unwrap();
        assert_eq!(ep, "notes/search-by-tag");
        assert_eq!(body["tag"], "misskey");
    }

    #[test]
    fn struct_variant_fields_serialize_camelcase() {
        // tag=type, フィールドは camelCase（antennaId 等）で往復すること
        let a = ColumnKind::Antenna { antenna_id: "a1".into() };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["type"], "antenna");
        assert_eq!(v["antennaId"], "a1");
        let back: ColumnKind = serde_json::from_value(v).unwrap();
        assert_eq!(back, a);
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
