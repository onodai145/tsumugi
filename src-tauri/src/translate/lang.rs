//! 設定の翻訳先言語コードを、PLaMo 3 Translate のコードへ変換する（Issue #603）。
//! 対応は 53 言語（モデルカードの「Supported Languages」で確認済み）。

/// PLaMo 3 Translate が対応する言語コード。
const SUPPORTED: &[&str] = &[
    "ja", "en", "zh-hans", "zh-hant", "ko", "ar", "az", "be", "bg", "bn", "ca", "ceb", "de", "es", "et",
    "eu", "fa", "fr", "gu", "he", "hi", "hr", "hu", "hy", "id", "it", "jv", "ka", "kk", "km", "lt", "lv",
    "ml", "mr", "ms", "my", "ne", "nl", "pa", "pl", "ru", "si", "sk", "sl", "sr", "sw", "ta", "te", "th",
    "tl", "ur", "uz", "vi",
];

/// 設定の言語コードを PLaMo のコードへ変換する。非対応は `None`。
/// `zh` は簡体字（`zh-hans`）、`zh-tw` / `zh-hk` は繁体字。地域付きのコード（`en-US` など）は地域を除いて照合する。
pub fn to_plamo_lang(code: &str) -> Option<&'static str> {
    let c = code.trim().to_ascii_lowercase().replace('_', "-");
    if c.is_empty() {
        return None;
    }
    match c.as_str() {
        "zh" | "zh-cn" | "zh-sg" => return Some("zh-hans"),
        "zh-tw" | "zh-hk" | "zh-mo" => return Some("zh-hant"),
        _ => {}
    }
    if let Some(s) = find(&c) {
        return Some(s);
    }
    let base = c.split('-').next().unwrap_or("");
    // 中国語は上で扱った地域だけを受ける（`zh-xx` を簡体字に落とさない）。
    if base == "zh" {
        return None;
    }
    find(base)
}

fn find(code: &str) -> Option<&'static str> {
    SUPPORTED.iter().copied().find(|s| *s == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_53_languages_of_the_model_card() {
        assert_eq!(SUPPORTED.len(), 53);
        let mut sorted = SUPPORTED.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 53, "重複がある");
    }

    #[test]
    fn maps_the_settings_presets() {
        // frontend/src/lib/translateLang.ts のプリセット（pt を除く）。
        for (code, want) in [
            ("ja", "ja"),
            ("en", "en"),
            ("zh", "zh-hans"),
            ("ko", "ko"),
            ("fr", "fr"),
            ("de", "de"),
            ("es", "es"),
            ("ru", "ru"),
        ] {
            assert_eq!(to_plamo_lang(code), Some(want), "{code}");
        }
    }

    #[test]
    fn portuguese_is_not_supported() {
        // プリセットの中で、ポルトガル語だけが PLaMo の対応言語に含まれない。
        assert_eq!(to_plamo_lang("pt"), None);
        assert_eq!(to_plamo_lang("pt-BR"), None);
    }

    #[test]
    fn maps_chinese_variants() {
        assert_eq!(to_plamo_lang("zh-CN"), Some("zh-hans"));
        assert_eq!(to_plamo_lang("zh-hans"), Some("zh-hans"));
        assert_eq!(to_plamo_lang("zh-TW"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-HK"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-hant"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-xx"), None);
    }

    #[test]
    fn reduces_region_codes_to_the_language() {
        assert_eq!(to_plamo_lang("en-US"), Some("en"));
        assert_eq!(to_plamo_lang("EN-GB"), Some("en"));
        assert_eq!(to_plamo_lang("ja-JP"), Some("ja"));
        assert_eq!(to_plamo_lang("fr_CA"), Some("fr"));
    }

    #[test]
    fn trims_and_ignores_case() {
        assert_eq!(to_plamo_lang("  JA "), Some("ja"));
    }

    #[test]
    fn rejects_blank_and_unknown_codes() {
        assert_eq!(to_plamo_lang(""), None);
        assert_eq!(to_plamo_lang("   "), None);
        assert_eq!(to_plamo_lang("xx"), None);
        assert_eq!(to_plamo_lang("tlh"), None);
    }
}
