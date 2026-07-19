use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// テーマ1個分の配色（app.css の CSS変数9個に対応）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub surface1: String,
    pub surface2: String,
    pub surface3: String,
    pub border: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    /// 成功/肯定的な意味の強調色（例: Renoteバナー）。追加前のカスタムテーマ読み込み用に既定値を持つ。
    #[serde(default = "default_success_color")]
    pub success: String,
    /// 情報的な意味の強調色（例: リプライバナー）。追加前のカスタムテーマ読み込み用に既定値を持つ。
    #[serde(default = "default_info_color")]
    pub info: String,
}

fn default_success_color() -> String {
    "#22c55e".into()
}

fn default_info_color() -> String {
    "#3b82f6".into()
}

/// ユーザーが作成したカスタムテーマ。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub colors: ThemeColors,
}

/// 表示まわりのグローバル設定。テーマ・新規カラムの既定幅・キーバインド上書き・フォント・背景。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiPrefs {
    /// "auto" | "light" | "dark" | "preset:<id>"(フロント側定義) | "custom:<CustomTheme.id>"
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
    /// 背景画像の配置方法。"cover" | "contain" | "fill" | "tile"（Issue #45）。
    #[serde(default = "default_background_fit_mode")]
    pub background_fit_mode: String,
    /// リアクションピッカーのピン留め絵文字（Issue #19）。Unicode絵文字はそのまま、
    /// カスタム絵文字は ":name:" 形式で保持する。フロント側で編集し、ここへ永続化する。
    #[serde(default = "default_pinned_emojis")]
    pub pinned_emojis: Vec<String>,
    /// モバイル版UI(投稿モーダル+FAB)かPC版UI(常時投稿欄)かの表示切替（Issue #51）。
    /// "auto"(OS判定に従う) | "desktop"(強制PC版) | "mobile"(強制モバイル版)。
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
    /// 既定アカウントの id。空文字なら未設定（アカウント一覧の先頭を使う）。
    #[serde(default)]
    pub default_account_id: String,
    /// Unicode絵文字の表示スタイル。"native" | "twemoji" | "fluentEmoji"（本家 emojiStyle 準拠）。
    #[serde(default = "default_emoji_style")]
    pub emoji_style: String,
    /// 起動時、キャッシュに無い間(閉じていた間)のノートをRESTで遡って埋める件数の上限。
    /// 0 なら無効（従来どおりキャッシュのみ表示）。
    #[serde(default = "default_gap_fill_limit")]
    pub gap_fill_limit: i32,
    /// ユーザーが作成したカスタムテーマの一覧。
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// メディア添付ノートのサムネイル高さ上限（px）。ノートを詰めたい人は小さく、
    /// 大きく見たい人は大きくできるようにする。
    #[serde(default = "default_media_thumbnail_height")]
    pub media_thumbnail_height: i32,
    /// ローカルキャッシュに保持するノート件数の上限。超えた分は古い順に削除する
    /// （Issue #6: 無制限に溜まり続けないようにする）。0 なら無制限。
    #[serde(default = "default_note_cache_limit")]
    pub note_cache_limit: i32,
    /// ローカルキャッシュに保持するノートの経過日数上限。created_at がこれより古いノートは
    /// 削除する。0 なら無制限。
    #[serde(default)]
    pub note_cache_max_age_days: i32,
    /// ローカルキャッシュDBのサイズ上限（MB）。超えている間は古い順に削除し続ける。0 なら無制限。
    #[serde(default)]
    pub note_cache_max_size_mb: i32,
    /// Rust側ログ(WS再接続/pingタイムアウト等)をアプリのログディレクトリへファイル永続化するか
    /// （Issue #12: 「謎のタイミングで通知が来る」の調査用）。切替はアプリ再起動後に反映される。
    #[serde(default)]
    pub enable_file_logging: bool,
}

fn default_column_opacity() -> i32 {
    100
}

fn default_background_fit_mode() -> String {
    "cover".into()
}

fn default_pinned_emojis() -> Vec<String> {
    ["👍", "❤️", "😆", "🎉", "🤔", "😢", "😮", "🙏"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn default_ui_mode() -> String {
    "auto".into()
}

fn default_emoji_style() -> String {
    "twemoji".into()
}

fn default_gap_fill_limit() -> i32 {
    200
}

fn default_media_thumbnail_height() -> i32 {
    200
}

fn default_note_cache_limit() -> i32 {
    10000
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
            background_fit_mode: default_background_fit_mode(),
            pinned_emojis: default_pinned_emojis(),
            ui_mode: default_ui_mode(),
            default_account_id: String::new(),
            emoji_style: default_emoji_style(),
            gap_fill_limit: default_gap_fill_limit(),
            custom_themes: Vec::new(),
            media_thumbnail_height: default_media_thumbnail_height(),
            note_cache_limit: default_note_cache_limit(),
            note_cache_max_age_days: 0,
            note_cache_max_size_mb: 0,
            enable_file_logging: false,
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
        // background_fit_mode も同様に既定値(cover, 追加前の見た目)へフォールバックすること。
        assert_eq!(v.background_fit_mode, "cover");
        // pinned_emojis も同様に既定値(追加前のハードコード8種)へフォールバックすること。
        assert_eq!(
            v.pinned_emojis,
            vec!["👍", "❤️", "😆", "🎉", "🤔", "😢", "😮", "🙏"]
        );
        // ui_mode も同様に既定値(auto, 追加前のOS判定のみの挙動)へフォールバックすること。
        assert_eq!(v.ui_mode, "auto");
        // emoji_style も同様に既定値(twemoji, 本家準拠)へフォールバックすること。
        assert_eq!(v.emoji_style, "twemoji");
        assert_eq!(v.gap_fill_limit, 200);
        assert_eq!(v.media_thumbnail_height, 200);
        assert_eq!(v.note_cache_limit, 10000);
        assert_eq!(v.note_cache_max_age_days, 0);
        assert_eq!(v.note_cache_max_size_mb, 0);
        assert_eq!(v.enable_file_logging, false);
    }

    #[test]
    fn theme_colors_deserializes_legacy_json_without_success_info() {
        // success/info 追加前に保存されたカスタムテーマも読めること（#[serde(default)]）
        let v: ThemeColors = serde_json::from_str(
            r##"{"surface1":"#111","surface2":"#222","surface3":"#333","border":"#444",
                "text":"#eee","textDim":"#999","accent":"#ff8800"}"##,
        )
        .unwrap();
        assert_eq!(v.success, "#22c55e");
        assert_eq!(v.info, "#3b82f6");
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
            background_fit_mode: "tile".into(),
            pinned_emojis: vec!["👍".into(), ":blob_cat:".into()],
            ui_mode: "mobile".into(),
            default_account_id: "a1".into(),
            emoji_style: "fluentEmoji".into(),
            gap_fill_limit: 500,
            custom_themes: vec![CustomTheme {
                id: "t1".into(),
                name: "My Theme".into(),
                colors: ThemeColors {
                    surface1: "#111111".into(),
                    surface2: "#222222".into(),
                    surface3: "#333333".into(),
                    border: "#444444".into(),
                    text: "#eeeeee".into(),
                    text_dim: "#999999".into(),
                    accent: "#ff8800".into(),
                    success: "#16a34a".into(),
                    info: "#2563eb".into(),
                },
            }],
            media_thumbnail_height: 320,
            note_cache_limit: 8000,
            note_cache_max_age_days: 30,
            note_cache_max_size_mb: 200,
            enable_file_logging: true,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn custom_themes_defaults_to_empty_for_legacy_json() {
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert!(v.custom_themes.is_empty());
    }
}
