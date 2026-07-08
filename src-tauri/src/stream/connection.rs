//! Connection Manager: アカウント毎に 1 WebSocket を張り、複数カラムを 1 本の接続に
//! チャンネル id で多重化する（設計書§6）。切断は前提として指数バックオフで再接続する。
//!
//! - 1 アカウント = 1 WebSocket。カラム(タブ)を追加/削除すると、その接続上で
//!   `connect`/`disconnect` メッセージを送ってチャンネルを出し入れする。
//! - 各チャンネルは購読ノートを subNote でキャプチャして、他者のリアクション/投票/削除を
//!   `ColumnNoteUpdated` イベントで反映する（TQL§6: 値は更新するが出入りしない）。
//!   subNote は接続単位のため、noteUpdated はそのノートを表示中の全カラムへ配る。

use crate::domain::{Note, Notification};
use crate::events::{
    ColumnConnectionState, ColumnNote, ColumnNoteUpdated, ColumnNotification, ConnectionState,
    NoteUpdate,
};
use crate::filter::eval::EvalContext;
use crate::filter::CompiledFilter;
use crate::state::AppState;
use crate::stream::inbox::Dedup;
use crate::stream::protocol::{self, Incoming};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Manager as _;
use tauri_specta::Event as _;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const DEDUP_CAPACITY: usize = 512;
const CAPTURE_CAP: usize = 512; // 1接続(=1アカウント)で subNote 購読するノート上限
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// ストリームの扱い方。Notes はフィルタ適用してノートを流し、Notifications は通知を流す。
#[derive(Clone)]
enum StreamMode {
    Notes {
        account_id: String,
        filter: Arc<CompiledFilter>,
        ctx: Arc<EvalContext>,
    },
    Notifications {
        account_id: String,
    },
}

impl StreamMode {
    fn account_id(&self) -> &str {
        match self {
            StreamMode::Notes { account_id, .. } => account_id,
            StreamMode::Notifications { account_id } => account_id,
        }
    }
}

/// アカウント接続タスクへ外から送る指示。
enum AccountCommand {
    /// チャンネル(カラム)を購読に加える。同じ column_id があれば張り替える。
    AddChannel {
        column_id: String,
        channel: String,
        params: Value,
        mode: StreamMode,
    },
    /// チャンネル(カラム)を購読から外す。
    RemoveChannel { column_id: String },
    /// 表示中ノートをキャプチャ購読する。
    Capture {
        column_id: String,
        note_ids: Vec<String>,
    },
    /// キャプチャ解除。
    Uncapture {
        column_id: String,
        note_ids: Vec<String>,
    },
}

struct AccountCtl {
    cmd: mpsc::Sender<AccountCommand>,
    cancel: watch::Sender<bool>,
    handle: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
pub struct ConnectionManager {
    /// account_id -> そのアカウントの WebSocket タスク
    accounts: Mutex<HashMap<String, AccountCtl>>,
    /// column_id -> account_id（close/capture のルーティング用）
    columns: Mutex<HashMap<String, String>>,
}

impl ConnectionManager {
    /// ノートを流すチャンネル(カラム)を開く（フィルタ適用）。
    /// 同じ account の WebSocket が無ければ張り、あれば相乗りする。
    #[allow(clippy::too_many_arguments)]
    pub fn open_channel(
        &self,
        app: AppHandle,
        column_id: String,
        account_id: String,
        host: String,
        token: String,
        channel: &str,
        params: serde_json::Value,
        filter: CompiledFilter,
        ctx: EvalContext,
    ) {
        let mode = StreamMode::Notes {
            account_id: account_id.clone(),
            filter: Arc::new(filter),
            ctx: Arc::new(ctx),
        };
        self.add_channel(
            app,
            column_id,
            account_id,
            host,
            token,
            channel.to_string(),
            params,
            mode,
        );
    }

    /// 通知(main チャンネル)を流すカラムを開く。
    pub fn open_notifications(
        &self,
        app: AppHandle,
        column_id: String,
        account_id: String,
        host: String,
        token: String,
    ) {
        let mode = StreamMode::Notifications {
            account_id: account_id.clone(),
        };
        self.add_channel(
            app,
            column_id,
            account_id,
            host,
            token,
            "main".to_string(),
            serde_json::json!({}),
            mode,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_channel(
        &self,
        app: AppHandle,
        column_id: String,
        account_id: String,
        host: String,
        token: String,
        channel: String,
        params: serde_json::Value,
        mode: StreamMode,
    ) {
        self.columns
            .lock()
            .unwrap()
            .insert(column_id.clone(), account_id.clone());

        let mut accounts = self.accounts.lock().unwrap();
        let ctl = accounts.entry(account_id.clone()).or_insert_with(|| {
            spawn_account(app, account_id.clone(), host, token)
        });
        let _ = ctl.cmd.try_send(AccountCommand::AddChannel {
            column_id,
            channel,
            params,
            mode,
        });
    }

    /// 指定カラムで note をキャプチャ購読する（表示中ノートのリアクションを追う）。
    pub fn capture(&self, column_id: &str, note_ids: Vec<String>) {
        self.send_to_account(column_id, |column_id| AccountCommand::Capture {
            column_id,
            note_ids,
        });
    }

    pub fn uncapture(&self, column_id: &str, note_ids: Vec<String>) {
        self.send_to_account(column_id, |column_id| AccountCommand::Uncapture {
            column_id,
            note_ids,
        });
    }

    fn send_to_account(
        &self,
        column_id: &str,
        make: impl FnOnce(String) -> AccountCommand,
    ) {
        let account_id = match self.columns.lock().unwrap().get(column_id) {
            Some(a) => a.clone(),
            None => return,
        };
        if let Some(ctl) = self.accounts.lock().unwrap().get(&account_id) {
            let _ = ctl.cmd.try_send(make(column_id.to_string()));
        }
    }

    /// カラムを閉じる（購読解除）。そのアカウントの最後のカラムなら WebSocket ごと畳む。
    pub fn close(&self, column_id: &str) {
        let account_id = match self.columns.lock().unwrap().remove(column_id) {
            Some(a) => a,
            None => return,
        };
        // このアカウントに残るカラムがあるか
        let remaining = self
            .columns
            .lock()
            .unwrap()
            .values()
            .any(|a| a == &account_id);

        let mut accounts = self.accounts.lock().unwrap();
        if remaining {
            if let Some(ctl) = accounts.get(&account_id) {
                let _ = ctl.cmd.try_send(AccountCommand::RemoveChannel {
                    column_id: column_id.to_string(),
                });
            }
        } else if let Some(ctl) = accounts.remove(&account_id) {
            let _ = ctl.cancel.send(true);
            ctl.handle.abort();
        }
    }

    /// 現在張っている WebSocket 接続数（= アカウント数）。
    #[allow(dead_code)]
    pub fn open_count(&self) -> usize {
        self.accounts.lock().unwrap().len()
    }
}

fn spawn_account(
    app: AppHandle,
    account_id: String,
    host: String,
    token: String,
) -> AccountCtl {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (cmd_tx, cmd_rx) = mpsc::channel(128);
    let handle = tauri::async_runtime::spawn(async move {
        run_account(app, account_id, host, token, cancel_rx, cmd_rx).await;
    });
    AccountCtl {
        cmd: cmd_tx,
        cancel: cancel_tx,
        handle,
    }
}

/// 購読中の 1 チャンネル(=1カラム)。sub_id は接続ごとに振り直す。
struct ChannelSub {
    sub_id: String,
    channel: String,
    params: Value,
    mode: StreamMode,
    dedup: Dedup,
}

/// subNote 購読集合（接続単位・上限付き FIFO）。noteUpdated ルーティングのため
/// note_id -> それを表示中のカラム集合を保持する。
struct CaptureSet {
    order: VecDeque<String>,                    // note_id を登録順に（FIFO 追い出し用）
    refs: HashMap<String, HashSet<String>>,     // note_id -> 関心のある column_id 集合
    cap: usize,
}

/// 新規登録の結果: subNote が必要か / 上限超過で追い出した note_id。
struct AddOutcome {
    subscribe: bool,
    evicted: Option<String>,
}

impl CaptureSet {
    fn new(cap: usize) -> Self {
        Self {
            order: VecDeque::new(),
            refs: HashMap::new(),
            cap: cap.max(1),
        }
    }

    /// (note_id, column_id) を登録する。
    fn add(&mut self, note_id: &str, column_id: &str) -> AddOutcome {
        if let Some(set) = self.refs.get_mut(note_id) {
            set.insert(column_id.to_string());
            return AddOutcome {
                subscribe: false,
                evicted: None,
            };
        }
        let mut set = HashSet::new();
        set.insert(column_id.to_string());
        self.refs.insert(note_id.to_string(), set);
        self.order.push_back(note_id.to_string());
        let evicted = if self.order.len() > self.cap {
            self.order.pop_front().inspect(|old| {
                self.refs.remove(old);
            })
        } else {
            None
        };
        AddOutcome {
            subscribe: true,
            evicted,
        }
    }

    /// (note_id, column_id) の関心を外す。この note を誰も見なくなったら true（unsubNote 対象）。
    fn remove_note(&mut self, note_id: &str, column_id: &str) -> bool {
        if let Some(set) = self.refs.get_mut(note_id) {
            set.remove(column_id);
            if set.is_empty() {
                self.refs.remove(note_id);
                self.order.retain(|x| x != note_id);
                return true;
            }
        }
        false
    }

    /// あるカラムを丸ごと外す。誰も見なくなった note_id 群を返す（unsubNote 対象）。
    fn remove_column(&mut self, column_id: &str) -> Vec<String> {
        let mut emptied = Vec::new();
        self.refs.retain(|note_id, set| {
            set.remove(column_id);
            if set.is_empty() {
                emptied.push(note_id.clone());
                false
            } else {
                true
            }
        });
        if !emptied.is_empty() {
            self.order.retain(|x| !emptied.contains(x));
        }
        emptied
    }

    /// note_id を表示中のカラム一覧（noteUpdated ルーティング用）。
    fn columns_for(&self, note_id: &str) -> Vec<String> {
        self.refs
            .get(note_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn note_ids(&self) -> impl Iterator<Item = &String> {
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

/// アカウント接続ループ（再接続込み）。購読チャンネルとキャプチャ集合は再接続をまたいで
/// 保持し、接続確立時に一括で再購読する。
async fn run_account(
    app: AppHandle,
    account_id: String,
    host: String,
    token: String,
    mut cancel: watch::Receiver<bool>,
    mut cmd_rx: mpsc::Receiver<AccountCommand>,
) {
    let mut subs: HashMap<String, ChannelSub> = HashMap::new();
    let mut sub_index: HashMap<String, String> = HashMap::new(); // sub_id -> column_id
    let mut captures = CaptureSet::new(CAPTURE_CAP);
    let mut backoff = BACKOFF_START;

    loop {
        if *cancel.borrow() {
            return;
        }
        emit_state_all(&app, &subs, ConnectionState::Connecting);

        let outcome = connect_and_run(
            &app,
            &account_id,
            &host,
            &token,
            &mut subs,
            &mut sub_index,
            &mut captures,
            &mut cancel,
            &mut cmd_rx,
        )
        .await;

        match outcome {
            RunOutcome::Cancelled => return,
            RunOutcome::Disconnected => {
                emit_state_all(&app, &subs, ConnectionState::Reconnecting)
            }
            RunOutcome::Fatal => emit_state_all(&app, &subs, ConnectionState::Error),
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
    account_id: &str,
    host: &str,
    token: &str,
    subs: &mut HashMap<String, ChannelSub>,
    sub_index: &mut HashMap<String, String>,
    captures: &mut CaptureSet,
    cancel: &mut watch::Receiver<bool>,
    cmd_rx: &mut mpsc::Receiver<AccountCommand>,
) -> RunOutcome {
    let url = format!("wss://{host}/streaming?i={token}");
    // ハンドシェイクに User-Agent を付ける（既定では送られないため）。
    let request = {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::header::{HeaderValue, USER_AGENT};
        match url.as_str().into_client_request() {
            Ok(mut req) => {
                req.headers_mut()
                    .insert(USER_AGENT, HeaderValue::from_static(crate::state::USER_AGENT));
                req
            }
            Err(e) => {
                log::warn!("[{account_id}] ws request build failed: {e}");
                return RunOutcome::Disconnected;
            }
        }
    };
    let ws = match tokio_tungstenite::connect_async(request).await {
        Ok((ws, _resp)) => ws,
        Err(e) => {
            log::warn!("[{account_id}] ws connect failed: {e}");
            return RunOutcome::Disconnected;
        }
    };
    let (mut write, mut read): (SplitSink<Ws, Message>, SplitStream<Ws>) = ws.split();

    // 全チャンネルを（新しい sub_id で）再購読する。
    sub_index.clear();
    for (column_id, sub) in subs.iter_mut() {
        sub.sub_id = uuid::Uuid::new_v4().to_string();
        sub_index.insert(sub.sub_id.clone(), column_id.clone());
        if write
            .send(Message::Text(
                protocol::connect(&sub.channel, &sub.sub_id, sub.params.clone()).into(),
            ))
            .await
            .is_err()
        {
            return RunOutcome::Disconnected;
        }
    }
    // キャプチャ中ノートを再購読
    for id in captures.note_ids() {
        let _ = write.send(Message::Text(protocol::sub_note(id).into())).await;
    }
    emit_state_all(app, subs, ConnectionState::Connected);

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    for sub in subs.values() {
                        let _ = write.send(Message::Text(protocol::disconnect(&sub.sub_id).into())).await;
                    }
                    let _ = write.close().await;
                    return RunOutcome::Cancelled;
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(AccountCommand::AddChannel { column_id, channel, params, mode }) => {
                        // 既存を張り替える場合は先に外す
                        if let Some(old) = subs.remove(&column_id) {
                            sub_index.remove(&old.sub_id);
                            let _ = write.send(Message::Text(protocol::disconnect(&old.sub_id).into())).await;
                            for nid in captures.remove_column(&column_id) {
                                let _ = write.send(Message::Text(protocol::unsub_note(&nid).into())).await;
                            }
                        }
                        let sub_id = uuid::Uuid::new_v4().to_string();
                        if write
                            .send(Message::Text(protocol::connect(&channel, &sub_id, params.clone()).into()))
                            .await
                            .is_err()
                        {
                            return RunOutcome::Disconnected;
                        }
                        sub_index.insert(sub_id.clone(), column_id.clone());
                        subs.insert(column_id.clone(), ChannelSub {
                            sub_id,
                            channel,
                            params,
                            mode,
                            dedup: Dedup::new(DEDUP_CAPACITY),
                        });
                        emit_state(app, &column_id, ConnectionState::Connected);
                    }
                    Some(AccountCommand::RemoveChannel { column_id }) => {
                        if let Some(sub) = subs.remove(&column_id) {
                            sub_index.remove(&sub.sub_id);
                            let _ = write.send(Message::Text(protocol::disconnect(&sub.sub_id).into())).await;
                            for nid in captures.remove_column(&column_id) {
                                let _ = write.send(Message::Text(protocol::unsub_note(&nid).into())).await;
                            }
                        }
                    }
                    Some(AccountCommand::Capture { column_id, note_ids }) => {
                        for id in note_ids {
                            apply_capture_add(&mut write, captures, &id, &column_id).await;
                        }
                    }
                    Some(AccountCommand::Uncapture { column_id, note_ids }) => {
                        for id in note_ids {
                            if captures.remove_note(&id, &column_id) {
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
                        if let HandleResult::CaptureNote { column_id, note_id } =
                            handle_text(app, &text, subs, sub_index, captures)
                        {
                            apply_capture_add(&mut write, captures, &note_id, &column_id).await;
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        let _ = write.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => return RunOutcome::Disconnected,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("[{account_id}] ws read error: {e}");
                        return RunOutcome::Disconnected;
                    }
                }
            }
        }
    }
}

/// キャプチャ登録し、必要なら subNote / (追い出しの)unsubNote を送る。
async fn apply_capture_add(
    write: &mut SplitSink<Ws, Message>,
    captures: &mut CaptureSet,
    note_id: &str,
    column_id: &str,
) {
    let outcome = captures.add(note_id, column_id);
    if outcome.subscribe {
        let _ = write.send(Message::Text(protocol::sub_note(note_id).into())).await;
    }
    if let Some(old) = outcome.evicted {
        let _ = write.send(Message::Text(protocol::unsub_note(&old).into())).await;
    }
}

enum HandleResult {
    None,
    /// 新規ノートが column_id に届いた（自動キャプチャ対象）
    CaptureNote { column_id: String, note_id: String },
}

/// テキストフレームを解釈して該当カラムへ emit する。
/// - channel note: sub_id からカラムを特定し、フィルタ/ミュート適用後に ColumnNote を emit。
///   新規なら自動キャプチャのため CaptureNote を返す。
/// - channel notification: 同様にカラム特定して ColumnNotification を emit。
/// - noteUpdated: そのノートを表示中の全カラムへ ColumnNoteUpdated を emit。
fn handle_text(
    app: &AppHandle,
    text: &str,
    subs: &mut HashMap<String, ChannelSub>,
    sub_index: &HashMap<String, String>,
    captures: &CaptureSet,
) -> HandleResult {
    match protocol::parse_incoming(text) {
        Incoming::ChannelNote { channel_id, note } => {
            let Some(column_id) = sub_index.get(&channel_id) else {
                return HandleResult::None;
            };
            let Some(sub) = subs.get_mut(column_id) else {
                return HandleResult::None;
            };
            let StreamMode::Notes { account_id, filter, ctx } = &sub.mode else {
                return HandleResult::None;
            };
            if !sub.dedup.accept(&note.id) {
                return HandleResult::None;
            }
            let id = note.id.clone();
            let normalized: Note = (*note).into();
            // フィルタを通過しないノートは出さない（キャッシュ・キャプチャもしない）
            if !filter.matches(&normalized, ctx) {
                return HandleResult::None;
            }
            if let Some(state) = app.try_state::<AppState>() {
                if crate::filter::mute::is_muted(&normalized, &state.mute.lock().unwrap()) {
                    return HandleResult::None;
                }
                if is_server_muted_note(&state, account_id, &normalized) {
                    return HandleResult::None;
                }
                let _ = state.settings.cache_note(column_id, &normalized);
            }
            let _ = ColumnNote {
                column_id: column_id.clone(),
                note: normalized,
            }
            .emit(app);
            HandleResult::CaptureNote {
                column_id: column_id.clone(),
                note_id: id,
            }
        }
        Incoming::ChannelNotification { channel_id, notification } => {
            let Some(column_id) = sub_index.get(&channel_id) else {
                return HandleResult::None;
            };
            let Some(sub) = subs.get_mut(column_id) else {
                return HandleResult::None;
            };
            if !sub.dedup.accept(&notification.id) {
                return HandleResult::None;
            }
            let account_id = sub.mode.account_id().to_string();
            let n: Notification = (*notification).into();
            if let Some(state) = app.try_state::<AppState>() {
                if notification_muted(&state, &account_id, &n) {
                    return HandleResult::None;
                }
            }
            let _ = ColumnNotification {
                column_id: column_id.clone(),
                notification: n,
            }
            .emit(app);
            HandleResult::None
        }
        Incoming::NoteUpdated { note_id, kind, body } => {
            if let Some((update, actor_id)) = map_note_update(&kind, &body) {
                // subNote は接続単位。そのノートを表示中の全カラムへ配る。
                for column_id in captures.columns_for(&note_id) {
                    let _ = ColumnNoteUpdated {
                        column_id,
                        note_id: note_id.clone(),
                        update: update.clone(),
                        actor_id: actor_id.clone(),
                    }
                    .emit(app);
                }
            }
            HandleResult::None
        }
        Incoming::Other => HandleResult::None,
    }
}

/// ノート本体 or renote 先のユーザがサーバ側ミュート/ブロック対象か。
fn is_server_muted_note(state: &AppState, account_id: &str, note: &Note) -> bool {
    if state.is_server_muted(account_id, &note.user.id) {
        return true;
    }
    matches!(&note.renote, Some(r) if state.is_server_muted(account_id, &r.user.id))
}

/// 通知の発生元ユーザが NG（ローカル）またはサーバ側ミュート/ブロックか。
fn notification_muted(state: &AppState, account_id: &str, n: &Notification) -> bool {
    let Some(user) = &n.user else {
        return false;
    };
    if state.is_server_muted(account_id, &user.id) {
        return true;
    }
    crate::filter::mute::is_user_muted(user, &state.mute.lock().unwrap())
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

/// 指定アカウントの全カラムへ接続状態を通知する。
fn emit_state_all(app: &AppHandle, subs: &HashMap<String, ChannelSub>, state: ConnectionState) {
    for column_id in subs.keys() {
        emit_state(app, column_id, state);
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
    fn capture_set_subscribes_once_and_routes_columns() {
        let mut c = CaptureSet::new(8);
        // 同一ノートを 2 カラムが見る: subNote は最初の 1 回だけ
        let a = c.add("n1", "colA");
        assert!(a.subscribe && a.evicted.is_none());
        let b = c.add("n1", "colB");
        assert!(!b.subscribe && b.evicted.is_none());
        // noteUpdated は両カラムへ配る
        let mut cols = c.columns_for("n1");
        cols.sort();
        assert_eq!(cols, vec!["colA".to_string(), "colB".to_string()]);
        // 片方だけ外しても購読は残る
        assert!(!c.remove_note("n1", "colA"));
        assert_eq!(c.columns_for("n1"), vec!["colB".to_string()]);
        // 最後のカラムが外れると unsub 対象
        assert!(c.remove_note("n1", "colB"));
        assert!(c.columns_for("n1").is_empty());
    }

    #[test]
    fn capture_set_evicts_fifo() {
        let mut c = CaptureSet::new(2);
        assert!(c.add("a", "col").subscribe);
        assert!(c.add("b", "col").subscribe);
        let out = c.add("c", "col"); // a を追い出す
        assert!(out.subscribe);
        assert_eq!(out.evicted.as_deref(), Some("a"));
        assert!(c.columns_for("a").is_empty());
    }

    #[test]
    fn capture_set_remove_column_returns_emptied() {
        let mut c = CaptureSet::new(8);
        c.add("n1", "colA");
        c.add("n1", "colB");
        c.add("n2", "colA");
        // colA を丸ごと外す: n2 は誰も見なくなるが n1 は colB が残る
        let emptied = c.remove_column("colA");
        assert_eq!(emptied, vec!["n2".to_string()]);
        assert_eq!(c.columns_for("n1"), vec!["colB".to_string()]);
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
