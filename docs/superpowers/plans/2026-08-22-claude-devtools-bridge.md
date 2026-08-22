# Claudeがフロントのdevtoolsを見られるようにするデバッグブリッジ Implementation Plan

> **注記(後付け作成):** 本来は実装前に本ドキュメントをタスク分割して書くべきところ、設計確定(`docs/superpowers/specs/2026-08-22-claude-devtools-bridge-design.md`)後にそのまま実装へ進んでしまった。実装・実機検証は完了済みのため、本ドキュメントは実際に行った内容を記録する後付けの計画書として書く(各ステップの「先に失敗を確認する」等のTDD的記述は、実際にはこの順で確認していない箇所を含む)。

**Goal:** Claudeがバックグラウンドジョブとして動く際にディスプレイを持たないため見られなかった、tsumugiのフロント側`console.log`/DOM状態を、ユーザーが実行中の実インスタンス(実アカウント・実データ)に対してJSを実行して確認できるようにする（Issue #232）。

**Architecture:** デバッグビルド限定(`#[cfg(all(debug_assertions, desktop))]`)で`src-tauri/src/debug_bridge.rs`がUnix系では`app_cache_dir()/debug-bridge.sock`にUnixドメインソケット、Windowsでは`\\.\pipe\tsumugi-debug-bridge-<USERNAME>`に名前付きパイプのリスナーを立てる。リクエストはHTTPの最小サブセット(`Content-Length`のみ解釈、メソッド/パス無視)として受け、ボディの生JS文字列を関数本体として`WebviewWindow::eval_with_callback`で評価し、結果を`{"ok":true,"value":...}` / `{"ok":false,"error":"..."}`のJSONで返す。TCP(127.0.0.1)ではなくUnixソケット/名前付きパイプを採用したのはlocalhost drive-by/DNS rebinding系の攻撃ベクトルを原理的に排除するため(設計ドキュメント参照)。共有ロジック(`handle_connection`/`read_http_body`)は`AsyncRead + AsyncWrite`にジェネリックにして両OSのストリーム型で使い回す。

**Tech Stack:** Rust (Tauri v2 backend, `src-tauri/`)、tokio(Unixドメインソケット/Windows名前付きパイプ+非同期I/O)。フロントエンド変更なし。

## Global Constraints

- デバッグビルド限定。リリースビルドではモジュールごとコンパイルされない(`cfg`ガード)。
- モバイルは対象外(`desktop`の`cfg`で除外)。
- Unix系: ソケットファイルはパーミッション`0600`。前回異常終了時の残骸ソケットファイルは起動時に削除してからbindする。
- Windows: 名前付きパイプ名にユーザー名を含め、同一マシンの他ユーザーとの衝突を避ける。`ServerOptions`の`reject_remote_clients`はデフォルトで有効なためネットワーク越しの到達は元々できない。実機(Windows)での動作確認はできておらず、`cargo check --target x86_64-pc-windows-gnu`によるクロスコンパイル確認のみ(既知の制約として設計ドキュメントに明記)。
- リクエストボディは関数本体として実行される(WebDriverの`execute_script`と同様、値を受け取るには明示的な`return`が必要)。式1つだけでは自動で値は返らない。
- 実行したJS文字列は`log::info!`で必ずログする(実データに対する任意JS実行の監査性のため)。

---

## Task 1: Unixソケットブリッジの実装

**Files:**
- Add: `src-tauri/src/debug_bridge.rs`
- Modify: `src-tauri/src/lib.rs`(モジュール宣言、`setup()`内での起動)
- Modify: `src-tauri/Cargo.toml`(`tokio`に`net`フィーチャ追加)

**Interfaces:**
- Consumes: `tauri::AppHandle`(`setup()`から渡される)、`WebviewWindow::eval_with_callback`(Tauri 2.11で確認済み)
- Produces: `pub fn socket_path(app: &AppHandle) -> PathBuf`、`pub fn spawn(app: AppHandle)`(バックグラウンドタスクとしてリスナーを起動)

- [x] **Step 1: `debug_bridge.rs`を実装**

以下を実装した(実ファイルは`src-tauri/src/debug_bridge.rs`参照):
- `socket_path`: `app_cache_dir()/debug-bridge.sock`を返す
- `spawn`: `tauri::async_runtime::spawn`でリスナーを起動。失敗してもアプリ本体は落とさずログのみ
- `listen`: 残骸ソケット削除→`UnixListener::bind`→パーミッション`0600`設定→accept loop
- `handle_connection`: `read_http_body`でボディ抽出→`eval_js`で評価→HTTPレスポンスとして返す
- `read_http_body`: ヘッダー終端(`\r\n\r\n`)まで読み、`Content-Length`のみ解釈してボディを読む(ヘッダー8KB/ボディ10MBの上限あり)
- `eval_js`: JSをtry/catchで包んだIIFEでラップし(`eval_with_callback`はWindows側の制約で例外をコールバックに渡さない仕様のため)、`tokio::sync::oneshot`でコールバック結果を非同期待ち受けする

- [x] **Step 2: 単体テストを書く**

`src-tauri/src/debug_bridge.rs`末尾の`#[cfg(test)] mod tests`に以下を追加:
- `finds_double_crlf`: ヘッダー終端検出のオフセット確認
- `http_response_has_correct_content_length`: マルチバイト文字を含む場合に`Content-Length`がバイト長(文字数ではない)になることの確認

Run: `cd src-tauri && cargo test debug_bridge`
Result: PASS(2テスト)

- [x] **Step 3: `lib.rs`に組み込む**

`mod debug_bridge;`を`#[cfg(all(debug_assertions, desktop, unix))]`付きで宣言し、`setup()`内(IME preedit設定の直後)で`debug_bridge::spawn(app.handle().clone());`を同cfgガード付きで呼ぶ。

- [x] **Step 4: `Cargo.toml`に`tokio`の`net`フィーチャを追加**

`tokio = { version = "1.52.3", features = [..., "net"] }`。`UnixListener`/`UnixStream`はtokioの`cfg_net_unix!`マクロでゲートされておりデフォルトフィーチャに含まれないため。

- [x] **Step 5: ビルド確認**

Run: `cargo build`
Result: 成功(既存の無関係な警告1件のみ)

- [x] **Step 6: 全Rustテスト実行**

Run: `cd src-tauri && cargo test`
Result: PASS(209 passed, 0 failed, 2 ignored。TSバインディング再生成テスト含む)

---

## Task 1.5: Windows名前付きパイプ対応を追加(ユーザー指摘を受けて後追い)

**背景:** 実装完了後、ユーザーから「Windowsはどうなるのか」と指摘があり、当初は`unix`限定のスコープ外としていた。tokio 1.52.3に`tokio::net::windows::named_pipe`が実在すること(`cfg_net_windows!`マクロで`feature = "net"`のみ要求、追加フィーチャ不要)をソースで確認した上で、Unixドメインソケットと同じ設計(ローカル専用・ファイルシステム上の名前でアクセス制御)がWindowsでも今すぐ実現できると判断し追加した。

**Files:**
- Modify: `src-tauri/src/debug_bridge.rs`(`handle_connection`/`read_http_body`を`AsyncRead + AsyncWrite`にジェネリック化し、`unix`/`windows_pipe`の2モジュールに分離)
- Modify: `src-tauri/src/lib.rs`(モジュールcfgゲートから`unix`条件を除去)

- [x] **Step 1: `tokio::net::windows::named_pipe`の実在とフィーチャ要件をソースで確認**

Run: `grep -n "named_pipe\|cfg_net_windows" ~/.cargo/registry/.../tokio-1.52.3/src/net/mod.rs`、`src/net/windows/named_pipe.rs`
Result: 存在確認。`cfg_net_windows!`は`cfg(all(windows, feature = "net"))`のみ要求(既に追加済みの`net`フィーチャで足りる、追加フィーチャ不要)

- [x] **Step 2: `handle_connection`/`read_http_body`をストリーム型に対してジェネリックにリファクタ**

`UnixStream`決め打ちだった`handle_connection(app: &AppHandle, stream: UnixStream)`と`read_http_body(stream: &mut UnixStream)`を`<S: AsyncRead + AsyncWrite + Unpin>`/`<S: AsyncRead + Unpin>`にジェネリック化。`http_response`/`eval_js`/`find_double_crlf`はストリーム型に依存しないためそのまま。

- [x] **Step 3: `unix`モジュールを既存のUnixドメインソケット実装として切り出す**

`socket_path`/`listen`(bind→パーミッション0600→accept loop)を`#[cfg(unix)] mod unix { ... }`にまとめる。

- [x] **Step 4: `windows_pipe`モジュールを新規実装**

tokioの`named_pipe`ドキュメント推奨パターン(`first_pipe_instance(true)`で最初のサーバーを作り、`connect().await`後に次のサーバーインスタンスを先に用意してから接続済みの方をタスクへ渡す)に従い実装。パイプ名は`\\.\pipe\tsumugi-debug-bridge-<USERNAME>`(同一マシンの他ユーザーとの衝突回避、Unix側の`app_cache_dir`がユーザー専用なのと同じ意図)。`ServerOptions`の`reject_remote_clients`はデフォルト有効のためそのまま使う(明示設定不要)。

- [x] **Step 5: `lib.rs`のモジュールcfgゲートを`#[cfg(all(debug_assertions, desktop, unix))]`から`#[cfg(all(debug_assertions, desktop))]`に変更**

- [x] **Step 6: Linuxでのビルド・テスト・実機動作が壊れていないことを再確認**

Run: `cargo build && cargo test`
Result: 209 passed(リファクタ前と同数)

Run: `cargo tauri dev`→ `curl --unix-socket ... --data-binary 'return 21 * 2;'`
Result: `{"ok":true,"value":42}`(リファクタ前と同じ挙動)

- [x] **Step 7: Windowsターゲットへのクロスコンパイルチェック**

Run: `cargo check --target x86_64-pc-windows-gnu --lib`(`rustup target add x86_64-pc-windows-gnu`済み環境)
Result: 成功(既存の無関係な警告1件のみ)。ただし実機(Windows)での`named_pipe`動作確認はできておらず、コンパイルが通ることの確認のみ(設計ドキュメントに既知の制約として明記)

---

## Task 2: 実機での動作確認

**Files:** なし(確認のみ)

**Interfaces:** なし

- [x] **Step 1: `cargo tauri dev`を起動し、ソケットファイルが生成されることを確認**

Run: `cargo tauri dev`(リポジトリルートから)→ `ls -la ~/.cache/com.onodai.tsumugi/debug-bridge.sock`
Result: `srw-------`(0600)で生成されていることを確認

- [x] **Step 2: `curl --unix-socket`でJS評価を確認**

以下を実行:
- `--data-binary '1 + 1'` → `{"ok":true}` (明示`return`が無いのでvalueなし、想定通り)
- `--data-binary 'return 1 + 1;'` → `{"ok":true,"value":2}`
- `--data-binary 'return document.title;'` → `{"ok":true,"value":"frontend"}`
- `--data-binary 'let x = document.querySelectorAll(".note-card").length; return x;'` → `{"ok":true,"value":0}`(複数文が実行できることを確認)

- [x] **Step 3: 例外時の挙動を確認し、`String(e)`にメッセージが含まれないJSC特有の挙動を修正**

最初の実装は`String((e && e.stack) || e)`のみで、WebKitGTKの`error.stack`はV8と違いメッセージを含まずスタックフレームのみ返すことが判明(実機で確認)。`String(e) + '\n' + e.stack`に修正し、`return 1 + 1;`等と同様に再ビルド→再確認。

Run: `--data-binary 'throw new Error("test error");'`
Result(修正前): `{"ok":false,"error":"@http://127.0.0.1:5173/:1:87\n..."}`(メッセージなし)
Result(修正後): `{"ok":false,"error":"Error: test error\n@http://127.0.0.1:5173/:1:87\n..."}`

- [x] **Step 4: 不正なリクエストでクラッシュしないことを確認**

Pythonで生ソケットに`"garbage not http"`を送りEOFなしで待たせた場合、`400 Bad Request`が返りコネクションが正常にクローズされることを確認(パニックしない)。

- [x] **Step 5: 起動した`cargo tauri dev`関連プロセスを終了**

`ps aux`で`cargo-tauri`/`vite`/`target/debug/tsumugi`の実PIDを特定し、`kill <pid>`で個別に終了(`pkill`/`killall`は使わない)。

---

## Task 3: 仕上げとPR作成

**Files:** なし(確認のみ)

**Interfaces:** なし

- [ ] **Step 1: `git status`で意図しない変更が残っていないか確認**

Run: `git status --short`
Expected: `docs/superpowers/specs/2026-08-22-claude-devtools-bridge-design.md`、`docs/superpowers/plans/2026-08-22-claude-devtools-bridge.md`、`src-tauri/Cargo.toml`、`src-tauri/src/lib.rs`、`src-tauri/src/debug_bridge.rs`以外に差分が無いこと

- [ ] **Step 2: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/superpowers/plans/2026-08-22-claude-devtools-bridge.md src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/debug_bridge.rs
git commit -m "feat: デバッグビルド限定のdevtools用Unixソケットブリッジを追加"
```

- [ ] **Step 3: PR作成**

```bash
git push -u origin feature/issue-232-devtools-bridge
gh pr create --title "feat: デバッグビルド限定のdevtools用Unixソケットブリッジ(Issue #232)" --body "$(cat <<'EOF'
## 概要
Claudeがバックグラウンドジョブとして動く際にディスプレイを持たず、フロントのdevtools/console.logを直接確認する手段が無かった問題への対応。デバッグビルド限定でUnixドメインソケットのブリッジを追加し、`curl --unix-socket`経由でユーザーが実行中の実インスタンスに対してJSを実行し結果を取得できるようにした。

TCP(127.0.0.1)ではなくUnixドメインソケットを採用しているのは、ブラウザの`fetch()`から到達できてしまう(localhost drive-by/DNS rebinding系)攻撃ベクトルを原理的に排除するため。詳細は`docs/superpowers/specs/2026-08-22-claude-devtools-bridge-design.md`参照。

Fixes #232

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URLが出力される。マージは行わず、ここで作業完了とする(CLAUDE.mdの方針通り、CI結果はユーザーが確認する)。
