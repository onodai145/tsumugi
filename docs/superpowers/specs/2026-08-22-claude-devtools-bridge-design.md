# Claudeがフロントのdevtoolsを見られるようにするデバッグブリッジ（Issue #232）

## 背景

Claudeがバックグラウンドジョブとして動く際にディスプレイを持たないため、tsumugi(Tauri v2 + WebKitGTK)のフロント開発者コンソール(F12 DevTools)を直接見る手段が今は無い。Rust側の標準出力は自分で`cargo tauri dev`を起動すれば見られるが、フロントの`console.log`やDOM調査はユーザーに手動でコピペしてもらう以外の方法が今は無い。

Issue #166(コードブロックのスクロールバードラッグ不可)の調査で、この往復が数十回発生し非効率だった実例がある。

## 検討して却下した案

### `WEBKIT_INSPECTOR_SERVER`直接接続

WebKitGTKのリモートインスペクタは単純なHTTP/WebSocketではなく、GLibの生ソケット + GVariant形式のバイナリ独自プロトコル(`Source/JavaScriptCore/inspector/remote/glib/RemoteInspectorServer.cpp`)を使っている。`curl`等では喋れず、ハンドシェイク(`SetupInspectorClient`→`SetTargetList`受信→対象選択→Inspector Protocol JSON)を含めて自前実装するのは実装コストに見合わない。

### tauri-driver(WebDriver)経由

このリポジトリのe2eテストで既に使われている`tauri-driver` + `WebKitWebDriver`は標準WebDriverプロトコル(JSON over HTTP)で`execute_script`相当が使え、当初は有力に見えた。しかしWebDriverは**セッション開始時にアプリの起動そのものを担う**ため、「ユーザーが今動かしている実インスタンス」には後から接続できない。かといって隔離設定で新規起動すると、実データ(アカウント・実際のノート)が無い空のアプリになってしまい、GUIデバッグの本来の目的(実データでの再現確認)を果たせない。実データがある構成で並行起動すると、SQLite/keyringのロック競合リスクもある。

## 採用: アプリ内蔵のデバッグ用ローカルソケットブリッジ

このリポジトリには既に前例がある: `open_devtools`コマンド(デバッグビルド限定、リリースビルドではno-op)。同じパターンをもう一歩進める。

- デバッグビルド限定で、`lib.rs`の`run()`内でUnix系ではUnixドメインソケット、Windowsでは名前付きパイプのリスナーを立てる
- リクエストボディのJS文字列を受け取り、`WebviewWindow::eval_with_callback(js, callback)`(Tauri 2.11で確認済み、JSON化された評価結果をコールバックで受け取れる)で実行
- コールバックの結果をレスポンスとしてそのまま返す

これにより、ユーザーが既に動かしている実インスタンス(実アカウント・実データ)に対して、Claudeが`curl --unix-socket <path> ...`のような形でJSを実行し結果を直接受け取れるようになる。新規プロセス起動・GPU/Xvfb不要・既存のWebDriver/インスペクタ関連の複雑さを一切回避できる。

### TCP(127.0.0.1:PORT)ではなくUnixドメインソケット/名前付きパイプを採用する理由

当初案は`127.0.0.1`バインドのTCPリスナーだったが、セキュリティレビューの結果却下した。TCPだと、ユーザーが悪意あるWebページを開いている間に、そのページの`fetch()`からCORSプリフライト不要な形(`text/plain`のPOST等)で任意JSを送りつけられるリスクがある(レスポンスは読めなくてもリクエストは実行されてしまう、いわゆるlocalhost drive-by/DNS rebinding系の攻撃パターン)。tsumugiは実アカウント・実データを扱うプロセスのため、ここは看過できない。

Unixドメインソケット/Windows名前付きパイプにすればブラウザから原理的に接続不可能なため、この攻撃ベクトルごと排除できる。Unix系では`curl --unix-socket`で叩けるため運用性は変わらない。ソケットファイルのパーミッション(0600)も付与する。名前付きパイプ側は`tokio::net::windows::named_pipe`の`reject_remote_clients`がデフォルトで有効なため、ネットワーク越しの到達はそもそもできない。

### ソケットパス/パイプ名

Unix系は固定パス(`app_cache_dir()/debug-bridge.sock`、アプリ専用でユーザー専用の場所)に統一する。Windowsは`\\.\pipe\tsumugi-debug-bridge-<USERNAME>`とし、同一マシンの他ユーザーとの衝突を避ける(Unix側のapp_cache_dirがユーザー専用なのと同じ意図)。両OSとも起動時ログにも場所を明示する。

### 有効化条件

デバッグビルドであれば常時有効とする(追加の環境変数によるopt-inは設けない)。

### 監査性

実行したJS文字列は標準出力にログする。実データに対して任意JS実行の口を開けるため、ユーザー自身が後から「何を実行されたか」追えるようにしておく。

## 検証すべき点(実装時)

- `eval_with_callback`の正確な戻り値仕様(エラー時の扱い含む)
- ソケットファイルのパーミッションが確実に他ユーザーから到達不可であることの確認
- Windows名前付きパイプ側は実機(Windows)での動作確認ができておらず、`cargo check --target x86_64-pc-windows-gnu`によるクロスコンパイル確認のみで済ませている。次にWindows環境で触る機会があれば実地検証すること

## 優先度

今すぐの対応は不要。将来的な開発体験改善として起票(Issue #232)。実装は別セッションで行う。
