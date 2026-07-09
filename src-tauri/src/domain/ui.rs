use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// 表示まわりのグローバル設定。テーマ・新規カラムの既定幅・キーバインド上書き・フォント・背景。
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
    /// CSS font-family 値をそのまま保持（例 `"Cascadia Code", monospace`）。
    /// 空文字なら既定フォントスタックを使う。
    #[serde(default)]
    pub font_family: String,
    /// 背景画像を data URL(base64)でそのまま保持。空文字なら背景画像なし。
    #[serde(default)]
    pub background_image: String,
    /// 背景に乗せる黒オーバーレイの濃さ（0〜100%）。可読性確保用。
    #[serde(default)]
    pub background_dim: i32,
    /// 背景画像のぼかし量（px, 0〜40）。
    #[serde(default)]
    pub background_blur: i32,
    /// カラム背景の不透明度（60〜100%）。背景画像を透けさせる。
    #[serde(default = "default_column_opacity")]
    pub column_opacity: i32,
    /// 既定アカウントの id。空文字なら未設定（アカウント一覧の先頭を使う）。
    #[serde(default)]
    pub default_account_id: String,
}

fn default_column_opacity() -> i32 {
    100
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            default_column_width: 300,
            keymap: HashMap::new(),
            font_family: String::new(),
            background_image: String::new(),
            background_dim: 0,
            background_blur: 0,
            column_opacity: default_column_opacity(),
            default_account_id: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_json_without_new_fields() {
        // keymap/font_family/background_* 追加前に保存された JSON も読めること（#[serde(default)]）
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.theme, "dark");
        assert_eq!(v.default_column_width, 320);
        assert!(v.keymap.is_empty());
        assert_eq!(v.font_family, "");
        assert_eq!(v.background_image, "");
        assert_eq!(v.background_dim, 0);
        assert_eq!(v.background_blur, 0);
        // column_opacity は #[serde(default = ...)] で 0 ではなく 100（不透明）にフォールバックすること。
        // 既存ユーザの見た目を壊さないための後方互換。
        assert_eq!(v.column_opacity, 100);
    }

    #[test]
    fn roundtrips_keymap() {
        let mut km = HashMap::new();
        km.insert("note.next".to_string(), "down".to_string());
        let p = UiPrefs {
            theme: "auto".into(),
            default_column_width: 300,
            keymap: km,
            font_family: "\"Cascadia Code\", monospace".into(),
            background_image: "data:image/png;base64,AAAA".into(),
            background_dim: 40,
            background_blur: 8,
            column_opacity: 85,
            default_account_id: "a1".into(),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }
}
