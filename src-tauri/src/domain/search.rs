//! サーバーサイド検索(Issue #430)で、接続先サーバーが対応する検索機能の判定。

use serde::Serialize;
use specta::Type;

/// `notes/search` の日時範囲(`sinceDate`/`untilDate`)が入った Misskey のバージョン。
const DATE_RANGE_MIN_VERSION: (u32, u32, u32) = (2025, 7, 0);

/// 接続先サーバーが対応する検索機能。フロントはこれを見て入力欄の出し分けをする。
/// 将来 `/api.json` から実際の対応パラメータで判定する方式へ変えても、この型を介せば
/// 呼び出し側(コマンド/フロント)は変更不要。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchCapabilities {
    /// 日時範囲の指定に対応しているか。
    pub date_range: bool,
}

/// Misskey のバージョン文字列から先頭の `YYYY.M.P` だけを読む。
/// `-alpha.0` や `-io.12b-...` などのサフィックスは無視する。形式が合わなければ None。
pub fn parse_misskey_version(version: &str) -> Option<(u32, u32, u32)> {
    let core = version.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// バージョンから対応機能を決める。None・パース不能は安全側（非対応）に倒す。
pub fn search_capabilities(version: Option<&str>) -> SearchCapabilities {
    let date_range = version
        .and_then(parse_misskey_version)
        .is_some_and(|v| v >= DATE_RANGE_MIN_VERSION);
    SearchCapabilities { date_range }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_suffixed_versions() {
        assert_eq!(parse_misskey_version("2026.9.1"), Some((2026, 9, 1)));
        assert_eq!(parse_misskey_version("2026.10.0-alpha.0"), Some((2026, 10, 0)));
        // フォーク(misskey.io 等)のサフィックスは無視して先頭の YYYY.M.P だけ読む
        assert_eq!(parse_misskey_version("2025.4.1-io.12b-fb6fbea074"), Some((2025, 4, 1)));
        assert_eq!(parse_misskey_version("2025.7.0+build.5"), Some((2025, 7, 0)));
        assert_eq!(parse_misskey_version(" 2025.7.0 "), Some((2025, 7, 0)));
    }

    #[test]
    fn rejects_unparseable_versions() {
        for v in ["", "abc", "2025", "2025.7", "2025.x.0", "v2025.7.0", "-alpha"] {
            assert_eq!(parse_misskey_version(v), None, "{v:?} should not parse");
        }
    }

    #[test]
    fn date_range_requires_2025_7_0_or_later() {
        assert!(!search_capabilities(Some("2025.6.4")).date_range);
        assert!(search_capabilities(Some("2025.7.0")).date_range);
        assert!(search_capabilities(Some("2026.9.1")).date_range);
        assert!(search_capabilities(Some("2026.10.0-alpha.0")).date_range);
        // フォークの古い番号は非対応扱い
        assert!(!search_capabilities(Some("2025.4.1-io.12b-fb6fbea074")).date_range);
    }

    #[test]
    fn unknown_or_unparseable_version_means_no_date_range() {
        assert!(!search_capabilities(None).date_range);
        assert!(!search_capabilities(Some("garbage")).date_range);
    }

    #[test]
    fn serializes_as_camel_case_for_the_frontend() {
        let v = serde_json::to_value(SearchCapabilities { date_range: true }).unwrap();
        assert_eq!(v, serde_json::json!({ "dateRange": true }));
    }
}
