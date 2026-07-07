//! Rust → フロントの通知イベント（tauri-specta Event）。設計書§9 / phase0-scaffold §3.2。
//! ペイロードに token は含めない。

use crate::domain::{Note, Notification};
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

/// 通知カラムに新規通知を追加する。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNotification {
    pub column_id: String,
    pub notification: Notification,
}

/// キャプチャ中ノートの更新（他者のリアクション/投票/削除）。値のみ更新し、
/// カラムからの出入りはしない（TQL§6 の方針）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNoteUpdated {
    pub column_id: String,
    pub note_id: String,
    pub update: NoteUpdate,
    /// 更新を起こしたユーザ（自分の操作は楽観的更新済みなのでフロントで無視するため）
    pub actor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NoteUpdate {
    Reacted { reaction: String },
    Unreacted { reaction: String },
    PollVoted { choice: u32 },
    Deleted,
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
