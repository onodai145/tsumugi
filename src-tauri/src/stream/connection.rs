//! Connection Manager: アカウント毎に 1 WebSocket を張り、チャンネルを id で多重化する。
//! 切断は前提として、指数バックオフで再接続する（設計書§6）。
//!
//! homeTimeline を購読しつつ、表示中ノートを subNote でキャプチャして、他者のリアクション/
//! 投票/削除を `ColumnNoteUpdated` イベントで反映する（NQL§6: 値は更新するが出入りしない）。

use crate::domain::Note;
use crate::events::{ColumnConnectionState, ColumnNote, ColumnNoteUpdated, ConnectionState, NoteUpdate};
use crate::stream::inbox::Dedup;
use crate::stream::protocol::{self, Incoming};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event as _;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use std::collections::HashMap;

const DEDUP_CAPACITY: usize = 512;
const CAPTURE_CAP: usize = 128; // 1接続で subNote 購読するノート上限
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// カラム(=1チャンネル接続)へ外から送る指示。
pub enum StreamCommand {
    Capture(Vec<String>),
    Uncapture(Vec<String>),
}

struct StreamCtl {
    cancel: watch::Sender<bool>,
    cmd: mpsc::Sender<StreamCommand>,
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
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let col = column_id.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_channel(app, col, host, token, "homeTimeline", cancel_rx, cmd_rx).await;
        });
        self.streams.lock().unwrap().insert(
            column_id,
            StreamCtl {
                cancel: cancel_tx,
                cmd: cmd_tx,
                handle,
            },
        );
    }

    /// 指定カラムで note をキャプチャ購読する（表示中ノートのリアクションを追う）。
    pub fn capture(&self, column_id: &str, note_ids: Vec<String>) {
        if let Some(ctl) = self.streams.lock().unwrap().get(column_id) {
            let _ = ctl.cmd.try_send(StreamCommand::Capture(note_ids));
        }
    }

    pub fn uncapture(&self, column_id: &str, note_ids: Vec<String>) {
        if let Some(ctl) = self.streams.lock().unwrap().get(column_id) {
            let _ = ctl.cmd.try_send(StreamCommand::Uncapture(note_ids));
        }
    }

    /// ストリームを閉じる（購読解除）。存在しなければ何もしない。
    pub fn close(&self, column_id: &str) {
        if let Some(ctl) = self.streams.lock().unwrap().remove(column_id) {
            let _ = ctl.cancel.send(true);
            ctl.handle.abort();
        }
    }

    #[allow(dead_code)]
    pub fn open_count(&self) -> usize {
        self.streams.lock().unwrap().len()
    }
}

/// subNote 購読中のノート集合（上限付き・FIFO で追い出し）。
struct CaptureSet {
    order: VecDeque<String>,
    set: HashSet<String>,
    cap: usize,
}

impl CaptureSet {
    fn new(cap: usize) -> Self {
        Self {
            order: VecDeque::new(),
            set: HashSet::new(),
            cap: cap.max(1),
        }
    }

    /// 追加。新規なら Some(evicted?) を返す（evicted は上限超過で追い出した id）。既知なら None。
    fn add(&mut self, id: &str) -> Option<Option<String>> {
        if self.set.contains(id) {
            return None;
        }
        self.set.insert(id.to_string());
        self.order.push_back(id.to_string());
        let evicted = if self.order.len() > self.cap {
            self.order.pop_front().inspect(|old| {
                self.set.remove(old);
            })
        } else {
            None
        };
        Some(evicted)
    }

    fn remove(&mut self, id: &str) -> bool {
        if self.set.remove(id) {
            self.order.retain(|x| x != id);
            true
        } else {
            false
        }
    }

    fn ids(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

enum RunOutcome {
    Cancelled,
    Disconnected,
    #[allow(dead_code)]
    Fatal,
}

/// 1チャンネルの接続ループ（再接続込み）。キャプチャ集合は再接続をまたいで保持し、
/// 接続確立時に再購読する。
async fn run_channel(
    app: AppHandle,
    column_id: String,
    host: String,
    token: String,
    channel: &str,
    mut cancel: watch::Receiver<bool>,
    mut cmd_rx: mpsc::Receiver<StreamCommand>,
) {
    let mut dedup = Dedup::new(DEDUP_CAPACITY);
    let mut capture = CaptureSet::new(CAPTURE_CAP);
    let mut backoff = BACKOFF_START;

    loop {
        if *cancel.borrow() {
            return;
        }
        emit_state(&app, &column_id, ConnectionState::Connecting);

        let outcome = connect_and_run(
            &app,
            &column_id,
            &host,
            &token,
            channel,
            &mut dedup,
            &mut capture,
            &mut cancel,
            &mut cmd_rx,
        )
        .await;

        match outcome {
            RunOutcome::Cancelled => return,
            RunOutcome::Disconnected => emit_state(&app, &column_id, ConnectionState::Reconnecting),
            RunOutcome::Fatal => emit_state(&app, &column_id, ConnectionState::Error),
        }

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = cancel.changed() => { if *cancel.borrow() { return; } }
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

#[allow(clippy::too_many_arguments)]
async fn connect_and_run(
    app: &AppHandle,
    column_id: &str,
    host: &str,
    token: &str,
    channel: &str,
    dedup: &mut Dedup,
    capture: &mut CaptureSet,
    cancel: &mut watch::Receiver<bool>,
    cmd_rx: &mut mpsc::Receiver<StreamCommand>,
) -> RunOutcome {
    let url = format!("wss://{host}/streaming?i={token}");
    let ws = match tokio_tungstenite::connect_async(&url).await {
        Ok((ws, _resp)) => ws,
        Err(e) => {
            log::warn!("[{column_id}] ws connect failed: {e}");
            return RunOutcome::Disconnected;
        }
    };
    let (mut write, mut read): (SplitSink<Ws, Message>, SplitStream<Ws>) = ws.split();

    // チャンネル購読（接続ごとに新規 id）
    let sub_id = uuid::Uuid::new_v4().to_string();
    if write
        .send(Message::Text(
            protocol::connect(channel, &sub_id, serde_json::json!({})).into(),
        ))
        .await
        .is_err()
    {
        return RunOutcome::Disconnected;
    }
    // 再接続時: キャプチャ中ノートを再購読
    for id in capture.ids() {
        let _ = write.send(Message::Text(protocol::sub_note(id).into())).await;
    }
    emit_state(app, column_id, ConnectionState::Connected);

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    let _ = write.send(Message::Text(protocol::disconnect(&sub_id).into())).await;
                    let _ = write.close().await;
                    return RunOutcome::Cancelled;
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(StreamCommand::Capture(ids)) => {
                        for id in ids {
                            if let Some(evicted) = capture.add(&id) {
                                let _ = write.send(Message::Text(protocol::sub_note(&id).into())).await;
                                if let Some(old) = evicted {
                                    let _ = write.send(Message::Text(protocol::unsub_note(&old).into())).await;
                                }
                            }
                        }
                    }
                    Some(StreamCommand::Uncapture(ids)) => {
                        for id in ids {
                            if capture.remove(&id) {
                                let _ = write.send(Message::Text(protocol::unsub_note(&id).into())).await;
                            }
                        }
                    }
                    None => { /* 送信側が閉じた: 無視 */ }
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(sub) = handle_text(app, column_id, &text, dedup) {
                            // 新規ノートは自動キャプチャ
                            if let Some(evicted) = capture.add(&sub) {
                                let _ = write.send(Message::Text(protocol::sub_note(&sub).into())).await;
                                if let Some(old) = evicted {
                                    let _ = write.send(Message::Text(protocol::unsub_note(&old).into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        let _ = write.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => return RunOutcome::Disconnected,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("[{column_id}] ws read error: {e}");
                        return RunOutcome::Disconnected;
                    }
                }
            }
        }
    }
}

/// テキストフレームを解釈。新規ノートなら emit して、その note_id を返す（自動キャプチャ用）。
/// noteUpdated なら ColumnNoteUpdated を emit する。
fn handle_text(app: &AppHandle, column_id: &str, text: &str, dedup: &mut Dedup) -> Option<String> {
    match protocol::parse_incoming(text) {
        Incoming::ChannelNote { note, .. } => {
            if dedup.accept(&note.id) {
                let id = note.id.clone();
                let normalized: Note = (*note).into();
                let _ = ColumnNote {
                    column_id: column_id.to_string(),
                    note: normalized,
                }
                .emit(app);
                return Some(id);
            }
            None
        }
        Incoming::NoteUpdated { note_id, kind, body } => {
            if let Some((update, actor_id)) = map_note_update(&kind, &body) {
                let _ = ColumnNoteUpdated {
                    column_id: column_id.to_string(),
                    note_id,
                    update,
                    actor_id,
                }
                .emit(app);
            }
            None
        }
        Incoming::Other => None,
    }
}

/// noteUpdated の kind/body を型付き NoteUpdate に落とす。
fn map_note_update(kind: &str, body: &Value) -> Option<(NoteUpdate, Option<String>)> {
    let actor = body.get("userId").and_then(Value::as_str).map(str::to_string);
    match kind {
        "reacted" => {
            let reaction = body.get("reaction").and_then(Value::as_str)?.to_string();
            Some((NoteUpdate::Reacted { reaction }, actor))
        }
        "unreacted" => {
            let reaction = body.get("reaction").and_then(Value::as_str)?.to_string();
            Some((NoteUpdate::Unreacted { reaction }, actor))
        }
        "pollVoted" => {
            let choice = body.get("choice").and_then(Value::as_u64)? as u32;
            Some((NoteUpdate::PollVoted { choice }, actor))
        }
        "deleted" => Some((NoteUpdate::Deleted, None)),
        _ => None,
    }
}

fn emit_state(app: &AppHandle, column_id: &str, state: ConnectionState) {
    let _ = ColumnConnectionState {
        column_id: column_id.to_string(),
        state,
    }
    .emit(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capture_set_evicts_fifo() {
        let mut c = CaptureSet::new(2);
        assert_eq!(c.add("a"), Some(None));
        assert_eq!(c.add("b"), Some(None));
        assert_eq!(c.add("a"), None); // 既知
        assert_eq!(c.add("c"), Some(Some("a".to_string()))); // a を追い出す
        assert!(c.remove("b"));
        assert!(!c.remove("zzz"));
    }

    #[test]
    fn map_reacted_and_actor() {
        let (u, actor) = map_note_update("reacted", &json!({"reaction":"👍","userId":"u2"})).unwrap();
        assert!(matches!(u, NoteUpdate::Reacted { reaction } if reaction == "👍"));
        assert_eq!(actor.as_deref(), Some("u2"));
    }

    #[test]
    fn map_pollvoted_and_deleted() {
        let (u, _) = map_note_update("pollVoted", &json!({"choice":1,"userId":"u"})).unwrap();
        assert!(matches!(u, NoteUpdate::PollVoted { choice } if choice == 1));
        let (u, actor) = map_note_update("deleted", &json!({"deletedAt":"x"})).unwrap();
        assert!(matches!(u, NoteUpdate::Deleted));
        assert!(actor.is_none());
    }

    #[test]
    fn map_unknown_is_none() {
        assert!(map_note_update("emojiAdded", &json!({})).is_none());
        assert!(map_note_update("reacted", &json!({})).is_none()); // reaction 欠落
    }
}
