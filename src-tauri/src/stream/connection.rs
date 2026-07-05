//! Connection Manager: アカウント毎に 1 WebSocket を張り、チャンネルを id で多重化する。
//! 切断は前提として、指数バックオフで再接続する（設計書§6）。
//!
//! Phase 2 のスコープ: homeTimeline チャンネルを購読し、受信ノートを正規化・重複排除して
//! `ColumnNote` イベントで push、接続状態を `ColumnConnectionState` で通知する。

use crate::api::normalize::RawNote;
use crate::domain::Note;
use crate::events::{ColumnConnectionState, ColumnNote, ConnectionState};
use crate::stream::inbox::Dedup;
use crate::stream::protocol::{self, Incoming};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event as _;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

const DEDUP_CAPACITY: usize = 512;
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// カラム(=1チャンネル接続)を制御するハンドル。
struct StreamCtl {
    cancel: watch::Sender<bool>,
    handle: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
pub struct ConnectionManager {
    streams: Mutex<HashMap<String, StreamCtl>>,
}

impl ConnectionManager {
    /// homeTimeline を購読するストリームを開く。同じ column_id が既にあれば張り替える。
    pub fn open_home(&self, app: AppHandle, column_id: String, host: String, token: String) {
        self.close(&column_id);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let col = column_id.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_channel(app, col, host, token, "homeTimeline", cancel_rx).await;
        });
        self.streams.lock().unwrap().insert(
            column_id,
            StreamCtl {
                cancel: cancel_tx,
                handle,
            },
        );
    }

    /// ストリームを閉じる（購読解除）。存在しなければ何もしない。
    pub fn close(&self, column_id: &str) {
        if let Some(ctl) = self.streams.lock().unwrap().remove(column_id) {
            let _ = ctl.cancel.send(true);
            ctl.handle.abort();
        }
    }

    /// 開いているカラム数（テスト・診断用）。
    #[allow(dead_code)]
    pub fn open_count(&self) -> usize {
        self.streams.lock().unwrap().len()
    }
}

/// 1チャンネルの接続ループ（再接続込み）。cancel が true になったら終了する。
async fn run_channel(
    app: AppHandle,
    column_id: String,
    host: String,
    token: String,
    channel: &str,
    mut cancel: watch::Receiver<bool>,
) {
    let mut dedup = Dedup::new(DEDUP_CAPACITY);
    let mut backoff = BACKOFF_START;

    loop {
        if *cancel.borrow() {
            return;
        }
        emit_state(&app, &column_id, ConnectionState::Connecting);

        match connect_and_run(&app, &column_id, &host, &token, channel, &mut dedup, &mut cancel)
            .await
        {
            RunOutcome::Cancelled => return,
            RunOutcome::Disconnected => {
                emit_state(&app, &column_id, ConnectionState::Reconnecting);
            }
            RunOutcome::Fatal => {
                emit_state(&app, &column_id, ConnectionState::Error);
            }
        }

        // バックオフ付き再接続（cancel が来たら即抜ける）
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = cancel.changed() => { if *cancel.borrow() { return; } }
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

enum RunOutcome {
    Cancelled,
    Disconnected,
    /// 再接続しても無駄な致命的失敗（認証エラー等）。Phase 2 後半で分類予定。
    #[allow(dead_code)]
    Fatal,
}

async fn connect_and_run(
    app: &AppHandle,
    column_id: &str,
    host: &str,
    token: &str,
    channel: &str,
    dedup: &mut Dedup,
    cancel: &mut watch::Receiver<bool>,
) -> RunOutcome {
    let url = format!("wss://{host}/streaming?i={token}");
    let mut ws = match tokio_tungstenite::connect_async(&url).await {
        Ok((ws, _resp)) => ws,
        Err(e) => {
            log::warn!("[{column_id}] ws connect failed: {e}");
            return RunOutcome::Disconnected;
        }
    };

    // チャンネル購読を送信（接続ごとに新規 id）
    let sub_id = uuid::Uuid::new_v4().to_string();
    if ws
        .send(Message::Text(
            protocol::connect(channel, &sub_id, serde_json::json!({})).into(),
        ))
        .await
        .is_err()
    {
        return RunOutcome::Disconnected;
    }
    emit_state(app, column_id, ConnectionState::Connected);

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    let _ = ws.send(Message::Text(protocol::disconnect(&sub_id).into())).await;
                    let _ = ws.close(None).await;
                    return RunOutcome::Cancelled;
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_text(app, column_id, &text, dedup);
                    }
                    Some(Ok(Message::Ping(p))) => {
                        let _ = ws.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return RunOutcome::Disconnected;
                    }
                    Some(Ok(_)) => { /* Binary/Pong 等は無視 */ }
                    Some(Err(e)) => {
                        log::warn!("[{column_id}] ws read error: {e}");
                        return RunOutcome::Disconnected;
                    }
                }
            }
        }
    }
}

/// テキストフレームを解釈し、新規ノートなら正規化して emit する。
fn handle_text(app: &AppHandle, column_id: &str, text: &str, dedup: &mut Dedup) {
    if let Incoming::ChannelNote { note, .. } = protocol::parse_incoming(text) {
        if dedup.accept(&note.id) {
            let normalized: Note = (*note).into();
            let _ = ColumnNote {
                column_id: column_id.to_string(),
                note: normalized,
            }
            .emit(app);
        }
    }
    // NoteUpdated（キャプチャ更新）は Phase 2 後半で ColumnNoteUpdated として扱う
}

fn emit_state(app: &AppHandle, column_id: &str, state: ConnectionState) {
    let _ = ColumnConnectionState {
        column_id: column_id.to_string(),
        state,
    }
    .emit(app);
}

/// `RawNote` を正規化する薄いヘルパ（テストから直接叩けるよう分離）。
#[allow(dead_code)]
pub fn normalize_note(raw: RawNote) -> Note {
    raw.into()
}
