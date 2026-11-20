//! 読み上げ(Issue #625)の設定の型と検証。
//! カラム単位の `ColumnSpeech` は `Column.speech` に、全体の `SpeechSettings` は `SettingsData.speech` に置く。

use serde::{Deserialize, Serialize};
use specta::Type;

pub const QUEUE_LIMIT_MIN: u8 = 1;
pub const QUEUE_LIMIT_MAX: u8 = 20;
pub const QUEUE_LIMIT_DEFAULT: u8 = 5;
pub const SPEED_MIN: f32 = 0.5;
pub const SPEED_MAX: f32 = 2.0;
pub const VOLUME_MIN: f32 = 0.1;
pub const VOLUME_MAX: f32 = 2.0;
/// 声の ID の最大長(エンジン固有の不透明な文字列。VOICEVOX ではスタイルの数値 ID)。
pub const VOICE_ID_MAX_LEN: usize = 64;

fn default_true() -> bool {
    true
}
fn default_queue_limit() -> u8 {
    QUEUE_LIMIT_DEFAULT
}
fn default_one() -> f32 {
    1.0
}

/// カラム(タブ)単位の読み上げ設定。`Column.speech`。旧い設定ファイルには無いので、既定値(オフ)で読む。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSpeech {
    /// このカラムを読み上げる。
    #[serde(default)]
    pub enabled: bool,
    /// 投稿者名(`{表示名}さん、`)を前置きする。
    #[serde(default = "default_true")]
    pub speak_author: bool,
    /// このカラムの待ち件数の上限。超えたら、このカラムの最古の待ち項目を捨てる。
    #[serde(default = "default_queue_limit")]
    pub queue_limit: u8,
    /// このカラムの声。空ならグローバルの既定の声を継承する。
    #[serde(default)]
    pub voice_id: String,
}

impl Default for ColumnSpeech {
    fn default() -> Self {
        Self { enabled: false, speak_author: true, queue_limit: QUEUE_LIMIT_DEFAULT, voice_id: String::new() }
    }
}

impl ColumnSpeech {
    /// 保存・インポート前に範囲へ丸める。長すぎる声の ID は(切り詰めると別の ID になるので)空に戻す。
    pub fn normalized(mut self) -> Self {
        self.queue_limit = self.queue_limit.clamp(QUEUE_LIMIT_MIN, QUEUE_LIMIT_MAX);
        if self.voice_id.chars().count() > VOICE_ID_MAX_LEN {
            self.voice_id = String::new();
        }
        self
    }
}

/// 読み上げに使うエンジンの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SpeechEngineKind {
    /// 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)に HTTP で接続する。
    #[default]
    External,
    /// 内蔵の VOICEVOX CORE(デスクトップのみ。エンジン一式と声は、設定画面から取得する)。Issue #625 段階2。
    VoicevoxCore,
}

/// 知らないエンジンの種類(将来のエンジン・手編集)は `External` として読む。設定ファイル全体が読めなくなるのを避ける。
/// (`deserialize_with` は specta の TS 出力が受け付けないので、`Deserialize` を手で実装する。)
impl<'de> Deserialize<'de> for SpeechEngineKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "voicevoxCore" => Self::VoicevoxCore,
            _ => Self::External,
        })
    }
}

/// 全体の読み上げ設定。`SettingsData.speech`。**エクスポートには含めない**。インポートでは現在の値を維持する。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSettings {
    /// 全体のマスタースイッチ。
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub engine: SpeechEngineKind,
    /// 外部エンジンの URL(例 `http://192.168.1.10:50021`)。空は未設定。
    #[serde(default)]
    pub external_url: String,
    /// 既定の声(エンジン固有の ID)。
    #[serde(default)]
    pub default_voice_id: String,
    #[serde(default = "default_one")]
    pub speed_scale: f32,
    #[serde(default = "default_one")]
    pub volume: f32,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: SpeechEngineKind::External,
            external_url: String::new(),
            default_voice_id: String::new(),
            speed_scale: 1.0,
            volume: 1.0,
        }
    }
}

/// 外部エンジンの URL を検証して返す。`http` / `https` のみ・ホストあり・認証情報なし。
/// 末尾のスラッシュは(パスの連結を単純にするため)呼び出し側で揃える。
pub fn parse_base_url(raw: &str) -> Result<url::Url, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("URL が空です".into());
    }
    let u = url::Url::parse(raw).map_err(|_| "URL の形式が正しくありません".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("URL は http:// か https:// で始めてください".into());
    }
    if u.host_str().is_none() {
        return Err("URL にホストがありません".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("URL にユーザー名・パスワードは含められません".into());
    }
    // 基準 URL にパスを連結するので、クエリやフラグメントがあると壊れた URL(`…?x=1/audio_query`)になる。
    if u.query().is_some() || u.fragment().is_some() {
        return Err("URL に ? や # は含められません".into());
    }
    Ok(u)
}

impl SpeechSettings {
    /// 保存してよい値か。URL が空(未設定)は有効。範囲外・不正な URL は理由つきで拒否する。
    pub fn validate(&self) -> Result<(), String> {
        if !self.external_url.trim().is_empty() {
            parse_base_url(&self.external_url)?;
        }
        // NaN は範囲に入らないので、ここで弾かれる。
        if !(SPEED_MIN..=SPEED_MAX).contains(&self.speed_scale) {
            return Err(format!("話速は {SPEED_MIN}〜{SPEED_MAX} の範囲で指定してください"));
        }
        if !(VOLUME_MIN..=VOLUME_MAX).contains(&self.volume) {
            return Err(format!("音量は {VOLUME_MIN}〜{VOLUME_MAX} の範囲で指定してください"));
        }
        if self.default_voice_id.chars().count() > VOICE_ID_MAX_LEN {
            return Err("声の ID が長すぎます".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_speech_default_is_off_with_author_and_limit_5() {
        let s = ColumnSpeech::default();
        assert!(!s.enabled);
        assert!(s.speak_author);
        assert_eq!(s.queue_limit, QUEUE_LIMIT_DEFAULT);
        assert_eq!(s.voice_id, "");
    }

    #[test]
    fn column_speech_missing_fields_fall_back_to_defaults() {
        let s: ColumnSpeech = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ColumnSpeech::default());
    }

    #[test]
    fn column_speech_normalized_clamps_queue_limit_and_drops_overlong_voice_id() {
        let s = ColumnSpeech { queue_limit: 0, voice_id: "x".repeat(VOICE_ID_MAX_LEN + 1), ..ColumnSpeech::default() }.normalized();
        assert_eq!(s.queue_limit, QUEUE_LIMIT_MIN);
        assert_eq!(s.voice_id, "");
        let s = ColumnSpeech { queue_limit: 200, voice_id: "12".into(), ..ColumnSpeech::default() }.normalized();
        assert_eq!(s.queue_limit, QUEUE_LIMIT_MAX);
        assert_eq!(s.voice_id, "12");
    }

    #[test]
    fn the_engine_kind_round_trips_as_camel_case() {
        assert_eq!(serde_json::to_string(&SpeechEngineKind::VoicevoxCore).unwrap(), "\"voicevoxCore\"");
        let s: SpeechSettings = serde_json::from_str(r#"{"engine":"voicevoxCore"}"#).unwrap();
        assert_eq!(s.engine, SpeechEngineKind::VoicevoxCore);
        assert!(s.validate().is_ok(), "内蔵は URL が空でも有効");
    }

    #[test]
    fn an_unknown_engine_kind_falls_back_to_external_instead_of_failing_the_whole_file() {
        // 将来のエンジンや、設定ファイルの手編集で、知らない値が入っていても、設定ファイル全体を読めなくしない。
        let s: SpeechSettings = serde_json::from_str(r#"{"enabled":true,"engine":"someFutureEngine"}"#).unwrap();
        assert_eq!(s.engine, SpeechEngineKind::External);
        assert!(s.enabled, "ほかの項目は読める");
    }

    #[test]
    fn speech_settings_default_is_off_and_unconfigured() {
        let s = SpeechSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.engine, SpeechEngineKind::External);
        assert_eq!(s.external_url, "");
        assert_eq!(s.speed_scale, 1.0);
        assert_eq!(s.volume, 1.0);
        assert!(s.validate().is_ok(), "未設定(URL が空)は有効な設定");
    }

    #[test]
    fn speech_settings_missing_fields_fall_back_to_defaults() {
        let s: SpeechSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, SpeechSettings::default());
    }

    #[test]
    fn url_must_be_http_or_https_with_a_host_and_no_credentials() {
        let ok = |u: &str| SpeechSettings { external_url: u.into(), ..Default::default() }.validate();
        assert!(ok("http://192.168.1.10:50021").is_ok());
        assert!(ok("https://tts.example.com/").is_ok());
        assert!(ok("  http://127.0.0.1:50021  ").is_ok(), "前後の空白は許す");
        assert!(ok("ftp://example.com").is_err());
        assert!(ok("file:///etc/passwd").is_err());
        assert!(ok("javascript:alert(1)").is_err());
        assert!(ok("not a url").is_err());
        assert!(ok("http://user:pass@example.com").is_err());
    }

    #[test]
    fn speed_volume_and_voice_id_ranges_are_enforced() {
        let with = |f: &dyn Fn(&mut SpeechSettings)| {
            let mut s = SpeechSettings::default();
            f(&mut s);
            s.validate()
        };
        assert!(with(&|s| s.speed_scale = 0.4).is_err());
        assert!(with(&|s| s.speed_scale = 2.1).is_err());
        assert!(with(&|s| s.speed_scale = f32::NAN).is_err());
        assert!(with(&|s| s.speed_scale = 2.0).is_ok());
        assert!(with(&|s| s.volume = 0.0).is_err());
        assert!(with(&|s| s.volume = 2.5).is_err());
        assert!(with(&|s| s.default_voice_id = "x".repeat(VOICE_ID_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn a_base_url_with_a_query_or_fragment_is_rejected() {
        // `http://h:1/?x=1` に `/audio_query` を連結すると、壊れた URL になる。
        assert!(parse_base_url("http://h:1/?x=1").is_err());
        assert!(parse_base_url("http://h:1?x=1").is_err());
        assert!(parse_base_url("http://h:1/#frag").is_err());
        assert!(parse_base_url("http://h:1/api/").is_ok(), "パスは許す(リバースプロキシの配下など)");
        assert!(SpeechSettings { external_url: "http://h:1/?x=1".into(), ..Default::default() }.validate().is_err());
    }

    #[test]
    fn parse_base_url_trims_and_strips_trailing_slashes() {
        assert_eq!(parse_base_url(" http://h:1/ ").unwrap().as_str(), "http://h:1/");
        assert!(parse_base_url("").is_err());
    }
}
