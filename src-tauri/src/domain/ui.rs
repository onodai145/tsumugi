use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// 表示まわりのグローバル設定。テーマ・新規カラムの既定幅・キーバインド上書き。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiPrefs {
    /// "auto" | "light" | "dark"
    pub theme: String,
    /// 新規カラム（グループ）作成時の既定幅（px）
    pub default_column_width: i32,
    /// キーバインドの上書き（action -> chord）。空なら既定を使う。
    /// 中身の解釈はフロント（lib/keymap）に委ねる（Rust からは不透明に永続化）。
    #[serde(default)]
    pub keymap: HashMap<String, String>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            default_column_width: 300,
            keymap: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_json_without_keymap() {
        // keymap 追加前に保存された JSON も読めること（#[serde(default)]）
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.theme, "dark");
        assert_eq!(v.default_column_width, 320);
        assert!(v.keymap.is_empty());
    }

    #[test]
    fn roundtrips_keymap() {
        let mut km = HashMap::new();
        km.insert("note.next".to_string(), "down".to_string());
        let p = UiPrefs {
            theme: "auto".into(),
            default_column_width: 300,
            keymap: km,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }
}
