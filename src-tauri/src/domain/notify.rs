use serde::{Deserialize, Serialize};
use specta::Type;

/// デスクトップ通知・通知音の設定（グローバル）。通知カラムに新着が来たとき使う。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfig {
    /// OS のデスクトップ通知を出す
    pub desktop: bool,
    /// 通知音を鳴らす
    pub sound: bool,
    /// 通知音の選択。プリセットID("beep"/"chime"/"ping"/"pop")か、data URL(カスタム音声ファイル)。
    /// 空文字なら既定プリセット("beep")を使う。タブ側で個別指定が無い場合のフォールバック。
    #[serde(default)]
    pub sound_choice: String,
}
