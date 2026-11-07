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

/// ユーザーミュートの期間(Misskey Web の選択肢に合わせる)。Misskey に送る期限(`expiresAt`)は
/// この値から Rust 側で計算する。フロントの時計に依存せず、過去の `expiresAt`(サーバーは黙って無視する)
/// を送らないため。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MutePeriod {
    Indefinitely,
    TenMinutes,
    OneHour,
    OneDay,
    OneWeek,
}

impl MutePeriod {
    /// `now_ms`(エポックミリ秒)からの期限。無期限は `None`(Misskey では `expiresAt: null`)。
    pub fn expires_at_ms(self, now_ms: i64) -> Option<i64> {
        const MINUTE: i64 = 60 * 1000;
        let length = match self {
            Self::Indefinitely => return None,
            Self::TenMinutes => 10 * MINUTE,
            Self::OneHour => 60 * MINUTE,
            Self::OneDay => 24 * 60 * MINUTE,
            Self::OneWeek => 7 * 24 * 60 * MINUTE,
        };
        Some(now_ms + length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn indefinitely_has_no_expiry() {
        assert_eq!(MutePeriod::Indefinitely.expires_at_ms(NOW), None);
    }

    #[test]
    fn each_period_adds_its_length_in_milliseconds() {
        assert_eq!(MutePeriod::TenMinutes.expires_at_ms(NOW), Some(NOW + 10 * 60 * 1000));
        assert_eq!(MutePeriod::OneHour.expires_at_ms(NOW), Some(NOW + 60 * 60 * 1000));
        assert_eq!(MutePeriod::OneDay.expires_at_ms(NOW), Some(NOW + 24 * 60 * 60 * 1000));
        assert_eq!(MutePeriod::OneWeek.expires_at_ms(NOW), Some(NOW + 7 * 24 * 60 * 60 * 1000));
    }

    #[test]
    fn serializes_as_camel_case_matching_the_frontend_values() {
        assert_eq!(serde_json::to_string(&MutePeriod::TenMinutes).unwrap(), "\"tenMinutes\"");
        assert_eq!(serde_json::to_string(&MutePeriod::Indefinitely).unwrap(), "\"indefinitely\"");
        assert_eq!(serde_json::from_str::<MutePeriod>("\"oneWeek\"").unwrap(), MutePeriod::OneWeek);
    }
}
