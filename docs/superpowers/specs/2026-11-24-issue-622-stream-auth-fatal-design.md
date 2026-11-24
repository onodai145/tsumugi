# ストリーミングの認証失敗を `Error` にして再接続を止め、再認証で復帰させる (Issue #622)

`stream/connection.rs` の `RunOutcome::Fatal` は、構築している箇所が無く到達不能になっている(`#[allow(dead_code)]` 付き)。トークンが失効・取り消しされたアカウントでも、ハンドシェイクの失敗は一律 `Disconnected` として扱われ、30 秒上限のバックオフで永久に再試行される。UI は `Reconnecting` のままで `Error` にならず、再認証の導線も出ない。

この spec は、**認証失敗を検出して再試行を止め、`Error` を表示し、再認証したら元に戻せる**ところまでを扱う。

## 決定事項(合意済み)

- Issue の「修正の方向」だけでなく、停止後の後始末と復帰まで含めて直す(案 B)。
- 認証失敗のときは再接続を**止める**(`Error` を出して、そのまま再試行を続けることはしない)。

## 目標 / 非目標

目標:

- ハンドシェイクが認証失敗(HTTP 401)で終わったら、そのアカウントの再接続を止め、該当カラムを `Error` にする。
- 認証失敗以外(ネットワーク断・5xx・タイムアウトなど)は、従来どおり `Reconnecting` で再試行を続ける。
- `Error` になったカラムについて、Backstage のログから「再認証」へ進める。
- 再認証が通ったら、再起動なしで、`Error` のカラムのストリームを新しいトークンで張り直す。
- 止まったアカウントのタスクが `ConnectionManager` に残って、以降の購読の追加が黙って失われることがない。

非目標:

- 再認証の時点で正常に動いているカラムのストリームの張り直し(古いトークンのまま動き続ける。トークンが取り消されていれば、次の切断で 401 になり、この spec の経路に入る)。
- `Error` から復帰した通知カラムの、停止中に届いた通知の補完(下記「既知の制限」)。
- 認証失敗の種類の区別(失効・取り消し・権限不足を分けて表示しない)。
- `needsLogin`(トークンがそもそも無いアカウント)との統合。別の概念なので触らない。

## 現状(調査結果)

1. `connect_and_run` の `connect_async` が `Err` を返すと、中身を見ずに `Disconnected` を返す(`connection.rs:573-578`)。tungstenite 0.30 は HTTP でのハンドシェイク拒否を `Error::Http(Box<Response<Option<Vec<u8>>>>)` で返すので、`status()` で見分けられる。
2. `run_account` は、`Fatal` の腕で `Error` を emit したあと、`Disconnected` と同じくバックオフして再試行する。つまり `Fatal` を返すようにしただけでは、再試行が止まらない。
3. `spawn_account` が起動時のトークンをタスクへ固定する。`complete_miauth`(再認証)は keyring を差し替えるだけで、動いているタスクには届かない。
4. `ConnectionManager.accounts` は `account_id -> AccountCtl`。`add_channel` は `entry().or_insert_with(spawn_account)` なので、タスクが終わった後も残った `AccountCtl` が再利用される。送信先の受信側は無く、`send_command` が警告を出して購読を捨てる。
5. フロントの `#resumeAccountColumns` は、画面に出ていないカラムしか開き直さない。`Error` で表示中のカラムは対象外。
6. `Error` 状態のログ(`接続エラー: <名前>`)には `reauthAccountId` が付かず、Backstage の「再認証」ボタンが出ない。403 の API エラーだけがボタンを出せる。

## 設計

### 1. 認証失敗の判定 (`stream/connection.rs`)

純粋関数 `is_auth_failure(&tungstenite::Error) -> bool` を足す。`Error::Http(resp)` で `resp.status() == 401` のときだけ `true`。

`connect_and_run` は、`connect_async` の `Err` が `is_auth_failure` なら `RunOutcome::Fatal` を返す。それ以外は従来どおり `Disconnected`。`RunOutcome::Fatal` の `#[allow(dead_code)]` は外す。

**403 を認証失敗に含めない。** Issue は 401 / 403 としているが、ハンドシェイクの 403 は、リバースプロキシや WAF(Cloudflare など)がブロックしている場合にも出る。その場合は一時的で、待てば回復しうる。誤って止めると、ユーザーが気づくまで二度と繋がらない。誤って再試行を続ける害(いまと同じ挙動)のほうが小さいので、確実な 401 だけを止める。Misskey のストリーミングが認証失敗に 401 を返すことは、実装前に実インスタンスで確認する(未確認。Misskey のソースから確かめる、または失効トークンで接続して応答を見る)。違っていたら、この判定を見直す。

### 2. `Fatal` で止まる (`run_account`)

`Fatal` を受けたら、次の順で処理してタスクを終える。

1. `subs` に登録済みのカラム全てへ `ConnectionState::Error` を emit する(従来の `emit_state_all`)。
2. `cmd_rx.close()` を呼ぶ。以降の送信は即座に失敗し、`UnboundedSender::is_closed()` が `true` になる。
3. `cmd_rx` に溜まっている未処理の `AddChannel` を `try_recv` で全て取り出し、そのカラムにも `Error` を emit する(`connect_async` の最中に積まれたコマンドは `subs` に入っておらず、放っておくと `Connecting` のまま残るため)。
4. `return` する。バックオフ待ちには入らない。

**close を回収より先にする理由**: 回収してから close すると、その間に届いたコマンドは、回収にも漏れ、送信にも成功して捨てられる(失われる)。先に close すれば、コマンドは「バッファに入って回収される」か「送信が失敗して呼び出し側へ返る(§3 の作り直し)」のどちらかになる。

### 3. 止まったタスクの後始末 (`ConnectionManager`)

- `add_channel` は、`accounts` に既存の `AccountCtl` があっても、`cmd.is_closed()` なら**捨てて新しく `spawn_account` する**。トークンは `add_channel` に渡されたもの(呼び出し側が `host_token` で取り直した最新のもの)を使う。
- `send_command` は、送れなかったコマンドを `Err(AccountCommand)` で返す(`mpsc::SendError` の中身)。`add_channel` は、`is_closed()` の確認と送信の間にタスクが終わった場合に備え、送信が失敗したら `AccountCtl` を作り直して同じコマンドを 1 回だけ送り直す。
- `close(column_id)` の既存の動き(最後のカラムなら `cancel` と `abort`、残りがあれば `RemoveChannel` を送る)は変えない。止まったタスクへの `RemoveChannel` は、警告ログが出るだけで害はない。

`spawn_account` は `StreamConfig` を引数に取る形にして(本番は `StreamConfig::default()`)、テストから短い時間とモックサーバー向けの設定(`scheme: "ws"`)を渡せるようにする。

### 4. 再接続のコマンド (`commands/column.rs`)

`Error` のカラムのストリームだけを張り直す新しいコマンド `reconnect_column(column_id)` を足す。`lib.rs` の `specta_builder()` に登録する。

- カラム定義と `host_token` を読み、ストリームを開く。通知カラムは `open_notifications`(`initial_last_seen_id` は `None`)、それ以外は `resolve_sources` のあと `open_streams_only`(既存)。
- 初期ページの取得や `OpenedColumn` の組み立てはしない(タブは既に表示されている)。
- ノートを流すカラムは、ストリームを開いたあと `gap_fill_on_reconnect` をバックグラウンドで起動して、停止中に流れたノートを補う(新しいタスクの最初の接続は `is_reconnect == false` なので、`connect_and_run` 側のギャップ埋めは走らないため)。
- `Error` 以外のカラムには呼ばない(呼び出し側のフロントが絞る)。コマンド自体は、既に開いているカラムに呼ばれても、`add_channel` が同じ `sub_key` を張り替えるだけで害がない。

テストのため、本体は `AppHandle` と `AppState` を引数に取る `reconnect_column_core` にする(`resume_column_core` と同じ形)。

**古い定義のストリームは開かない(世代の確認)**: `reconnect_column_core` は `begin_and_load_column` で世代(epoch)を控え(`begin` は世代を進めないので、進行中の取得を無効にしない)、本体の `reconnect_column_with(app, state, column, epoch)` が、ストリームを開く部分を `ColumnFence::write_if_current` で守る。世代を控えたあとに `update_column` / `close_column` が走った(世代が変わった)なら、何も開かず `Ok(())` を返す(開き直し・閉じる責任は、その呼び出し側にある)。開くと、旧い定義の購読(例: 増えすぎたソース)が残って、別のノートをカラムへ流し込みうる。

### 5. フロント (`lib/store.svelte.ts`)

- `columnConnectionState` で `error` を受けたとき、ログを `接続エラー` から次の文面へ変える: `認証に失敗しました: <アカウント> のトークンが失効している可能性があります`。`reauthAccountId` にタブの `accountId` を渡して、Backstage の「再認証」ボタンを出す。
- 同じアカウントのカラムが複数同時に `Error` になるので、ログは**アカウントごとに、そのログが残っている間は 1 回**だけ出す。アカウント ID → 出したログの id の `Map`(`#authErrorLogId`)を持ち、そのログがまだ `logs` に残っているときだけ抑止する(ログが上限で流れ落ちた・クリアされたあとの次の `Error` では、もう一度出す)。そのアカウントの再認証が成功したら(`completeAccount`)、エントリを外す。
- `Error` が、タブの挿入より先に届くことがある(`#connState` に記録されるだけで、タブが無いのでログを出せない)。`#insertTab` が、作ったタブの状態が `error` なら、同じ導線(`#logAuthError`)を出す。
- `completeAccount` の最後(`#resumeAccountColumns` の後)に、`#reconnectErrorColumns(accountId)` を呼ぶ。そのアカウントのタブのうち `state === "error"` のものへ `commands.reconnectColumn(tab.id)` を呼ぶ。失敗はログのみ(再認証自体の成否には影響させない。`#resumeAccountColumns` と同じ)。
- `reconnectColumn` が成功したタブは、表示中のノートを `captureNotes` で張り直す(`#captureInitial`)。止まったタスクの購読(キャプチャ)は一緒に消えているので、張り直さないと、表示中のノートのリアクション・投票・削除が、アプリを再起動するまで反映されない。通常の再接続は、タスクが購読を保持して再購読するので、これは不要。

### 6. Rust 側のエラー通知は増やさない

アカウント単位の新しいイベントは足さない。`ColumnConnectionState { Error }` がそのまま「認証失敗」を意味する(`Fatal` は認証失敗でしか返らず、`Error` を emit する経路は他に無い)。`ConnectionState` の列挙は変えない。将来、認証以外の `Fatal` を足すときは、そのとき種類を持たせる。

## エラー処理と境界

- 失効したトークンで 401 → 認証失敗として止まる。ネットワークが無いときは `Error::Io` などになり、`Disconnected` のまま。
- 一度つながったあとにトークンが失効した場合: サーバーは接続を切る(`Close` か EOF)ので `Disconnected` になり、再接続のハンドシェイクで 401 を受けて `Fatal` に入る。
- 再認証したが、まだ古い 401 が返る場合(トークンが反映されていないなど): `reconnect_column` で開いたタスクがまた 401 を受けて `Error` に戻る。ユーザーは再度再認証できる。無限ループにはならない(1 回の再認証につき、`reconnect_column` は `Error` のカラムに 1 回呼ばれるだけ)。
- `reconnect_column` が `host_token` で `Unauthorized`(トークンが無い)を返した場合: ログのみ。`needsLogin` の経路が別に面倒を見る。

## 既知の制限

- `Error` から復帰した**通知カラム**は、停止中に届いた通知を補完しない。通知のギャップ埋めは、タスクが保持するウォーターマーク(`last_seen_notification_id`)に依存する。止まったタスクと一緒に失われるため。復帰後の新しい通知は流れてくる。必要になったら別 Issue にする。
- 再認証の前に、正常に動いていたカラムは古いトークンのまま動き続ける(非目標)。

## テスト

Rust(`stream/connection/reconnect_tests.rs` の `MockServer` を拡張して使う。`accept_hdr_async` のコールバックでハンドシェイクを 401 で拒否する `reject_with_status(code)` を足す):

- `is_auth_failure`: 401 は `true`、403・500・`Io` エラーは `false`(純粋関数のテスト)。
- 認証失敗: 401 で拒否すると、カラムへ `Error` が emit され、タスクが終了し、その後サーバーへ再接続が来ない(`accept` がタイムアウトする)。
- 認証失敗(保留中のコマンド): 接続前に積まれた 2 つ目のカラムの `AddChannel` にも `Error` が emit される。
- 認証失敗以外: 500 で拒否しても `Reconnecting` となり、再試行が続く(`Fatal` にならない)。
- `ConnectionManager`: 認証失敗で止まったあと、同じアカウントへ `add_channel` すると新しいタスクが作られ、購読がモックサーバーへ届く(`is_closed` での差し替え)。
- `reconnect_column_core`: ストリームが開く(`open_count`)、通知カラムとノートカラムの両方。

フロント(Vitest):

- `error` を受けると、`reauthAccountId` 付きのログが 1 つ出る。同じアカウントの 2 つ目のカラムが `error` になっても、ログは増えない。
- `completeAccount` が、`error` のタブにだけ `reconnectColumn` を呼ぶ(`connected` のタブには呼ばない)。
- `reconnectColumn` の失敗は、`completeAccount` を失敗させない。

実画面の確認(自動テストにはしない。実装後に手動で行う):

- Docker の Misskey(`e2e/` の Compose)で、アカウントのトークンを管理画面から取り消し、`Error` と「再認証」が出ること、再認証後にカラムが `Connected` へ戻ることを実画面で確認する。

## 影響範囲

- `src-tauri/src/stream/connection.rs`: 判定関数、`Fatal` の処理、`add_channel` / `send_command` / `spawn_account` の変更。
- `src-tauri/src/commands/column.rs`、`src-tauri/src/lib.rs`: `reconnect_column` の追加と登録。
- `frontend/src/lib/store.svelte.ts`: ログ文面、アカウント単位の重複抑止、`completeAccount` からの再接続。
- `frontend/src/bindings/tauri.gen.ts`: 再生成(手で編集しない)。
- ドキュメント: `CLAUDE.md` の `stream/` の説明に、認証失敗で止まる挙動を 1 文足す。ユーザー向けの `docs/guide/user-guide.md` に、認証失敗の表示と再認証の手順があれば合わせる。
