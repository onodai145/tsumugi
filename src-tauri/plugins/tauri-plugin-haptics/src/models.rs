use serde::{Deserialize, Serialize};
use specta::Type;

/// 振動パターン。値はAndroid Kotlin側(HapticsPlugin.kt)のVibrationEffectマッピングと対応する。
/// Light/Medium のみ現時点で呼び出し元があり、Success/Warning/Errorは将来の用途
/// (投稿失敗フィードバック、破壊的操作の確認など)を見越した先行定義（Issue #26設計参照）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HapticPattern {
    Light,
    Medium,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VibrateRequest {
    pub pattern: HapticPattern,
}

/// 触覚入りの通知音の再生要求(Issue #592)。`data` は音声のバイト列の base64。
/// モバイルでしか使わないので、デスクトップでは dead_code にならないよう cfg で絞る。
#[cfg(mobile)]
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayHapticAudioRequest {
    pub data: String,
    pub haptics_enabled: bool,
}
