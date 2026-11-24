# ストリーミングの認証失敗で止まり、再認証で復帰する 実装計画 (Issue #622)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ハンドシェイクが HTTP 401 で拒否されたらアカウントの再接続を止めて該当カラムを `Error` にし、Backstage の「再認証」から再認証すると `Error` のカラムのストリームを再起動なしで張り直す。

**Architecture:** `connect_and_run` が 401 だけを `RunOutcome::Fatal` にし、`run_account` は `Error` を emit したあと未処理コマンドを回収して終了する。`ConnectionManager::add_channel` は止まったタスクを `is_closed()` で検出して作り直す。新コマンド `reconnect_column` が `Error` のカラムのストリームを開き直し、フロントは再認証の完了(`completeAccount`)で `error` のタブにだけそれを呼ぶ。

**Tech Stack:** Rust(tokio, tokio-tungstenite 0.30, tauri 2, tauri-specta), Svelte 5 + TypeScript(Vitest)

**Spec:** `docs/superpowers/specs/2026-11-24-issue-622-stream-auth-fatal-design.md`

## Global Constraints

- ブランチは `fix/issue-622-stream-auth-fatal`(作成済み)。`main` では編集もコミットもしない。
- 認証失敗は **HTTP 401 のみ**(403 は含めない)。`ConnectionState` の列挙と、`ColumnConnectionState` 以外のイベントは**増やさない**。
- コミットメッセージは件名 1 行のみ(本文なし)、`type(scope): 件名` 形式。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別途付く。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告する(再試行しない)。
- `cargo run` / `./target/debug/tsumugi` は使わない。Rust のテストは `cd src-tauri && cargo test ...`。新しい target ディレクトリでの最初のビルドは 15〜30 分かかる。メモリが厳しいときは `CARGO_BUILD_JOBS=4` を付ける。`CARGO_TARGET_DIR` を `/tmp` に置かない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集しない(`cd src-tauri && cargo test generates_frontend_bindings` で再生成)。
- フロントのテストは `cd frontend && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視する)。
- 検証用に自分で起動したプロセス(`cargo tauri dev`、Xvfb など)は完了前に PID を指定して止める。`pkill` / `killall` は使わない。

## Review Focus

仕様が示唆しているが、明示的なテスト対象になりにくい入力・状態。各行のテストは、担当タスクに入れてある。

- 401 のとき、接続前に積まれた `AddChannel` だけが残っている(`subs` が空)カラム → 全カラムが `Error` になる(Task 1)。
- 403 や 500 の応答(WAF・サーバー障害)→ 止まらず再試行を続ける(Task 1)。
- 止まったアカウントに、同じアカウントの 2 つ目・3 つ目のカラムを続けて追加する → タスクは 1 つだけ作り直され、2 つ目以降は同じタスクに相乗りする(Task 2)。
- `Error` のカラムを閉じる(または止まったアカウントの最後のカラムを閉じる)→ パニックせず、`open_count` が 0 に戻る(Task 2)。
- 再認証したのにまた 401 → `Error` に戻り、ログの重複抑止が再認証でリセットされているので、もう一度「再認証」ボタン付きのログが出る(Task 4)。

---

### Task 1: 401 で `Fatal` を返して止まる

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`(`RunOutcome` 〜 `:457`、`next_backoff` の直後、`run_account` の `match outcome` 〜 `:524-530`、`connect_and_run` の `connect_async` 〜 `:573-579`)
- Test: `src-tauri/src/stream/connection/reconnect_tests.rs`

**Interfaces:**
- Produces: `fn is_auth_failure(e: &tokio_tungstenite::tungstenite::Error) -> bool`(`connection.rs` 内の private 関数)。`MockServer::reject_with_status(&self, code: u16) -> tokio::time::Instant` と `MockServer::accept_with_uri(&self) -> (MockConn, String)`(テスト用、Task 2 が使う)。`fn app_with_events() -> tauri::App<MockRuntime>`(テスト用、Task 2 が使う)。

- [ ] **Step 1: spec と plan をコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/superpowers/specs/2026-11-24-issue-622-stream-auth-fatal-design.md docs/superpowers/plans/2026-11-24-issue-622-stream-auth-fatal.md
git commit -m "docs(stream): ストリーミング認証失敗の停止と復帰のspecとplanを追加(Issue #622)"
```

- [ ] **Step 2: テスト用の補助を足す(`reconnect_tests.rs`)**

ファイル先頭の `use` 群の末尾に追加する。

```rust
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http;
```

`impl MockServer` の `accept` の直後に追加する。

```rust
    /// ハンドシェイクを HTTP ステータス `code` で拒否する(401 など)。拒否した時刻を返す。
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
```

`Harness::start` の先頭の `mock_app()` と `Builder` の組み立てを関数へ切り出す。`impl Harness` の前に追加する。

```rust
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
```

`Harness::start` の次の部分を

```rust
        let app = mock_app();
        Builder::<MockRuntime>::new()
            .events(collect_events![
                ColumnNote,
                ColumnNoteUpdated,
                ColumnNotification,
                ColumnConnectionState,
            ])
            .mount_events(&app);
        let handle = app.handle().clone();
```

次のように置き換える。

```rust
        let app = app_with_events();
        let handle = app.handle().clone();
```

- [ ] **Step 3: 失敗するテストを書く(`reconnect_tests.rs` の末尾に追加)**

```rust
#[test]
fn is_auth_failure_is_true_only_for_a_401_handshake_response() {
    use tokio_tungstenite::tungstenite::Error as WsError;
    let http_error =
        |code: u16| WsError::Http(Box::new(http::Response::builder().status(code).body(None).unwrap()));
    assert!(is_auth_failure(&http_error(401)));
    assert!(!is_auth_failure(&http_error(403)), "403 は WAF などの一時的な拒否もありうるので止めない");
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
    assert!(matches!(h.next_state("col1").await, ConnectionState::Connected));
    first.close().await;
    assert!(matches!(h.next_state("col1").await, ConnectionState::Reconnecting));
    assert!(matches!(h.next_state("col1").await, ConnectionState::Connecting));

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

    assert!(!h.task.is_finished());
}
```

- [ ] **Step 4: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib stream::connection::reconnect_tests -- --test-threads=1`
Expected: コンパイルエラー `cannot find function \`is_auth_failure\` in this scope`。

- [ ] **Step 5: 実装する(`connection.rs`)**

`RunOutcome` を置き換える。

```rust
enum RunOutcome {
    Cancelled,
    Disconnected,
    /// 認証失敗(ハンドシェイクが HTTP 401)。再試行しても直らないので、アカウントのタスクを
    /// 止める(Issue #622)。
    Fatal,
}
```

`next_backoff` の直後に追加する。

```rust
/// ハンドシェイクの失敗が、トークンの失効・取り消し(HTTP 401)によるものか。
/// 403 は含めない: リバースプロキシや WAF の一時的な拒否でも出るので、止めると自動では戻らない。
fn is_auth_failure(e: &tokio_tungstenite::tungstenite::Error) -> bool {
    use tokio_tungstenite::tungstenite::{http::StatusCode, Error};
    matches!(e, Error::Http(resp) if resp.status() == StatusCode::UNAUTHORIZED)
}
```

`connect_and_run` の接続部分を置き換える。

```rust
    let ws = match tokio_tungstenite::connect_async(request).await {
        Ok((ws, _resp)) => ws,
        Err(e) if is_auth_failure(&e) => {
            log::warn!("[{account_id}] ws handshake rejected as unauthorized: {e}");
            return RunOutcome::Fatal;
        }
        Err(e) => {
            log::warn!("[{account_id}] ws connect failed: {e}");
            return RunOutcome::Disconnected;
        }
    };
```

`run_account` の `match outcome` を置き換える。

```rust
        match outcome {
            RunOutcome::Cancelled => return,
            RunOutcome::Disconnected => {
                emit_state_all(&app, &subs, ConnectionState::Reconnecting)
            }
            RunOutcome::Fatal => {
                emit_state_all(&app, &subs, ConnectionState::Error);
                // 接続中に積まれて、まだ `subs` に入っていない購読にも Error を伝える。
                // 受信側を閉じてから回収するので、以降の送信は失敗する(= 呼び出し側が
                // タスクの停止を検出して作り直せる)。
                cmd_rx.close();
                while let Ok(cmd) = cmd_rx.try_recv() {
                    if let AccountCommand::AddChannel { column_id, .. } = cmd {
                        emit_state(&app, &column_id, ConnectionState::Error);
                    }
                }
                return;
            }
        }
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib stream::connection -- --test-threads=1`
Expected: `reconnect_tests` の 4 件の新規テストを含め、全て PASS(既存のテストも落ちない)。

- [ ] **Step 7: コミットする**

```bash
git add src-tauri/src/stream/connection.rs src-tauri/src/stream/connection/reconnect_tests.rs
git commit -m "fix(stream): ハンドシェイクが401のときは再接続を止めてErrorにする(Issue #622)"
```

---

### Task 2: 止まったアカウントのタスクを作り直す

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`(`ConnectionManager` 〜 `:129`、`add_channel` 〜 `:187-236`、`send_command` 〜 `:309`、`spawn_account` 〜 `:320`)
- Test: `src-tauri/src/stream/connection/reconnect_tests.rs`

**Interfaces:**
- Consumes: Task 1 の `MockServer::reject_with_status`・`accept_with_uri`・`app_with_events`・`fast_config()`。
- Produces: `fn try_send(tx: &mpsc::UnboundedSender<AccountCommand>, cmd: AccountCommand) -> Result<(), AccountCommand>`、`spawn_account(app, account_id, host, token, config: StreamConfig) -> AccountCtl`、`ConnectionManager.config: StreamConfig` と、テスト用 `ConnectionManager::with_config(config) -> Self`(private)。

- [ ] **Step 1: 失敗するテストを書く(`reconnect_tests.rs` の末尾に追加)**

```rust
#[tokio::test]
async fn try_send_hands_the_command_back_when_the_receiver_is_gone() {
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);

    match try_send(&tx, AccountCommand::RemoveChannel { column_id: "c1".into() }) {
        Err(AccountCommand::RemoveChannel { column_id }) => assert_eq!(column_id, "c1"),
        _ => panic!("the command must be handed back"),
    }
}

/// `acc1` のタスクが止まる(401 を受ける)まで待つ。
async fn wait_until_account_task_stops(manager: &ConnectionManager) {
    within(async {
        while !manager.accounts.lock().unwrap().get("acc1").unwrap().cmd.is_closed() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

#[tokio::test]
async fn add_channel_replaces_an_account_task_that_stopped_after_an_auth_failure() {
    let server = MockServer::bind().await;
    let app = app_with_events();
    let manager = ConnectionManager::with_config(fast_config());
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
    assert!(uri.contains("i=new-token"), "the replacement task must use the new token: {uri}");
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
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib stream::connection::reconnect_tests -- --test-threads=1`
Expected: コンパイルエラー `cannot find function \`try_send\``、`no function or associated item named \`with_config\``。

- [ ] **Step 3: 実装する(`connection.rs`)**

`ConnectionManager` に設定を持たせる。

```rust
#[derive(Default)]
pub struct ConnectionManager {
    /// account_id -> そのアカウントの WebSocket タスク
    accounts: Mutex<HashMap<String, AccountCtl>>,
    /// column_id -> account_id（close/capture のルーティング用）
    columns: Mutex<HashMap<String, String>>,
    /// タスクへ渡す接続設定。本番は `StreamConfig::default()`、テストは短い時間と `ws` を渡す。
    config: StreamConfig,
}
```

`impl ConnectionManager {` の先頭に追加する。

```rust
    #[cfg(test)]
    fn with_config(config: StreamConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }
```

`add_channel` の本体(`self.columns...insert` の後)を置き換える。

```rust
        let mut accounts = self.accounts.lock().unwrap();
        // 認証失敗などで止まったタスクは捨てて、渡されたトークン(呼び出し側が取り直した最新のもの)で
        // 作り直す。止まったタスクへ送ると購読が黙って失われる(Issue #622)。
        if accounts.get(&account_id).is_some_and(|ctl| ctl.cmd.is_closed()) {
            accounts.remove(&account_id);
        }
        let cmd = AccountCommand::AddChannel {
            sub_key,
            column_id,
            channel,
            params,
            initial_last_seen_id,
            mode,
        };
        let ctl = accounts.entry(account_id.clone()).or_insert_with(|| {
            spawn_account(
                app.clone(),
                account_id.clone(),
                host.clone(),
                token.clone(),
                self.config.clone(),
            )
        });
        // `is_closed()` の確認から送信までの間にタスクが止まった場合は、作り直して 1 回だけ送り直す。
        if let Err(cmd) = try_send(&ctl.cmd, cmd) {
            let ctl = spawn_account(app, account_id.clone(), host, token, self.config.clone());
            send_command(&account_id, &ctl.cmd, cmd);
            accounts.insert(account_id, ctl);
        }
```

`send_command` を `try_send` の上に載せ替える(既存の doc コメントはそのまま残す)。

```rust
/// 送れなかったコマンドを呼び出し側へ返す。受信側が無い(タスクが終了済み)ときだけ失敗する。
fn try_send(
    tx: &mpsc::UnboundedSender<AccountCommand>,
    cmd: AccountCommand,
) -> Result<(), AccountCommand> {
    tx.send(cmd).map_err(|e| e.0)
}

fn send_command(
    account_id: &str,
    tx: &mpsc::UnboundedSender<AccountCommand>,
    cmd: AccountCommand,
) -> bool {
    if try_send(tx, cmd).is_err() {
        log::warn!("[{account_id}] stream command dropped: account task is gone");
        return false;
    }
    true
}
```

`spawn_account` に設定を引数で渡す。

```rust
fn spawn_account<R: Runtime>(
    app: AppHandle<R>,
    account_id: String,
    host: String,
    token: String,
    config: StreamConfig,
) -> AccountCtl {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let handle = tauri::async_runtime::spawn(async move {
        run_account(app, account_id, host, token, config, cancel_rx, cmd_rx).await;
    });
    AccountCtl {
        cmd: cmd_tx,
        cancel: cancel_tx,
        handle,
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib stream::connection -- --test-threads=1`
Expected: 全て PASS(#621 の `send_command_*` テストも通る)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/stream/connection.rs src-tauri/src/stream/connection/reconnect_tests.rs
git commit -m "fix(stream): 認証失敗で止まったアカウントのタスクを購読の追加時に作り直す(Issue #622)"
```

---

### Task 3: `reconnect_column` コマンド

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`resume_column` の直後に追加。テストは `mod tests` 内の `resume_column_core_serves_the_cache_when_the_boundary_is_set` の直後)
- Modify: `src-tauri/src/lib.rs:60`(`commands::column::resume_column,` の次の行)
- Generated: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `load_column`、`resolve_sources`、`open_streams_only`、`gap_fill_on_reconnect`(いずれも `column.rs` 内の既存関数)。`ConnectionManager::open_notifications`(Task 2 で作り直しに対応済み)。
- Produces: `commands.reconnectColumn(columnId: string): Promise<Result<null, Error>>`(生成された TS)。`async fn reconnect_column_core<R: Runtime>(app: &AppHandle<R>, state: &AppState, column_id: &str) -> Result<()>`。

- [ ] **Step 1: 失敗するテストを書く(`column.rs` の `mod tests`)**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_column_core_opens_the_stream_without_fetching_the_first_page() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Local)).unwrap();
        let app = tauri::test::mock_app();

        reconnect_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(state.connections.open_count(), 1, "ストリームを開く");
        assert!(
            state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(),
            "初期取得はしないので、境界は作られない"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_column_core_opens_the_stream_for_a_notifications_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Notifications)).unwrap();
        let app = tauri::test::mock_app();

        reconnect_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(state.connections.open_count(), 1);
        assert_eq!(mock.received_requests().await.unwrap().len(), 0, "REST は呼ばない");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_column_core_rejects_an_unknown_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        let app = tauri::test::mock_app();

        assert!(reconnect_column_core(app.handle(), &state, "closed").await.is_err());
        assert_eq!(state.connections.open_count(), 0);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib commands::column::tests::reconnect_column_core`
Expected: コンパイルエラー `cannot find function \`reconnect_column_core\``。

- [ ] **Step 3: 実装する(`column.rs`、`resume_column` の直後)**

```rust
/// 認証失敗で止まった(`Error` の)カラムのストリームだけを張り直す(Issue #622)。
/// 再認証の完了後にフロントが呼ぶ。初期ページの取得や `OpenedColumn` の組み立てはしない
/// (タブは既に表示されている)。
#[tauri::command]
#[specta::specta]
pub async fn reconnect_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
) -> Result<()> {
    reconnect_column_core(&app, &state, &column_id).await
}

async fn reconnect_column_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column_id: &str,
) -> Result<()> {
    // 世代(epoch)は進めない: カラム定義は変わらず、進行中の取得を無効にする理由が無い。
    let column = load_column(state, column_id)?;
    let (host, token) = state.host_token(&column.account_id)?;
    if matches!(column.kind, ColumnKind::Notifications) {
        // 通知のギャップ埋めは、止まったタスクのウォーターマークと一緒に失われる(Issue #684)。
        state.connections.open_notifications(
            app.clone(),
            column.id.clone(),
            column.account_id.clone(),
            host,
            token,
            None,
        );
        return Ok(());
    }
    let resolved = resolve_sources(state, &column.account_id, &column.kind, &column.filter).await?;
    open_streams_only(app, state, &column, &resolved, host, token);
    // 新しいタスクの最初の接続は再接続ではないので、停止中に流れたノートはここで補う。
    let app = app.clone();
    let column_id = column.id.clone();
    tauri::async_runtime::spawn(async move {
        gap_fill_on_reconnect(&app, &column_id).await;
    });
    Ok(())
}
```

`lib.rs:60` の `commands::column::resume_column,` の次の行に追加する。

```rust
            commands::column::reconnect_column,
```

- [ ] **Step 4: テストが通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib commands::column::tests::reconnect_column_core && cargo test generates_frontend_bindings`
Expected: PASS。`git diff frontend/src/bindings/tauri.gen.ts` に `reconnectColumn: (columnId: string) => typedError<null, Error>(__TAURI_INVOKE("reconnect_column", { columnId })),` が出る(他の行は変わらない)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat(column): Errorのカラムのストリームを張り直すreconnect_columnを追加する(Issue #622)"
```

---

### Task 4: フロント(再認証の導線と、再認証後の再接続)

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(import `:27` 付近、`#wasReconnecting` の直後 `:286`、`#subscribe` の `columnConnectionState` リスナー `:1137-1157`、`completeAccount` `:1325-`)
- Create: `frontend/src/lib/store.svelte.connectionError.test.ts`

**Interfaces:**
- Consumes: Task 3 の `commands.reconnectColumn(columnId)`。
- Produces: `AppStore.applyConnectionState(payload: ColumnConnectionState): void`(public。リスナーの本体で、テストから呼ぶ)。

- [ ] **Step 1: 失敗するテストを書く(`store.svelte.connectionError.test.ts`)**

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account } from "../bindings/tauri.gen";
import type { TabView } from "./store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue({ status: "ok", data: null }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");
const { commands } = await import("./ipc");

const account: Account = {
  id: "acc1",
  host: "misskey.test",
  username: "me",
  userId: "u1",
  displayName: "Me",
  avatarUrl: null,
  instance: null,
  isCat: false,
  avatarBlurhash: null,
};
const other: Account = { ...account, id: "acc2", username: "you", userId: "u2" };

function tab(id: string, accountId: string, state: TabView["state"]): TabView {
  return {
    id,
    accountId,
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" },
    notes: [],
    notifications: [],
    state,
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    selectionMoveSeq: 0,
    epoch: 0,
  };
}

function setTabs(...tabs: TabView[]) {
  app.groups = [{ id: "g1", width: 300, auto: false, tabs, activeTabId: tabs[0].id }];
}

function authLogs() {
  return app.logs.filter((l) => l.reauthAccountId !== undefined);
}

beforeEach(() => {
  app.accounts = [account, other];
  app.logs = [];
  vi.spyOn(commands, "completeMiauth").mockResolvedValue({ status: "ok", data: account } as never);
  vi.spyOn(commands, "syncServerMutes").mockResolvedValue({
    status: "ok",
    data: { blockedUsers: 0, wordRules: 0 },
  } as never);
  vi.spyOn(commands, "listColumns").mockResolvedValue({ status: "ok", data: [] } as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  app.groups = [];
  app.accounts = [];
  app.logs = [];
});

describe("認証失敗でカラムがErrorになったとき(Issue #622)", () => {
  it("再認証ボタン付きのログを、アカウントごとに1回だけ出す", () => {
    setTabs(tab("c1", "acc1", "connected"), tab("c2", "acc1", "connected"), tab("c3", "acc2", "connected"));

    app.applyConnectionState({ columnId: "c1", state: "error" });
    app.applyConnectionState({ columnId: "c2", state: "error" }); // 同じアカウントの2つ目
    app.applyConnectionState({ columnId: "c3", state: "error" }); // 別アカウント

    expect(authLogs().map((l) => l.reauthAccountId).sort()).toEqual(["acc1", "acc2"]);
    expect(authLogs().find((l) => l.reauthAccountId === "acc1")?.text).toBe(
      "認証に失敗しました: @me@misskey.test のトークンが失効している可能性があります",
    );
    expect(app.groups[0].tabs.map((t) => t.state)).toEqual(["error", "error", "error"]);
  });

  it("Error以外の状態ではログを出さない", () => {
    setTabs(tab("c1", "acc1", "connected"));

    app.applyConnectionState({ columnId: "c1", state: "reconnecting" });

    expect(authLogs()).toEqual([]);
  });
});

describe("再認証したあとのストリームの再接続(Issue #622)", () => {
  it("そのアカウントのErrorのタブにだけ reconnectColumn を呼ぶ", async () => {
    setTabs(
      tab("c1", "acc1", "error"),
      tab("c2", "acc1", "connected"),
      tab("c3", "acc2", "error"),
    );
    const reconnect = vi.spyOn(commands, "reconnectColumn").mockResolvedValue({ status: "ok", data: null } as never);

    await app.completeAccount("session1");

    expect(reconnect).toHaveBeenCalledTimes(1);
    expect(reconnect).toHaveBeenCalledWith("c1");
  });

  it("再認証したのにまた失敗したら、もう一度ログを出す(重複抑止は再認証でリセットされる)", async () => {
    setTabs(tab("c1", "acc1", "connected"));
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({ status: "ok", data: null } as never);
    app.applyConnectionState({ columnId: "c1", state: "error" });
    expect(authLogs()).toHaveLength(1);

    await app.completeAccount("session1");
    app.applyConnectionState({ columnId: "c1", state: "error" });

    expect(authLogs()).toHaveLength(2);
  });

  it("reconnectColumn が失敗しても、再認証自体は成功のままにする", async () => {
    setTabs(tab("c1", "acc1", "error"));
    vi.spyOn(commands, "reconnectColumn").mockResolvedValue({
      status: "error",
      error: { kind: "unauthorized", message: "no token" },
    } as never);

    await expect(app.completeAccount("session1")).resolves.toBeUndefined();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.connectionError.test.ts`
Expected: FAIL(`app.applyConnectionState is not a function`)。

- [ ] **Step 3: 実装する(`store.svelte.ts`)**

bindings の import 一覧(`ConnectionState,` がある `:27` 付近)に追加する。

```ts
  ColumnConnectionState,
```

`#wasReconnecting` の定義(`:286`)の直後に追加する。

```ts
  // 認証失敗(接続の Error)のログを出したアカウントの集合。同じアカウントの複数カラムが
  // 同時に Error になるので、ログと「再認証」ボタンはアカウントごとに 1 回だけ出す(Issue #622)。
  // そのアカウントの再認証が成功したら外す(再認証したのにまた失敗したら、もう一度出す)。
  #authErrorLogged = new Set<string>();
```

`async #subscribe()` の直前に、リスナーの本体をメソッドとして追加する。

```ts
  /// ColumnConnectionState イベントの本体。`Error` は、ストリームのハンドシェイクが認証失敗(401)で
  /// 拒否されて Rust 側が再接続を止めたことを表す(他に Error になる経路は無い。Issue #622)。
  applyConnectionState(p: ColumnConnectionState) {
    // タブがまだ #insertTab 前でも(resumeColumn/addColumn の await 解決前に
    // Connected が届くことがある)状態を取りこぼさないよう、まず記録する。
    this.#connState.set(p.columnId, p.state);
    const tab = this.#findTab(p.columnId);
    if (!tab) return;
    tab.state = p.state;
    // 状態遷移を Backstage に記録（起動時の connecting→connected は除外）
    const name = tab.title;
    if (p.state === "error") {
      if (this.#authErrorLogged.has(tab.accountId)) return;
      this.#authErrorLogged.add(tab.accountId);
      const acc = this.accounts.find((a) => a.id === tab.accountId);
      const who = acc ? `@${acc.username}@${acc.host}` : tab.accountId;
      this.#log("error", `認証に失敗しました: ${who} のトークンが失効している可能性があります`, tab.accountId);
    } else if (p.state === "reconnecting") {
      this.#wasReconnecting.add(p.columnId);
      this.#log("warn", `再接続中: ${name}`);
    } else if (p.state === "connected" && this.#wasReconnecting.delete(p.columnId)) {
      this.#log("success", `再接続しました: ${name}`);
    }
  }
```

`#subscribe` 内の `columnConnectionState` リスナー(`this.#unlisten.push(` から `);` までのブロック)を、次の 1 つに置き換える。

```ts
    this.#unlisten.push(
      await events.columnConnectionState.listen((e) => this.applyConnectionState(e.payload)),
    );
```

`#resumeAccountColumns` の直後に追加する。

```ts
  /// 再認証の直後に、そのアカウントの接続が Error で止まっているカラムのストリームを張り直す
  /// (Issue #622)。Rust 側は認証失敗でタスクごと止めているので、新しいトークンで作り直させる。
  /// 失敗はログのみ(再認証自体の成否には影響させない)。
  async #reconnectErrorColumns(accountId: string) {
    const targets = this.groups
      .flatMap((g) => g.tabs)
      .filter((t) => t.accountId === accountId && t.state === "error");
    for (const t of targets) {
      try {
        await unwrap(commands.reconnectColumn(t.id));
      } catch (e) {
        this.#logFailure(e);
      }
    }
  }
```

`completeAccount` を次のように変える(`this.#clearNeedsLogin(account.id);` の直後に 1 行、末尾に 1 行)。

```ts
    this.#clearNeedsLogin(account.id); // 認証が通ったので、通信の対象に戻す
    this.#authErrorLogged.delete(account.id); // 再認証したのにまた失敗したら、ログをもう一度出す
    // 起動時に取らなかった(トークンなしだった)インスタンス情報を取る。再認証自体は待たせない。
    void this.#refreshInstanceMeta([account]);
    await this.#syncServerMutes(account.id);
    await this.#resumeAccountColumns(account.id);
    await this.#reconnectErrorColumns(account.id);
  }
```

- [ ] **Step 4: テストと型チェックが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.connectionError.test.ts src/lib/store.svelte.resumeColumns.test.ts src/lib/store.svelte.needsLogin.test.ts && pnpm check`
Expected: 全て PASS、`svelte-check` のエラー 0。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.connectionError.test.ts
git commit -m "feat(frontend): 認証失敗の再認証導線と、再認証後のストリーム再接続を追加する(Issue #622)"
```

---

### Task 5: ドキュメントと実画面の確認

**Files:**
- Modify: `CLAUDE.md`(`stream/` の説明、`:61`)
- Modify: `docs/guide/user-guide.md:210`

- [ ] **Step 1: `CLAUDE.md` を更新する**

`stream/` の行の `connection.rs (one WS per account, ping/pong + backoff reconnect)` を次に置き換える。

```
`connection.rs` (one WS per account, ping/pong + backoff reconnect; a handshake rejected with HTTP 401 — only 401, 403 can be a transient WAF block — is `RunOutcome::Fatal`: the columns go to `Error`, the task stops retrying and exits, and `ConnectionManager::add_channel` replaces a stopped task (`cmd.is_closed()`) with a fresh one using the token it is given; the frontend calls `reconnect_column` for `error` tabs after re-auth, Issue #622. A recovered notifications column does not backfill missed notifications, Issue #684)
```

- [ ] **Step 2: ユーザーガイドを更新する**

`docs/guide/user-guide.md:210` を次に置き換える。

```
- 各タブ名の左のドットの色が接続状態を表します（接続済み/接続中・再接続中/エラー）。エラー(赤)は、トークンの失効や取り消しなどで認証に失敗し、再接続を止めた状態です。下部の Backstage に「認証に失敗しました」と出るので、横の「再認証」から認可し直してください。完了すると、エラーだったカラムが自動で接続し直します。
```

- [ ] **Step 3: 全体のテストを流す**

Run: `cd src-tauri && cargo test --lib stream:: commands::column:: -- --test-threads=1 && cd ../frontend && pnpm check && pnpm test`
Expected: 全て PASS。

- [ ] **Step 4: 実画面で確認する(手動。自動テストにはしない)**

`e2e/README.md` の手順で Docker Compose の使い捨て Misskey を起動し、実 UI の確認は必ず **Xvfb 越し**(`WAYLAND_DISPLAY` も unset、`dbus-run-session` 経由)でアプリを起動する。

1. そのアカウントで、ホームカラムと通知カラムを開き、`Connected`(緑)を確認する。
2. Misskey の管理画面(または `i/revoke-token`)でアクセストークンを取り消し、アプリ側の接続を切る(サーバーのコンテナ再起動で切れる)。
3. カラムのドットが赤(`Error`)になり、Backstage に「認証に失敗しました: …」と「再認証」ボタンが出ること。複数カラムでもログは 1 つだけであること。サーバー側のログに、再接続の試行が続いていないこと。
4. 「再認証」から MiAuth をやり直し、`Error` だったカラムが `Connected` に戻り、新しいノートが流れること。通知カラムは復帰後の新しい通知が流れること(停止中の通知の補完は Issue #684 のため対象外)。
5. 起動したプロセスは PID を指定して止める(`pkill` / `killall` は使わない)。コンテナは `docker rm -fv <id>` で消す。

- [ ] **Step 5: コミットする**

```bash
git add CLAUDE.md docs/guide/user-guide.md
git commit -m "docs: 認証失敗時の挙動をCLAUDE.mdとユーザーガイドに追記する(Issue #622)"
```

- [ ] **Step 6: PR(ユーザーの指示があったときだけ)**

`git push -u origin fix/issue-622-stream-auth-fatal` のあと、`.github/pull_request_template.md` の構成に沿った本文を手で書いて `gh pr create` する。本文に `Fixes #622` を入れる(`#684` は参照のみ)。マージは `gh pr merge --merge`。CI の結果は待たない(ユーザーが見る)。
