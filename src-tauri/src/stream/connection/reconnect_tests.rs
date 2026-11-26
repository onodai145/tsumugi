//! 切断→backoff→再接続→購読復元の自動テスト(Issue #224)。
//!
//! 実 Misskey の代わりに、テスト内で立てたモック WebSocket サーバーへ `run_account` を
//! 直接つなぐ。イベントは `tauri::test::mock_app()` の AppHandle 上で実際に emit させ、
//! listen して検証する。

use super::*;
use serde_json::json;
use std::time::Duration;
use tauri::test::{mock_app, MockRuntime};
use tauri_specta::{collect_events, Builder};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http;

/// テストが待つ上限。これを超えたら「起きるはずのことが起きなかった」とみなして失敗させる。
const WAIT: Duration = Duration::from_secs(5);

async fn within<T>(fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(WAIT, fut)
        .await
        .expect("timed out waiting for expected event")
}

/// 本番と同じ挙動で、時間だけ短くした設定。
fn fast_config() -> StreamConfig {
    StreamConfig {
        scheme: "ws",
        backoff_start: Duration::from_millis(20),
        backoff_max: Duration::from_millis(160),
        ping_interval: Duration::from_millis(50),
        read_timeout: Duration::from_millis(200),
    }
}

struct MockServer {
    listener: TcpListener,
}

impl MockServer {
    async fn bind() -> Self {
        Self {
            listener: TcpListener::bind("127.0.0.1:0").await.unwrap(),
        }
    }

    /// `run_account` の `host` 引数に渡す値。
    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.listener.local_addr().unwrap().port())
    }

    /// TCP接続だけ受けて即座に切る(WebSocketハンドシェイクを失敗させる)。受けた時刻を返す。
    async fn reject(&self) -> tokio::time::Instant {
        let (tcp, _) = within(self.listener.accept()).await.unwrap();
        drop(tcp);
        tokio::time::Instant::now()
    }

    /// クライアントの接続を1本受けて WebSocket ハンドシェイクを完了する。
    async fn accept(&self) -> MockConn {
        let (tcp, _) = within(self.listener.accept()).await.unwrap();
        let ws = within(tokio_tungstenite::accept_async(tcp))
            .await
            .expect("ws handshake failed");
        MockConn { ws }
    }

    /// ハンドシェイクを HTTP ステータス `code` で拒否する(401 など)。拒否した時刻を返す。
    #[allow(clippy::result_large_err)] // tungstenite のコールバックの型
    async fn reject_with_status(&self, code: u16) -> tokio::time::Instant {
        let (tcp, _) = within(self.listener.accept()).await.unwrap();
        let result = within(tokio_tungstenite::accept_hdr_async(
            tcp,
            move |_req: &Request, _resp: Response| -> Result<Response, ErrorResponse> {
                Err(http::Response::builder().status(code).body(None).unwrap())
            },
        ))
        .await;
        assert!(result.is_err(), "the handshake must have been rejected");
        tokio::time::Instant::now()
    }

    /// `accept` と同じだが、クライアントが要求した URI(`/streaming?i=<token>`)も返す。
    #[allow(clippy::result_large_err)] // tungstenite のコールバックの型
    async fn accept_with_uri(&self) -> (MockConn, String) {
        let (tcp, _) = within(self.listener.accept()).await.unwrap();
        let mut uri = String::new();
        let ws = within(tokio_tungstenite::accept_hdr_async(
            tcp,
            |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                uri = req.uri().to_string();
                Ok(resp)
            },
        ))
        .await
        .expect("ws handshake failed");
        (MockConn { ws }, uri)
    }
}

struct MockConn {
    ws: tokio_tungstenite::WebSocketStream<TcpStream>,
}

impl MockConn {
    /// 次のテキストフレームを JSON として受け取る。Ping/Pong 等の制御フレームは読み飛ばす。
    async fn recv_json(&mut self) -> Value {
        self.recv_json_within(WAIT).await
    }

    /// `limit` は text フレームが届くまでの合計の待ち時間。制御フレームを読み飛ばしても延びない。
    async fn recv_json_within(&mut self, limit: Duration) -> Value {
        let recv = async {
            loop {
                match self.ws.next().await {
                    Some(Ok(Message::Text(t))) => return serde_json::from_str(&t).unwrap(),
                    Some(Ok(_)) => continue,
                    other => panic!("expected a text frame, got {other:?}"),
                }
            }
        };
        tokio::time::timeout(limit, recv)
            .await
            .expect("timed out waiting for a text frame")
    }

    async fn send_text(&mut self, text: &str) {
        self.ws.send(Message::Text(text.into())).await.unwrap();
    }

    /// 購読中チャンネル(sub_id)へ届いたノートのフレームを送る。
    async fn send_channel_note(&mut self, sub_id: &Value, note_id: &str) {
        let frame = json!({
            "type": "channel",
            "body": {
                "id": sub_id,
                "type": "note",
                "body": {
                    "id": note_id,
                    "createdAt": "2026-07-05T00:00:00Z",
                    "user": { "id": "u1", "username": "alice" }
                }
            }
        });
        self.send_text(&frame.to_string()).await;
    }

    /// サーバー側から正常にクローズする。
    async fn close(mut self) {
        self.ws.close(None).await.unwrap();
    }
}

struct Harness {
    cmd_tx: mpsc::UnboundedSender<AccountCommand>,
    cancel_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    states: UnboundedReceiver<ColumnConnectionState>,
    notes: UnboundedReceiver<ColumnNote>,
    // App を落とすと AppHandle が無効になるため保持しておく。
    _app: tauri::App<MockRuntime>,
}

/// イベントを登録した mock の App。`ConnectionManager` 経由のテストでも使う。
fn app_with_events() -> tauri::App<MockRuntime> {
    let app = mock_app();
    Builder::<MockRuntime>::new()
        .events(collect_events![
            ColumnNote,
            ColumnNoteUpdated,
            ColumnNotification,
            ColumnConnectionState,
        ])
        .mount_events(&app);
    app
}

impl Harness {
    fn start(server: &MockServer, config: StreamConfig) -> Self {
        let app = app_with_events();
        let handle = app.handle().clone();

        let (state_tx, states) = unbounded_channel();
        ColumnConnectionState::listen_any(&handle, move |ev| {
            let _ = state_tx.send(ev.payload);
        });

        let (note_tx, notes) = unbounded_channel();
        ColumnNote::listen_any(&handle, move |ev| {
            let _ = note_tx.send(ev.payload);
        });

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let host = server.host();
        let task = tokio::spawn(async move {
            run_account(
                handle,
                "acc1".into(),
                host,
                "tok".into(),
                config,
                Default::default(),
                cancel_rx,
                cmd_rx,
            )
            .await;
        });
        Self {
            cmd_tx,
            cancel_tx,
            task,
            states,
            notes,
            _app: app,
        }
    }

    /// 通知カラム(main チャンネル)を購読に加える。フィルタ設定が要らないので最小の構成。
    async fn add_notifications_channel(&self, column_id: &str) {
        self.cmd_tx
            .send(AccountCommand::AddChannel {
                sub_key: column_id.into(),
                column_id: column_id.into(),
                channel: "main".into(),
                params: json!({}),
                mode: StreamMode::Notifications {
                    account_id: "acc1".into(),
                },
                initial_last_seen_id: None,
            })
            .unwrap();
    }

    /// ノートを流すチャンネル(フィルタなし)を購読に加える。
    async fn add_notes_channel(&self, column_id: &str, channel: &str) {
        self.cmd_tx
            .send(AccountCommand::AddChannel {
                sub_key: column_id.into(),
                column_id: column_id.into(),
                channel: channel.into(),
                params: json!({}),
                mode: StreamMode::Notes {
                    account_id: "acc1".into(),
                    filter: Arc::new(CompiledFilter::PassAll),
                    ctx: Arc::new(EvalContext::default()),
                },
                initial_last_seen_id: None,
            })
            .unwrap();
    }

    async fn capture(&self, column_id: &str, note_ids: &[&str]) {
        self.cmd_tx
            .send(AccountCommand::Capture {
                column_id: column_id.into(),
                note_ids: note_ids.iter().map(|s| s.to_string()).collect(),
            })
            .unwrap();
    }

    /// 次に emit された ColumnNote のノートIDを返す。
    async fn next_note_id(&mut self) -> String {
        within(self.notes.recv())
            .await
            .expect("note channel closed")
            .note
            .id
    }

    /// `column_id` 宛ての次の接続状態イベントを返す。
    async fn next_state(&mut self, column_id: &str) -> ConnectionState {
        loop {
            let ev = within(self.states.recv())
                .await
                .expect("state channel closed");
            if ev.column_id == column_id {
                return ev.state;
            }
        }
    }
}

#[tokio::test]
async fn initial_connect_subscribes_channel_and_reports_connected() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    let mut conn = server.accept().await;
    let msg = conn.recv_json().await;

    assert_eq!(msg["type"], "connect");
    assert_eq!(msg["body"]["channel"], "main");
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connected
    ));
}

#[tokio::test]
async fn reconnects_and_resubscribes_channel_with_fresh_sub_id_after_server_close() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    let mut first = server.accept().await;
    let first_connect = first.recv_json().await;
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connected
    ));

    first.close().await;
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Reconnecting
    ));

    // backoff 明けの再接続試行で Connecting、確立で Connected の順に遷移する。
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connecting
    ));
    let mut second = server.accept().await;
    let second_connect = second.recv_json().await;
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connected
    ));

    assert_eq!(second_connect["type"], "connect");
    assert_eq!(second_connect["body"]["channel"], "main");
    assert_ne!(
        second_connect["body"]["id"], first_connect["body"]["id"],
        "sub_id must be reissued per connection"
    );
}

#[tokio::test]
async fn resubscribes_captured_notes_after_reconnect() {
    let server = MockServer::bind().await;
    let h = Harness::start(&server, fast_config());
    h.add_notes_channel("col1", "homeTimeline").await;
    h.capture("col1", &["n1", "n2"]).await;

    let mut first = server.accept().await;
    assert_eq!(first.recv_json().await["type"], "connect");
    for expected in ["n1", "n2"] {
        let m = first.recv_json().await;
        assert_eq!(m["type"], "subNote");
        assert_eq!(m["body"]["id"], expected);
    }

    first.close().await;
    let mut second = server.accept().await;

    // 再接続時は connect を先に、続けてキャプチャ済みノートの subNote を登録順に送り直す。
    assert_eq!(second.recv_json().await["type"], "connect");
    for expected in ["n1", "n2"] {
        let m = second.recv_json().await;
        assert_eq!(m["type"], "subNote");
        assert_eq!(m["body"]["id"], expected);
    }
}

#[tokio::test]
async fn delivers_notes_only_for_the_current_connections_sub_id() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notes_channel("col1", "homeTimeline").await;

    let mut first = server.accept().await;
    let old_sub_id = first.recv_json().await["body"]["id"].clone();
    first.close().await;

    let mut second = server.accept().await;
    let new_sub_id = second.recv_json().await["body"]["id"].clone();

    // 旧接続の sub_id 宛てのノートは、再接続後は購読が無いものとして捨てられる。
    second.send_channel_note(&old_sub_id, "stale").await;
    second.send_channel_note(&new_sub_id, "fresh").await;
    assert_eq!(h.next_note_id().await, "fresh");
}

#[tokio::test]
async fn dedup_survives_reconnect_so_a_note_seen_before_is_not_emitted_again() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notes_channel("col1", "homeTimeline").await;

    let mut first = server.accept().await;
    let sub_id = first.recv_json().await["body"]["id"].clone();
    first.send_channel_note(&sub_id, "n1").await;
    assert_eq!(h.next_note_id().await, "n1");
    first.close().await;

    let mut second = server.accept().await;
    let sub_id = second.recv_json().await["body"]["id"].clone();
    second.send_channel_note(&sub_id, "n1").await; // 再接続後の再配信
    second.send_channel_note(&sub_id, "n2").await;
    assert_eq!(h.next_note_id().await, "n2");
}

#[tokio::test]
async fn reconnects_when_server_goes_silent_past_read_timeout() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    // 接続は保持したまま何も読み書きしない(= Ping に Pong が返らない、TCPだけ生きている状態)。
    let _silent = server.accept().await;
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connected
    ));

    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Reconnecting
    ));
    let mut second = server.accept().await;
    assert_eq!(second.recv_json().await["type"], "connect");
}

#[tokio::test]
async fn cancel_disconnects_channels_and_stops_the_task() {
    let server = MockServer::bind().await;
    let h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    let mut conn = server.accept().await;
    let sub_id = conn.recv_json().await["body"]["id"].clone();

    h.cancel_tx.send(true).unwrap();

    let m = conn.recv_json().await;
    assert_eq!(m["type"], "disconnect");
    assert_eq!(m["body"]["id"], sub_id);
    within(h.task).await.unwrap();
}

#[test]
fn next_backoff_doubles_and_caps_at_max() {
    let max = Duration::from_secs(30);
    assert_eq!(
        next_backoff(Duration::from_secs(1), max),
        Duration::from_secs(2)
    );
    assert_eq!(
        next_backoff(Duration::from_secs(16), max),
        Duration::from_secs(30)
    );
    assert_eq!(next_backoff(max, max), max);
}

#[tokio::test]
async fn backoff_grows_between_consecutive_failed_attempts() {
    let server = MockServer::bind().await;
    let config = StreamConfig {
        backoff_start: Duration::from_millis(50),
        backoff_max: Duration::from_millis(400),
        ..fast_config()
    };
    let _h = Harness::start(&server, config);

    let mut attempts = vec![server.reject().await];
    for _ in 0..3 {
        attempts.push(server.reject().await);
    }

    // 試行間隔は下限だけ検証する(上限を見るとCI負荷でflakyになるため。上限は next_backoff の単体テストが担う)。
    let gaps: Vec<_> = attempts.windows(2).map(|w| w[1] - w[0]).collect();
    for (gap, floor_ms) in gaps.iter().zip([50u64, 100, 200]) {
        assert!(
            *gap >= Duration::from_millis(floor_ms),
            "gap {gap:?} should be at least {floor_ms}ms"
        );
    }
}

#[tokio::test]
async fn backoff_resets_after_a_connection_was_established() {
    let server = MockServer::bind().await;
    let config = StreamConfig {
        backoff_start: Duration::from_millis(20),
        backoff_max: Duration::from_secs(10),
        ..fast_config()
    };
    let _h = Harness::start(&server, config);

    // 失敗を重ねて待ち時間を 20→40→80→160→320 と伸ばす。次に失敗すれば 640ms 待つ状態。
    for _ in 0..5 {
        server.reject().await;
    }
    let conn = server.accept().await;
    conn.close().await;
    let closed_at = tokio::time::Instant::now();

    // 一度接続できていたので待ち時間は初期値(20ms)へ戻り、伸ばした値(640ms)は引きずらない。
    // 上限を見る唯一のテスト。リセットされていれば約20ms、されていなければ640ms以上で、
    // 閾値を離してあるためCI負荷でも誤検知しにくい。
    server.accept().await;
    assert!(
        closed_at.elapsed() < Duration::from_millis(300),
        "reconnect took {:?}; backoff should have been reset to the start value",
        closed_at.elapsed()
    );
}

/// 期待するフレームが来ないとき、Ping が届き続けていても待ちが打ち切られること。
/// (フレーム単位でタイムアウトを測ると Ping のたびに延びて、テストが失敗せずハングする)
#[tokio::test]
#[should_panic(expected = "timed out waiting for a text frame")]
async fn recv_json_gives_up_even_while_pings_keep_arriving() {
    let server = MockServer::bind().await;
    let _h = Harness::start(&server, fast_config());

    // チャンネル未購読なので text フレームは来ない。クライアントの Ping だけが 50ms ごとに届く。
    let mut conn = server.accept().await;
    conn.recv_json_within(Duration::from_millis(300)).await;
}

/// Issue #486: 返信先がミュート対象のとき、emit されるノートでは返信先を外し、
/// キャッシュへ渡すノートには元の返信先を残す(NG 設定を後で外したときに復元できるように)。
#[tokio::test]
async fn channel_note_strips_muted_reply_from_emit_but_keeps_it_in_cache() {
    let app = mock_app();
    Builder::<MockRuntime>::new()
        .events(collect_events![ColumnNote])
        .mount_events(&app);
    let handle = app.handle().clone();
    // AppState を管理しないと mute/cache/strip の分岐ごと飛ばされ、テストが空振りになる。
    app.manage(AppState::new_for_test(
        crate::store::SettingsStore::new_in_memory(),
    ));
    let state = handle.state::<AppState>();
    state.set_server_mutes("acc1", HashSet::from(["u_muted".to_string()]));

    let (note_tx, mut notes) = unbounded_channel();
    ColumnNote::listen_any(&handle, move |ev| {
        let _ = note_tx.send(ev.payload);
    });

    let mut subs = HashMap::from([(
        "col1".to_string(),
        ChannelSub {
            sub_id: "sub1".into(),
            column_id: "col1".into(),
            channel: "homeTimeline".into(),
            params: json!({}),
            mode: StreamMode::Notes {
                account_id: "acc1".into(),
                filter: Arc::new(CompiledFilter::PassAll),
                ctx: Arc::new(EvalContext::default()),
            },
            dedup: Dedup::new(DEDUP_CAPACITY),
            last_seen_notification_id: None,
        },
    )]);
    let sub_index = HashMap::from([("sub1".to_string(), "col1".to_string())]);
    let captures = CaptureSet::new(8);
    let mut reaction_event_dedup = Dedup::new(DEDUP_CAPACITY);

    let frame = json!({
        "type": "channel",
        "body": {
            "id": "sub1",
            "type": "note",
            "body": {
                "id": "n9",
                "createdAt": "2026-07-05T00:00:00Z",
                "user": { "id": "u1", "username": "alice" },
                "text": "my reply",
                "replyId": "r1",
                "reply": {
                    "id": "r1",
                    "createdAt": "2026-07-05T00:00:00Z",
                    "text": "muted target",
                    "user": { "id": "u_muted", "username": "bob" }
                }
            }
        }
    });
    let result = handle_text(
        &handle,
        "acc1",
        &frame.to_string(),
        &mut subs,
        &sub_index,
        &captures,
        &mut reaction_event_dedup,
    )
    .await;
    assert!(matches!(result, HandleResult::CaptureNote { .. }));

    // emit されたノートは返信先が外れ、「↩ 返信」用の reply_id は残る。
    let emitted = within(notes.recv())
        .await
        .expect("note channel closed")
        .note;
    assert_eq!(emitted.id, "n9");
    assert!(emitted.reply.is_none(), "emit では返信先を外す");
    assert_eq!(emitted.reply_id.as_deref(), Some("r1"));

    // キャッシュには元の返信先が残る。
    let cached = state
        .cache
        .get_note("n9")
        .await
        .unwrap()
        .expect("n9 is cached");
    let cached_reply = cached.reply.expect("キャッシュには元の返信先を残す");
    assert_eq!(cached_reply.id, "r1");
}

#[test]
fn is_auth_failure_is_true_only_for_a_401_handshake_response() {
    use tokio_tungstenite::tungstenite::Error as WsError;
    let http_error = |code: u16| {
        WsError::Http(Box::new(
            http::Response::builder().status(code).body(None).unwrap(),
        ))
    };
    assert!(is_auth_failure(&http_error(401)));
    assert!(
        !is_auth_failure(&http_error(403)),
        "403 は WAF などの一時的な拒否もありうるので止めない"
    );
    assert!(!is_auth_failure(&http_error(500)));
    assert!(!is_auth_failure(&WsError::ConnectionClosed));
}

#[tokio::test]
async fn an_unauthorized_handshake_reports_error_for_every_pending_column_and_stops_the_task() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    // 接続前に積まれたコマンドは、まだ `subs` に入っていない。止まるときに回収されて Error になる。
    h.add_notifications_channel("col1").await;
    h.add_notifications_channel("col2").await;

    server.reject_with_status(401).await;

    assert!(matches!(h.next_state("col1").await, ConnectionState::Error));
    assert!(matches!(h.next_state("col2").await, ConnectionState::Error));
    // バックオフ(20ms)を十分に超えて待っても、再接続は来ない。
    assert!(
        tokio::time::timeout(Duration::from_millis(300), server.listener.accept())
            .await
            .is_err(),
        "an unauthorized account must not be retried"
    );
    within(h.task).await.unwrap();
}

#[tokio::test]
async fn an_unauthorized_reconnect_reports_error_and_stops_retrying() {
    let server = MockServer::bind().await;
    let mut h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    let mut first = server.accept().await;
    assert_eq!(first.recv_json().await["type"], "connect");
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connected
    ));
    first.close().await;
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Reconnecting
    ));
    assert!(matches!(
        h.next_state("col1").await,
        ConnectionState::Connecting
    ));

    // トークンが失効した: 再接続のハンドシェイクが 401 になる。
    server.reject_with_status(401).await;

    assert!(matches!(h.next_state("col1").await, ConnectionState::Error));
    assert!(
        tokio::time::timeout(Duration::from_millis(300), server.listener.accept())
            .await
            .is_err(),
        "an unauthorized account must not be retried"
    );
    within(h.task).await.unwrap();
}

#[tokio::test]
async fn a_non_401_handshake_rejection_keeps_retrying() {
    let server = MockServer::bind().await;
    let h = Harness::start(&server, fast_config());
    h.add_notifications_channel("col1").await;

    // 500 や 403 は止めない: バックオフ後に、同じサーバーへ再び接続が来る。
    server.reject_with_status(500).await;
    server.reject_with_status(403).await;
    // 403 のあとにも、もう一度接続が来る(403 で止まっていたら、ここで待ち切れず失敗する)。
    server.reject_with_status(500).await;

    assert!(!h.task.is_finished());
}

#[tokio::test]
async fn try_send_hands_the_command_back_when_the_receiver_is_gone() {
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);

    match try_send(
        &tx,
        AccountCommand::RemoveChannel {
            column_id: "c1".into(),
        },
    ) {
        Err(AccountCommand::RemoveChannel { column_id }) => assert_eq!(column_id, "c1"),
        _ => panic!("the command must be handed back"),
    }
}

/// `acc1` のタスクが止まる(401 を受ける)まで待つ。
async fn wait_until_account_task_stops(manager: &ConnectionManager) {
    within(async {
        while !manager
            .accounts
            .lock()
            .unwrap()
            .get("acc1")
            .unwrap()
            .cmd
            .is_closed()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

#[tokio::test]
async fn add_channel_replaces_an_account_task_that_stopped_after_an_auth_failure() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    // モックの接続は `recv_json` 以外では読まず、Ping に Pong を返さない。既定の短い `read_timeout`
    // だと、アイドルによる再接続が起きて「タスクを再利用した」の確認と区別できなくなる。
    let manager = ConnectionManager::with_config(StreamConfig {
        read_timeout: Duration::from_secs(30),
        ..fast_config()
    });
    let open = |column: &str, token: &str| {
        manager.open_notifications(
            app.handle().clone(),
            column.into(),
            "acc1".into(),
            server.host(),
            token.into(),
            None,
        )
    };

    open("col1", "old-token");
    server.reject_with_status(401).await;
    wait_until_account_task_stops(&manager).await;

    // 再認証後の再接続: 新しいトークンで、タスクが作り直される。
    open("col2", "new-token");
    let (mut conn, uri) = server.accept_with_uri().await;
    assert!(
        uri.contains("i=new-token"),
        "the replacement task must use the new token: {uri}"
    );
    assert_eq!(conn.recv_json().await["type"], "connect");

    // 続けて同じアカウントの別カラムを足しても、タスクは増えず、同じ接続に相乗りする。
    open("col3", "new-token");
    assert_eq!(conn.recv_json().await["type"], "connect");
    assert!(
        tokio::time::timeout(Duration::from_millis(200), server.listener.accept())
            .await
            .is_err(),
        "a live account task must be reused"
    );
    assert_eq!(manager.open_count(), 1);
}

#[tokio::test]
async fn closing_the_last_column_of_a_stopped_account_removes_it() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    let manager = ConnectionManager::with_config(fast_config());

    manager.open_notifications(
        app.handle().clone(),
        "col1".into(),
        "acc1".into(),
        server.host(),
        "old-token".into(),
        None,
    );
    server.reject_with_status(401).await;
    wait_until_account_task_stops(&manager).await;

    manager.close("col1");

    assert_eq!(manager.open_count(), 0);
}

// ---- 認証失敗で止まった通知カラムのウォーターマーク退避(Issue #684) ----

#[tokio::test]
async fn auth_failure_keeps_the_watermark_of_a_notification_column_still_waiting_to_connect() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    let manager = ConnectionManager::with_config(fast_config());

    // 初回接続のハンドシェイクが 401: AddChannel はまだ `subs` に入っていない。
    manager.open_notifications(
        app.handle().clone(),
        "col1".into(),
        "acc1".into(),
        server.host(),
        "old-token".into(),
        Some("9tj000005".into()),
    );
    server.reject_with_status(401).await;
    wait_until_account_task_stops(&manager).await;

    assert_eq!(
        manager.take_stalled_notification_mark("col1").as_deref(),
        Some("9tj000005")
    );
    assert_eq!(
        manager.take_stalled_notification_mark("col1"),
        None,
        "取り出したら消える"
    );
}

#[tokio::test]
async fn auth_failure_keeps_the_watermark_of_a_subscribed_notification_column() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    let manager = ConnectionManager::with_config(fast_config());

    manager.open_notifications(
        app.handle().clone(),
        "col1".into(),
        "acc1".into(),
        server.host(),
        "tok".into(),
        Some("9tj000005".into()),
    );
    // 一度つながって購読が `subs` に入ってから切れ、再接続が 401 になる。
    let mut conn = server.accept().await;
    assert_eq!(conn.recv_json().await["type"], "connect");
    drop(conn);
    server.reject_with_status(401).await;
    wait_until_account_task_stops(&manager).await;

    assert_eq!(
        manager.take_stalled_notification_mark("col1").as_deref(),
        Some("9tj000005")
    );
}

#[tokio::test]
async fn closing_a_column_drops_its_stalled_watermark() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    let manager = ConnectionManager::with_config(fast_config());

    manager.open_notifications(
        app.handle().clone(),
        "col1".into(),
        "acc1".into(),
        server.host(),
        "tok".into(),
        Some("9tj000005".into()),
    );
    server.reject_with_status(401).await;
    wait_until_account_task_stops(&manager).await;

    manager.close("col1");

    assert_eq!(manager.take_stalled_notification_mark("col1"), None);
}
