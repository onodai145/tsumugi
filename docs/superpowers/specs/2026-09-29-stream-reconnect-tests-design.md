# Streaming/WebSocket再接続の自動テスト（Issue #224）

Issue #73「テストをちゃんとやる」のサブタスク。

## 背景

`stream/connection.rs`（ping/pong・backoff再接続）と`stream/inbox.rs`（重複排除）は、ロジック単体のテストはあったが、実際の「切断→再接続→購読復元」を通しで検証する自動テストが無かった。実Misskeyサーバー相手では再現性が低いため、テスト内で立てたモックWebSocketサーバーで検証する。

## 前提: そのままではテストできなかった点

1. 接続先が`wss://{host}/streaming`固定で、平文の`ws://127.0.0.1:PORT`に向けられない。
2. `run_account` / `connect_and_run` / `handle_text`が具象型`AppHandle`（Wry）を直接受け取っていた。ヘッドレスのテストでWryの`AppHandle`は作れず、`tauri::test::mock_app()`が返す`AppHandle<MockRuntime>`とは型が合わない。

## 検討した案（AppHandle対策）

- **Runtimeジェネリック化（採用）**: `AppHandle<R: Runtime>`にして`mock_app()`を使う。`emit`/`listen`まで実物で検証できる。波及は`commands/column.rs`のgap fill系（`gap_fill_on_reconnect` / `notification_gap_fill_on_reconnect` / `GapFillGuard`）に限られ、`#[tauri::command]`側はWryのまま。
- 出力口traitの切り出し: 波及は`connection.rs`内で済むが、`handle_text`の`try_state(AppState)`依存もtrait化が要り、抽象が増える。
- ワイヤ層のみ分離: テストは軽いが、`handle_text`経由のemitやgap fill呼び出しが検証範囲外になる。

## 設計

- `StreamConfig`（スキーム・backoff開始/上限・ping間隔・read timeout）を`run_account`へ注入する。`Default`は本番値（`wss`と1s/30s/25s/65s）。`cfg(test)`分岐で本番コードに平文`ws://`の経路を残さないため、スキームも設定として渡す。
- backoffの「倍々＋上限」は純関数`next_backoff`へ切り出し、決定的に単体テストする。
- テストは`connection.rs`の子モジュール`connection/reconnect_tests.rs`に置く（親のprivate項目へ可視性を変えずにアクセスするため。`connection.rs`が既に1200行超なので別ファイルにした）。
- モックサーバーは`TcpListener` + `tokio_tungstenite::accept_async`。依存の追加は無く、`tauri`の`test` featureをdev-dependenciesに足すのみ。

## テストシナリオ

1. 初回接続で`connect`（channel/params）が届き、`Connected`がemitされる
2. サーバー切断で`Reconnecting`→`Connecting`→`Connected`と遷移し、全チャンネルの`connect`が旧sub_idと異なる新sub_idで再送される
3. キャプチャ済みノートの`subNote`が、`connect`の後に登録順で再送される
4. 再接続後は新sub_id宛てのノートだけが`ColumnNote`になり、旧sub_id宛ては捨てられる
5. 再接続をまたいでDedupが保持され、同じノートIDの再配信は弾かれる（`inbox.rs`対象）
6. 接続失敗が続くとbackoffが伸び、上限で頭打ちになる（`next_backoff`単体＋試行間隔の下限）。接続確立後はリセットされる
7. サーバーが無応答のままread timeoutを超えると切断とみなして再接続する
8. cancelで`disconnect`を送ってタスクが終了する

## スコープ外

- 再接続時にgap fillが呼ばれること自体の検証。テストのappにはAppStateをmanageしないため、gap fillは早期returnする。
- 実Misskey接続テスト、E2E。

## 実装中に分かったこと

- タイミング依存の検証は原則として下限のみ見る（上限はCI負荷でflakyになるため）。上限は`next_backoff`の単体テストが担う。例外は「接続確立後にbackoffがリセットされる」テストで、リセット時の約20msと未リセット時の640ms以上を大きく離し、閾値300msで判定する。
- テストヘルパーの待ちは**合計の期限**で打ち切る。フレーム単位のタイムアウトだと、クライアントが`fast_config`で50msごとに送るPingのたびに待ちが延び、期待するフレームが来ないときにテストが失敗せずハングする（変異テストで発覚。`recv_json_gives_up_even_while_pings_keep_arriving`が回帰テスト）。
- 変異テスト（本番コードを意図的に壊して失敗することを確認）で、sub_id再発行・backoffリセット・read timeout・backoff上限を壊した場合に各テストが失敗することを確認した。キャプチャ再購読を壊した場合は、当初は上記のハングで失敗せず止まったが、修正後に別ワークツリーで再実行し、`resubscribes_captured_notes_after_reconnect`が約5秒（待ちの上限）で失敗することを確認した。
