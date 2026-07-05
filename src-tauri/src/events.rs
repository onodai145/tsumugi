//! Rust → フロントの通知イベント（tauri-specta Event）。設計書§9 / phase0-scaffold §3.2。
//! ペイロードに token は含めない。

use crate::domain::Note;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// カラムに新規ノートを追加する（フィルタ通過済み）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNote {
    pub column_id: String,
    pub note: Note,
}

/// カラムの接続状態（UI 表示用）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnConnectionState {
    pub column_id: String,
    pub state: ConnectionState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Error,
}
