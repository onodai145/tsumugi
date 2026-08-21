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

/// 起動時のギャップ埋め結果をまとめて反映する（通知は鳴らさない・出入りの都度イベントにしない）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnGapFill {
    pub column_id: String,
    pub notes: Vec<Note>,
    /// newest_known_id に追いつく前に gap_fill_limit 等で打ち切られた場合 true。
    pub truncated: bool,
    /// truncated=true のとき、続きを取得する際に fetch_backfill の until_id に使う境界ノートid。
    pub boundary_id: Option<String>,
    /// truncated=true のときの到達目標(元のキャッシュ最新ノートid)。
    pub target_id: Option<String>,
}

/// 通知カラムに新規通知を追加する。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNotification {
    pub column_id: String,
    pub notification: Notification,
}

/// 再接続時の通知ギャップ埋め結果をまとめて反映する(Issue #147)。ノートの ColumnGapFill と
/// 同じ設計判断で、通知音・デスクトップ通知は鳴らさない（瞬断中に溜まった通知で誤爆しないため）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNotificationGapFill {
    pub column_id: String,
    pub notifications: Vec<Notification>,
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
