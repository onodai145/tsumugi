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
}
