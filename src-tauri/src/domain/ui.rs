use serde::{Deserialize, Serialize};
use specta::Type;

/// 表示まわりのグローバル設定。テーマと新規カラムの既定幅。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiPrefs {
    /// "auto" | "light" | "dark"
    pub theme: String,
    /// 新規カラム（グループ）作成時の既定幅（px）
    pub default_column_width: i32,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            default_column_width: 300,
        }
    }
}
