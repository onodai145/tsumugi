# Issue #12: 通知音のRust側ネイティブ再生への移行 設計

## 背景・根本原因

Issue #12「謎のタイミングで通知音が鳴る」の調査により、以下が判明した。

- 通知音の再生はフロントエンド（`frontend/src/lib/store.svelte.ts`）が`AudioContext`（Web Audio API）で完結して行っている
  - プリセット音（beep/chime/ping/pop）は`playTone()`/`playPreset()`でオシレーターをその場で合成
  - カスタム音声は`new Audio(dataUrl).play()`
- `AudioContext`はSpec上生成直後は`suspended`状態で始まり、ユーザー操作やブラウザ/webviewの自動再生ポリシーが働かない限り鳴らない。Tauriのwebview（WebKitGTK等）はウィンドウのフォーカス喪失・バックグラウンド化などで再度`suspended`に落ちる可能性がある
- リポジトリ全体を検索しても`resume()`を呼んでいる箇所が無く、一度`suspended`になった`AudioContext`を明示的に復帰させる処理が存在しない
- これにより、通知イベント自体は正しいタイミングで発火・ログも残る（`#logDebug`）が、実際の音の再生だけが`suspended`状態のまま滞留し、何らかの理由（フォーカス復帰等）で復帰した瞬間に大幅に遅れて（または溜まった分がまとめて）鳴る、という現象が起きていたと推測される
- 「謎の通知音が鳴ったあとは試聴もできなくなり、再起動すると直る」という報告とも整合する（試聴ボタンも同じ壊れた`audioCtx`シングルトンを使い回すため、再起動でモジュールがリロードされるまで復帰しない）

このクラスの不具合は「webview内でWeb Audio APIを使って音を鳴らしている」という設計そのものに起因するため、対症療法（`resume()`呼び出しの追加等）ではなく、**音を実際に鳴らす部分をRust側のネイティブオーディオ再生に移す**ことで解決する。

## 関連: Issue #75（着メロ打ち込み機能）との両立

Issue #75はTone.js（Web Audio APIベースのJSライブラリ）を使った着メロ打ち込み機能の構想。これとの衝突を避けるため、**「音の合成/打ち込み」と「音を鳴らすトリガー」を分離**する：

- 合成・打ち込み（プリセットのオフラインレンダリング、将来の着メロ機能）は引き続きフロントエンド／JS側の責務
- 「今この音を鳴らせ」という実行トリガーだけをRust側に寄せる

現行の`NotifyConfig.sound_choice`（`src-tauri/src/domain/notify.rs`）は既に「プリセットID」または「`data:`URL（カスタム音声）」の2値を取る設計になっている。将来Tone.jsで打ち込んだ着メロも「オフラインレンダリングして`data:`URL化」すれば既存の「カスタム音声」経路にそのまま乗るため、**今回の移行はこのプロトコルを変更せずに実現できる**。

## アーキテクチャ

**変更前:** 通知イベント受信 → フロントエンドJSが`AudioContext`で音を合成/再生 → webviewの自動再生ポリシーの影響で再生タイミングが不定になる。

**変更後:**
- 通知イベント受信・「鳴らすかどうか」の判定ロジック（`wantsSound`判定、タブ別設定の解決、dedup=`#markNotified`など）は画面依存のON/OFFロジックのため**フロントエンドに残す**
- 「実際に音を鳴らす」実行部分のみ新しいTauriコマンド`play_notify_sound(choice: String)`としてRust側に切り出す。JS側の`AudioContext`/`playTone`/`playPreset`/`new Audio()`実装は丸ごと削除する
- 設定画面の「試聴」ボタンも同じコマンドを呼ぶ（プレビューと実通知で実装が分岐しない一本道にする）
- 対象は**デスクトップ＋Android全プラットフォーム**。「webviewでWeb Audio APIを使う」という同じ仕組みに起因する問題である以上、OSを問わず起こりうるため、Android含め全プラットフォームをスコープに含める（ただしAndroid実機での動作検証は自動化が難しく、手動検証項目とする）

`choice`文字列の解釈はRust側が持つ：
- プリセットID（`"beep"`/`"chime"`/`"ping"`/`"pop"`/`""`） → `include_bytes!`で埋め込み済みのWAVを再生
- `data:`で始まる文字列 → base64部分をデコードしてバイト列を再生

## コンポーネント・データフロー

**Rust側（新規）:**
- `src-tauri/assets/sounds/{beep,chime,ping,pop}.wav` — プリセット音源。開発時に一度だけレンダリングして同梱する（現行のオシレーター合成パラメータをそのまま録るか、Tone.jsでレンダリングするかは実装時の任意選択。実行時合成はしない）
- `src-tauri/src/commands/sound.rs`（新規モジュール）
  - `#[tauri::command] play_notify_sound(choice: String) -> Result<()>` を追加し、`specta_builder()`にも登録する
  - 内部を2段に分離する：
    1. `resolve_audio_bytes(choice: &str) -> Result<Cow<'static, [u8]>>` — プリセットID／`data:`URLをバイト列に解決する**純粋関数**（副作用なし、単体テスト可能）
    2. 実際にデバイスへ出力する部分（`rodio`経由）。多重再生を許容するため、呼び出しごとに`Sink`を新規作成し`.detach()`して非同期に鳴らしっぱなしにする
- `Cargo.toml`に`rodio`を追加する（`cpal`を直接使う理由（低レイテンシ最適化等）は無いため、高レベルAPIの`rodio`を採用）

**フロントエンド側（変更）:**
- `frontend/src/lib/store.svelte.ts`から`playTone`/`playPreset`/`playNotifySound`/`audioCtx`（Web Audio API関連一式）を削除する
- 呼び出し箇所（`columnNote`/`columnNotification`イベントハンドラの計2箇所）を、`playNotifySound(choice)` → `void unwrap(commands.playNotifySound(choice))`（fire-and-forget）に置き換える
- `NotifySection.svelte`の「試聴」ボタンも同様に置き換える

## エラーハンドリング・ロギング

- 出力デバイスが無い／取得失敗、`data:`のbase64デコード失敗、音声フォーマットのデコード失敗などは、JS版と同じく「音の失敗で通知全体を止めない」方針を踏襲する。`play_notify_sound`はエラーでも`Err`を返さず内部で吸収し、呼び出し側（フロントエンド）は結果を無視できる
- 今回のIssue #12は「ログはあるのに実際に鳴った形跡が追えない」ことが調査を難しくした原因だったため、Rust側では失敗時に既存のログ基盤へwarnログを残す。「鳴らそうとした→失敗した」がBackstageログとは別に（Rust側の）ログファイルで追えるようにし、今後似た問題が起きたときの切り分けを容易にする

## テスト方針

- `resolve_audio_bytes()`はRust単体テストで、プリセットID4種＋空文字（デフォルトbeep扱い）＋`data:`URL＋不正な文字列（デコード失敗）のケースをカバーする（CIのヘッドレス環境でも音声デバイス無しで検証可能）
- 実機での「実際に音が鳴るか」「webviewバックグラウンド時も遅延しないか」は自動テスト化が難しいため、手動検証項目として明記する（Linuxデスクトップで最低限確認。Android/macOS/Windowsは可能な範囲で）
- 既存のフロントエンドテスト（`store.svelte.test.ts`内のsound関連テスト）は、Web Audio API直呼びのテストからTauriコマンド呼び出しのモック検証に置き換える

## スコープ外

- Issue #75（着メロ打ち込み機能）自体の実装は本設計のスコープ外。プロトコル（`sound_choice`の`data:`URL経路）が将来そのまま使えることのみ確認済み
- `AudioContext`の`resume()`呼び出しによる対症療法は採用しない（根本解決のRust移行を選択したため）
