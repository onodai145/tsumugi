use serde::{Deserialize, Serialize};
use specta::Type;

/// ローカル NG（ミュート）設定。サーバの mute/block とは別の、クライアント側フィルタ。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MuteConfig {
    /// 本文/CW に含まれると非表示にする語（部分一致・大文字小文字無視）
    pub ng_words: Vec<String>,
    /// 非表示にするユーザ（`@user@host` 形式。`@` は省略可）
    pub ng_users: Vec<String>,
    /// 非表示にするインスタンス（host。例: example.com）
    pub ng_instances: Vec<String>,
}
