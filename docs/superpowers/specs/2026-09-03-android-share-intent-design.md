# Android共有インテント受信 設計 (Issue #116)

## 概要

Android の共有シート(他アプリの「共有」メニュー)に tsumugi を候補として表示し、テキスト・画像・動画を受け取ってコンポーズ画面に流し込む。対象は Android のみ(iOS はまだビルド対象外)。

## 対応範囲

- `ACTION_SEND` / `ACTION_SEND_MULTIPLE`
- MIME: `text/plain`, `image/*`, `video/*`
- アプリ実行中(バックグラウンド含む)は既存ウィンドウにフォーカスしてコンポーズを開く(`singleTask` のため新規 Activity は作られない)
- コールドスタート(未起動から共有経由で起動)にも対応

## 参考にした先例

Tauri 公式のモバイルプラグイン開発ガイド([Mobile Plugin Development | Tauri](https://v2.tauri.app/develop/plugins/develop-mobile/))は `onNewIntent` をオーバーライドして共有を検出する基本パターンを示しているが、そこで使われる `@TauriPlugin` は**独立したプラグインクレート**(`cargo tauri plugin new` でスキャフォールドする、`android/` gradleモジュール・`build.rs`のPluginBuilder・自動生成ACLファイル・`guest-js/`一式を伴う構成)を前提にしており、アプリ本体クレートに直接クラスを足して済ませる方法はドキュメント上に存在しない。

同種のコミュニティプラグイン([tauri-plugin-sharetarget](https://lib.rs/crates/tauri-plugin-sharetarget), [tauri-plugin-mobile-sharetarget](https://github.com/IT-ess/tauri-plugin-mobile-sharetarget))も存在するが、いずれも小規模(Star 一桁〜十数程度)でエコシステムとして未成熟なため依存追加はせず自前実装する。

フルのプラグインクレートを新設するのは今回の用途(自分のアプリ内で完結する一機能)には過剰なため、**JNI直結**方式を採る。これは tsumugi が既に採用しているパターンで、`android-native-keyring-store` クレートが `Keyring.kt` に `external fun initializeNdkContext(context: Context)` を宣言し、Rust側の JNI export を直接呼んでいる(`src-tauri/gen/android/app/src/main/java/io/crates/keyring/Keyring.kt`)。JNI関数は生成コード `Rust.kt` が `System.loadLibrary("tsumugi_lib")` で読み込む同じ `.so` に静的リンクされるため、追加のロード処理は不要。プラグインクレート一式(gradleモジュール・ACL自動生成・`guest-js/`)を新設せずに済み、tsumugi の「REST クライアントや Streaming も外部依存に頼らず手書きする」という方針とも合致する。

## コールドスタート/ウォームスタートの統一

当初案では実行中の共有を `trigger()` によるプラグインイベントの即時プッシュで拾う想定だったが、JNI直結ではプラグインイベント機構(`addPluginListener`)自体が使えない。代わりに、**単一のプル方式**に統一する:

- Android で共有シートから tsumugi が選ばれると、`singleTask` により既存タスクが前面に呼び戻される(新規 Activity は作られない)。これは通常のアプリ復帰(タスクスイッチで前面に戻す)と同じ扱いであり、WebView の可視化に伴い `document.visibilitychange` が `visible` になる。
- フロントは起動時(`onMount`)と `visibilitychange` で `visible` になるたびに `get_pending_share` コマンドを呼ぶ。共有が無ければ `None` が返るだけの安価な呼び出しなので、通常のアプリ復帰のたびに呼んでも問題ない。
- これにより、コールドスタートとウォームスタートを同じ1コマンドの使い回しでカバーできる。プラグインイベント/ACL/`addPluginListener` は一切不要になる。

## アーキテクチャ

```
他アプリの共有 → ACTION_SEND(_MULTIPLE)
      ↓
MainActivity.kt (onCreate の起動時intent / onNewIntent の両方から共通処理を呼ぶ)
  - text/plain → EXTRA_TEXT をそのまま文字列化
  - image/* or video/* → EXTRA_STREAM (単数)/ ArrayList<Uri> (複数, ACTION_SEND_MULTIPLE) を
    ContentResolver 経由で cacheDir/shared-intents/ に元ファイル名で一時コピー
  - external fun nativeShareReceived(text: String?, filePaths: Array<String>) を呼ぶ
      ↓ (JNI, libtsumugi_lib.so 内)
Rust: #[no_mangle] extern "system" fn Java_com_onodai_tsumugi_MainActivity_nativeShareReceived
  - 受け取った値を ShareReceived にまとめ、プロセスグローバルな
    Mutex<Option<ShareReceived>> (mobile_intent モジュール) に格納する
      ↓
commands::app::get_pending_share() -> Option<ShareReceived> (specta command)
  - Android: 上記グローバルから take() して返す(一度きりの消費)
  - 非Android: 常に None (no-op)
      ↓
Frontend: App.svelte
  - onMount で1回、以後 document.visibilitychange で visible になるたびに呼ぶ
  - 結果があれば showComposeModal を開き、ComposeBar に text/attachments 初期値を渡す
```

## Rust側

- `domain/` に `ShareReceived { text: Option<String>, file_paths: Vec<String> }` (`specta::Type` 付き) を追加
- 新規 `src-tauri/src/mobile_intent.rs`:
  - `#[cfg(target_os = "android")]` ブロック: `static PENDING_SHARE: Mutex<Option<ShareReceived>>` と、JNI export 関数、`pub fn take_pending_share() -> Option<ShareReceived>`
  - それ以外の OS: `pub fn take_pending_share() -> Option<ShareReceived> { None }` のみ
- `Cargo.toml` の `[target.'cfg(target_os = "android")'.dependencies]` に `jni = "0.21"` を追加
- `commands/app.rs` に `get_pending_share() -> Option<ShareReceived>` を追加し `specta_builder()` の `collect_commands!` に登録(既存の `core:default` で権限は足りるため、Capabilities への追加は不要)

## Kotlin側(MainActivity.kt)

- `external fun nativeShareReceived(text: String?, filePaths: Array<String>)` を宣言
- `onCreate(savedInstanceState)` の既存処理の後に、起動時の `intent` を渡して共通処理関数を呼ぶ
- `override fun onNewIntent(intent: Intent)` を追加し、`super.onNewIntent(intent)` を呼んだ上で同じ共通処理関数を呼ぶ(`super`呼び出しは `WryActivity` 側の既存 `Rust.onNewIntent` 転送を壊さないために必須)
- 共通処理関数 `handleShareIntent(intent: Intent)`:
  - `action == Intent.ACTION_SEND && type == "text/plain"` → `intent.getStringExtra(Intent.EXTRA_TEXT)`
  - `type` が `image/` または `video/` で始まる場合:
    - 単数(`ACTION_SEND`): API 33+ は `intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)`、それ未満は `@Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri` にフォールバック
    - 複数(`ACTION_SEND_MULTIPLE`): 同様に `getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)` / 非対応OSは `@Suppress("DEPRECATION")` 版にフォールバック
    - 各 `Uri` について `contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), ...)` で元ファイル名を取得(取得できない場合のみ `content-${System.currentTimeMillis()}` のような機械的な名前にフォールバック。拡張子の当て推量はしない — Misskeyドライブに誤ったファイル名で入るのを避けるため)
    - `contentResolver.openInputStream(uri)` から `File(File(cacheDir, "shared-intents").apply { mkdirs() }, displayName)` へコピー
    - コピー失敗時はその1件だけスキップ(例外を握りつぶしログのみ)
  - 抽出した `text`(nullable)と `filePaths`(`Array<String>`、空配列可)で `nativeShareReceived` を呼ぶ。両方とも空/nullなら呼ばない
- `onCreate` の先頭付近で `File(cacheDir, "shared-intents").deleteRecursively()` を実行し、前回分の残骸を掃除する

## フロントエンド

- `App.svelte`:
  - `onMount` で `pollPendingShare()` を1回呼ぶ
  - `document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") pollPendingShare(); })` を登録
  - `pollPendingShare()`: `commands.getPendingShare()` を呼び、`text` か `filePaths.length > 0` のいずれかがあれば `app.showComposeModal = true` にし、保留データを渡す
- `ComposeBar.svelte`:
  - 保留データを受け取り、`text` へ反映、`filePaths` の各要素を既存の `{ kind: "local", path, name, previewUrl }` として `attachments` に追加(画像なら `readAttachmentPreview` でプレビュー生成。`pickFiles` の既存ロジックと共通化できる部分は関数として括り出す)
  - 送信時は通常の `local` 添付と同じフローで `upload_file` に乗る(追加のアップロード経路は不要)

## エラーハンドリング

- 未対応 MIME やパース失敗: 何もしない(エラーダイアログは出さず、共有元アプリのUXを壊さない)
- 一時ファイルコピー失敗: 該当ファイルのみスキップ、テキストや他ファイルは活かす

## テスト方針

- Rust: `mobile_intent::take_pending_share` の非Android(no-op)分岐、`ShareReceived` の serde/specta 型生成(既存の `generates_frontend_bindings` テストでカバー)、`get_pending_share` コマンドの一度きり消費(取得後は `None` になる)
- Kotlin の Intent パース自体は `cargo test` 対象外(手動確認に委ねる)
- フロントエンド(Vitest): `App.svelte` の起動時/`visibilitychange` での `getPendingShare` 呼び出し→`showComposeModal` 遷移、`ComposeBar` への添付反映ロジック
- 実機/エミュレータでの手動確認: ブラウザ/ギャラリー等から「共有」→ tsumugi 選択→ コンポーズにテキスト/画像が入ることを目視確認(コールドスタート・ウォームスタート双方)
