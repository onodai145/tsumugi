//! 予約投稿(Issue #60)で、接続先サーバーが対応するかの判定。

use super::search::parse_misskey_version;
use serde::Serialize;
use specta::Type;

/// 本家 Misskey で予約投稿(`notes/drafts` の `scheduledAt`)が入った最初のリリース。
/// 上流 PR #16577(2025-09-26 マージ)は 2025.10.0-alpha.0 から含まれ、2025.9.0 には含まれない。
const SCHEDULE_MIN_VERSION: (u32, u32, u32) = (2025, 10, 0);

/// 接続先サーバーが対応する予約投稿機能。フロントはこれを見て予約ボタンの出し分けをする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleCapabilities {
    /// 予約投稿が使えるか。
    pub available: bool,
}

/// バージョンから対応可否を決める。None・パース不能は安全側(非対応)に倒す。
pub fn schedule_capabilities(version: Option<&str>) -> ScheduleCapabilities {
    let available = version
        .and_then(parse_misskey_version)
        .is_some_and(|v| v >= SCHEDULE_MIN_VERSION);
    ScheduleCapabilities { available }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_from_2025_10_0() {
        assert!(!schedule_capabilities(Some("2025.9.0")).available);
        assert!(schedule_capabilities(Some("2025.10.0")).available);
        assert!(schedule_capabilities(Some("2025.10.0-alpha.0")).available);
        assert!(schedule_capabilities(Some("2026.9.1")).available);
        assert!(schedule_capabilities(Some("2026.10.0")).available);
    }

    #[test]
    fn misskeyio_fork_version_is_unavailable() {
        // MisskeyIO フォークの旧版は notes/drafts の予約方式を持たない
        assert!(!schedule_capabilities(Some("2025.4.1-io.12b-fb6fbea074")).available);
    }

    #[test]
    fn unknown_or_unparseable_version_is_unavailable() {
        assert!(!schedule_capabilities(None).available);
        for v in ["", "unknown", "2025", "2025.10", "v2025.10.0"] {
            assert!(!schedule_capabilities(Some(v)).available, "{v:?} should be unavailable");
        }
    }
}
