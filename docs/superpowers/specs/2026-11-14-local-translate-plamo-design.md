# ローカル翻訳（PLaMo 3 Translate）設計

Issue: #603

## 背景 / 目的

ノート翻訳は現在、接続先 Misskey サーバーの `notes/translate` だけで行っている（Issue #440、
`2026-10-02-note-translate-design.md`）。サーバーに翻訳機能（DeepL / LibreTranslate）が設定されていなければ使えず、
本文はサーバー経由で外部の翻訳サービスへ渡る。

PFN の翻訳特化 LLM「PLaMo 3 Translate」（0.8B）を端末内で動かし、サーバーに翻訳機能がなくても、本文を外へ出さずに
翻訳できるようにする。対象はデスクトップ（Linux / macOS / Windows）と Android。

## 合意済みの前提（決定事項）

- **エンジンの関係**: 設定で選べる。既定は従来どおりサーバー翻訳で、ローカル翻訳は任意で有効にする。
- **対象**: デスクトップと Android を同じ spec で扱う。実装は段階を分ける（後述）。
- **モデル**: `pfnet/plamo-3-translate-0.8b-GGUF` の `Q4_K_M`（573,126,848 バイト）の1種類に固定する。
  アプリ内で Hugging Face からダウンロードする（重みは同梱しない）。
- **推論方式**: `llama-cpp-2`（llama.cpp）をアプリのプロセス内に直結する。
- **UI 表記**: メニュー項目も結果欄もエンジン別。サーバーは「翻訳」「{言語} から翻訳」、ローカルは「PLaMo で翻訳」。
- **通信種別**: Android ではダウンロード前に回線の種類と従量制かを判定して警告する。
- 翻訳結果のキャッシュはしない（#440 と同じ）。CW 文・投票選択肢の翻訳はしない。

## 調査結果（実現可能性 spike、Issue #603 のコメントに詳細）

2026-11-14 に、リポジトリ外の使い捨てプロジェクトで確認した。

| 項目 | 結果 |
|---|---|
| ランタイム | `llama-cpp-2` の crates.io 0.1.161 は、同梱 llama.cpp に `plamo3` トークナイザがなく、公式 GGUF（`tokenizer.ggml.model = plamo3`）を読めない見込み（同梱ソースの確認による推論で、0.1.161 での実行は未確認）。`utilityai/llama-cpp-rs` の git（rev `955aa2f7`、crate 0.1.162、llama.cpp v0.9.0 同梱）では実際に読み込み・翻訳できた |
| デスクトップ CPU | 約 54 tok/s、ノート1件 0.15〜0.6 秒、ロード 約 1.2 秒 |
| デスクトップ GPU（Vulkan） | 約 200〜460 tok/s だが初回に約 3.5 秒のシェーダ初期化。CPU で十分 |
| Android 実機（Pixel 8 Pro、Tensor G3） | 基本命令（`armv8-a`）で約 13.8 tok/s、ノート1件 1〜3 秒 |
| 訳文 | 絵文字コード・メンション・URL・ハッシュタグ・MFM・改行が保たれた。日英・独→日・韓→日とも自然 |
| 注意 | `vocab().vocab_type()` が panic する（Rust 側 enum が `PLAMO3` を知らない）。スレッド数がコア数を超えると極端に遅くなる |
| ビルド | 素の `cargo build`（16並列）で OOM。並列度4・ディスク上の target・メモリ上限付きで約 2 分で完走 |

品質は 8 文の目視確認で、数値評価はしていない。

## 仕様

### 1. エンジン選択と `translate_note`

- `UiPrefs` に `translate_engine: String`（`"server"` | `"local"`）を追加する。`#[serde(default)]` で既定 `"server"`。
  `commands::note::effective_translate_engine()` で正規化し、未知の値は `"server"` に倒す。
  enum にしないのは、将来の値追加やダウングレードで `UiPrefs` 全体の読み込みが壊れないようにするため。
  `Default` 実装と既存テストの網羅リテラルも更新する。
- `translate_note(account_id, note_id, text)` に `text`（翻訳元の本文）を追加し、Rust 側でエンジンを振り分ける。
  - `server`: 従来どおり `notes/translate`（`text` は使わない）。
  - `local`: `text` を端末内のモデルで翻訳する。サーバーには何も送らない。
- `get_translator_available(account_id)`: `local` のときは「このビルド・プラットフォームでローカル翻訳に対応しているか」を返す
  （モデル未取得でも true。押したときに案内を出すほうが分かりやすい）。
- `Translation.source_lang` を `String` から `Option<String>` にする。PLaMo は翻訳元言語を内部で判別する
  （`source_lang` を省略すると判別する。モデルカードより）が、判別結果は返さないので、`local` では `None`。
  波及先は `domain/note.rs`、生成バインディング、`NoteCard.svelte` とそのテスト。

### 2. モデルの取得と保存（`translate/model_store.rs`）

- URL・リビジョン（コミット）・期待する SHA-256・サイズをコード上の定数にする。PFN 側の更新で重みが黙って変わらないよう
  リビジョンを固定し、更新はアプリのリリースで追従する。
- 保存先は `app_data_dir/models/`（`app_cache_dir` は消される可能性があり、`app_config_dir` は設定バックアップ対象のため）。
  書き込みは `.part` ファイルに行い、ハッシュ検証後にリネームする。途中のファイルは Range リクエストで再開する。
- 開始前に空き容量を確認する（約 0.6GB + 余裕）。
- ダウンロード用の専用 HTTP クライアントは `net_guard` のガード（私的アドレスの拒否）を適用する。接続先は固定 URL だけで、
  ユーザー入力の URL は取らない。起動時や設定を開いただけでは通信しない。
- 状態は `LocalModelStatus`（`NotInstalled` / `Downloading { downloaded, total }` / `Verifying` / `Ready` / `Failed { reason }`）。
- コマンド（すべて `specta_builder()` に登録）: `get_local_model_status`、`start_local_model_download(accepted_license_version)`、
  `cancel_local_model_download`、`delete_local_model`。
- 進捗は tauri-specta の Event（`LocalModelStatusChanged`、約 250ms 間隔に間引く）。同時に走るダウンロードは1つだけ。
  設定画面を閉じても続く。
- 読み込み時の検査はサイズだけ（ハッシュはダウンロード完了時だけ）。
- キャンセルと通信の中断では `.part` を残し、次回は途中から再開する（Range）。`.part` を捨てるのは、削除・ハッシュ不一致・サイズ超過のとき。
- `get_local_model_status` は `LocalModelInfo`（状態・サイズ・現行のライセンス版とその全文の URL・同意済みか・このビルドが対応しているか）を返す。
- `is_translate_lang_supported(lang)` を足し、翻訳先の言語が PLaMo の対応言語かの判定を Rust（`lang::to_plamo_lang`）に一本化する（フロントに言語表を複製しない）。

### 3. ライセンス同意

- ライセンスは PLaMo Community License（2026-11-09 改定版）。
  個人・非商用は無料。商用は事前登録と年商 10 億円以下が条件。モデルを第三者に提供する場合は、全文の写しの提供、
  "Built with PLaMo" の表示、派生モデル名への "PLaMo" の含有が必要（4条）。PFN が一方的に改定できる（12条）。
- 「ダウンロード」を押すと、ライセンスの要約（日本語）と全文へのリンクを出し、「同意してダウンロード」で開始する。
- 強制は Rust 側で行う。`start_local_model_download(accepted_license_version)` は、引数がアプリ内の定数
  （現行 `2026-11-09`）と一致しなければ `Error::Invalid` で拒否する。
- 保存済みの同意版が現行のライセンス版と同じなら、ダイアログを出さずにダウンロードを開始する（版が変わったときと未同意のときだけ出す）。同意版は、ダウンロードの開始に成功したあとに保存する（開始できなかったのに同意済みが残らないようにする）。
- 同意した版は `SettingsData` に新しいフィールドとして保存する。MCP 設定と同様に設定のエクスポートには含めず、
  インポート時は現在の値を維持する（同意は端末ごとの行為のため）。
- ライセンス版が将来の更新で変わったときは、次のダウンロード時にだけ再同意を求める。取得済みのモデルはそのまま使える。
- "Built with PLaMo" は、設定画面のモデルカードと、結果欄の見出し「PLaMo で翻訳」、ユーザーガイドに載せる。
  既存に About やライセンス表示の画面があれば、そこにも載せる（実装時に確認）。
- モデルは同梱しない。ユーザーの端末へ取得するのはユーザー自身の操作であり、アプリは固定の URL へ案内するだけ、
  という位置づけで進める（ただし後述「リリース前の確認事項」を参照）。
- **出力の取り扱い（5条）**: 翻訳結果は選択可能なプレーンテキストとして表示するだけにし、コピー・共有・投稿の専用導線は設けない
  （現行の `NoteCard.svelte` もそうなっている）。5条(2)により、変換モデルの出力は PLaMo を使った旨の表示なしで公開できるが、
  5条(4)（出力を第三者へ提供・公開するとき、その出力で学習したモデルにも本ライセンスが及ぶことを示す義務）は免除されない。
  翻訳結果をそのまま投稿・共有する機能を将来足す場合は、5条(4)の周知を設計に含める。

### 4. 推論（`src-tauri/src/translate/`）

構成:

| ファイル | 役割 |
|---|---|
| `prompt.rs` | PFML3 プロンプトの組み立てと停止判定（純粋関数） |
| `engine.rs` | `llama-cpp-2` を包む。`LocalTranslator` trait の背後に置き、テストでは偽実装に差し替える |
| `model_store.rs` | 上記 §2 |
| `manager.rs` | モデルの遅延ロード、アイドル時のアンロード、翻訳要求の直列化 |

- 推論は `spawn_blocking` で行い、翻訳ごとに新しい context を作る。`LlamaBackend` の初期化は一度だけ（`OnceLock`）。
- 翻訳要求は直列に処理し、同時に来たものは順番待ちにする。初回の翻訳時にモデルをロードする（約 1〜2 秒）。
- アイドルが続くとアンロードする。初期値はデスクトップ 10 分、Android 3 分（実測で見直す）。
- 推論の途中キャンセルは入れない。結果が不要なら、フロントが破棄する（既存の `translateSeq` と同じ）。
- パラメータ: greedy（temperature 0）、スレッド数 `min(4, available_parallelism)`、CPU のみ（GPU は使わない）、
  `n_ctx` 4096、入力が約半分（約 2000 トークン）を超えるなら `LOCAL_TEXT_TOO_LONG` で拒否する（初期値。Android のメモリ実測で見直す）。
- プロンプトは PFML3 の生文字列（BOS は llama.cpp が付ける）:
  `<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input<|plamo:msg|>{本文}<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>{翻訳先}<|plamo:msg|>`
- **プロンプトの乗っ取り対策**: 固定部分はスペシャルトークンとして解釈させ、**ユーザー本文はスペシャルトークンとして解釈させない**。
  本文に `<|plamo:tag|>` と書かれても構造を壊せないようにする。
  - 本文は固定部分と別々にトークン化して連結する。ただし、**PLaMo 3 のトークナイザは `parse_special = false` でも本文中の
    `<|plamo:tag|>` などをスペシャルトークンへ変換する**（実モデルで確認: `a<|plamo:tag|>b` が3トークンになり、
    `<|plamo:tag|>` は id 16）ので、別々にトークン化するだけでは防げない。
  - そこで、トークン化の前に、本文の `<|` の間へゼロ幅スペース（U+200B）を挟んで、スペシャルトークンの文字列に一致しないようにする
    （`prompt::sanitize_text`）。見た目は変わらない。`<|` を含まない本文は変えない。
  - 通常の文では、一括トークン化と同じ結果になることを、実モデルのテストで確認する。
- 停止: EOG、または出力に `<|plamo:` が現れたとき。最大生成トークン数は `n_ctx` から入力を引いた範囲。
- ノート本文と訳文はログに出さない。llama.cpp 自体のログも抑止する。
- 言語コード: 設定の言語コード（`ja` / `en` / `zh` / `ko` / `fr` / `de` / `es` / `pt` / `ru` など）を PLaMo のコードへ変換する表を持つ。
  PLaMo の対応は 53 言語（モデルカードの対応言語一覧で確認済み）。`zh` は `zh-hans`（`zh-tw` / `zh-hk` は `zh-hant`）へ、
  地域付きコード（`en-us` など）は地域を除いて照合する。**既存のプリセットのうち `pt`（ポルトガル語）だけが PLaMo の対応言語に
  含まれない**ので、`LOCAL_LANG_UNSUPPORTED` になる。非対応は同じく `LOCAL_LANG_UNSUPPORTED`。

### 5. エラー

専用の `Error` variant は足さず、#440 の `UNAVAILABLE` の前例どおり `Error::Invalid` の文言で区別する。フロントは文言で判別する。

| 文言 | 意味 | 表示 |
|---|---|---|
| `LOCAL_MODEL_NOT_READY` | モデル未取得、サイズの不一致、または読み込みに失敗 | 「PLaMo のモデルが未取得か、読み込めません。設定の「外部連携」の「翻訳エンジン」からモデルをダウンロードしてください」 |
| `LOCAL_TEXT_TOO_LONG` | 入力が長すぎる | 「ノートが長すぎるため翻訳できません」 |
| `LOCAL_LANG_UNSUPPORTED` | 翻訳先の言語に非対応 | 「翻訳先の言語に PLaMo は対応していません」 |
| `LOCAL_UNSUPPORTED` | このビルド・プラットフォームが非対応 | 「この環境では PLaMo による翻訳は使えません」 |

### 6. 通信種別の判定（Android）

- `MainActivity.kt` で `ConnectivityManager` の状態変化を監視し、回線の種類と従量制かを JNI で Rust に渡す
  （Kotlin から Rust への push。`mobile_intent.rs` の `nativeShareReceived` と同じ構造）。
  `gen/android/.../generated/` は触らず、`MainActivity.kt` とマニフェストだけを編集する。
- 回線の種類は `NetworkCapabilities.hasTransport(...)` で判定し、`Wifi | Ethernet | Cellular | Other | Unknown`。
  従量制は `NET_CAPABILITY_NOT_METERED` の有無で、`Metered | Unmetered | Unknown`。2つを別々に持つのは、
  無制限プランのモバイル通信が非従量と報告される場合や、従量制に設定した Wi-Fi があるため。
- マニフェストに `ACCESS_NETWORK_STATE`（通常権限。ユーザーへの確認は出ない）を追加する。
- 新規コマンド `get_network_status() -> NetworkStatus { transport, metered }`。デスクトップは `Unknown` を返す no-op。
- ダウンロード確認ダイアログを開くたびに値を読む。
  - `Cellular`: 「モバイル通信を使用中です。約 573MB を取得します。続けますか?」
  - `Cellular` 以外で従量制: 「従量制の接続です。約 573MB を取得します。続けますか?」
  - `Wifi` / `Ethernet` で非従量制: サイズ表示のみ。取得できなければ控えめな注記。
  - いずれもブロックはせず、続ける / キャンセルをユーザーが選ぶ。
- VPN 経由では回線の種類が見えないことがある（Android API の仕様からの推測で未検証）。その場合は `Other` とし、
  従量制の値だけで判断する。実装時に Pixel 8 Pro で Wi-Fi / モバイル通信 / VPN を切り替えて確認する。
- 範囲外: デスクトップの判定、ダウンロード中に従量制へ切り替わった場合の自動一時停止（Range での再開と手動キャンセルで対応）。

### 7. フロントエンド

- `ui/settings/ExternalIntegrationSection.svelte`: 「翻訳エンジン」（サーバー / PLaMo（端末内））を追加する。既存の
  `Dropdown` と `setUiPrefs({ ...app.ui, ... })` の保存方式に合わせる。PLaMo を選ぶとモデルの状態カード
  （状態、サイズ、進捗バー、ダウンロード / キャンセル / 削除、"Built with PLaMo"）を出す。
  モデル未取得でもエンジンの切り替え自体はできる。エンジンが PLaMo で、翻訳先が PLaMo 非対応の言語
  （プリセットでは `pt`）のときは、「この言語には対応していません」と注記を出す。
- `ui/NoteMenu.svelte`: 項目名をエンジン別にする（サーバー「翻訳」、ローカル「PLaMo で翻訳」）。表示条件は既存に加えて
  `get_translator_available` の結果。
- `ui/NoteCard.svelte`: 結果欄の見出しをエンジン別にする（サーバー「{言語} から翻訳」、ローカル「PLaMo で翻訳」）。
  `sourceLang` が無いときは言語を出さない。3つの `LOCAL_*` エラーの表示を加える。`translate_note` へ本文を渡す。
- 視覚値は `docs/design/style-guide.md` のスケールに従い、一回限りの値は使わない。

## Android とビルド

- `llama-cpp-2` は git 依存で rev を固定する（`955aa2f7`、llama.cpp v0.9.0 同梱）。crates.io の次のリリースに `plamo3`
  トークナイザが入ったら、通常のバージョン指定に戻す。
- features は spike で動いた構成。デスクトップは `default-features = false, features = ["common"]`、Android はそこに
  `android-static-stdcxx` を足す（OpenMP と共有 libc++ は使わない）。依存は `cfg(target_os = "android")` で分ける。
- 新しい Cargo feature `local-translate`（既定で有効）を足す。無効にすると C++ のビルドが走らず、ローカル翻訳は
  「非対応」として返る。重いビルドを避けたい開発時と、llama.cpp がビルドできない環境への逃げ道を兼ねる。
- Android は NDK r27c、API 26（minSdk）でビルドできることを確認済み。
- aarch64 の最適化は、**初回は既定の `armv8-a` のまま**にする。dotprod / i8mm を固定で有効にすると、非対応の端末で
  SIGILL になるため。Pixel 8 Pro で約 13.8 tok/s は実測済み。実行時に最適な版を選ぶ方式
  （`GGML_CPU_ALL_VARIANTS` + `GGML_BACKEND_DL`）は、別途検証してから入れる（フォローアップ）。
- メモリは、重みの再配置（repack）でヒープに約 0.6GB 以上を使う見込み。実測して `n_ctx` とアンロード時間を調整する。
  メモリ逼迫の通知（`onTrimMemory`）でのアンロードは今回入れない（フォローアップ）。
- ダウンロードにフォアグラウンドサービスは使わない。アプリが止められたら `.part` が残り、次回は途中から再開する。
- `android-build` CI ジョブ（aarch64 のビルド確認）に llama.cpp のコンパイルが加わり、約 2 分延びる。
- デスクトップは、Linux / macOS / Windows の各 CI で cmake が使えるかを確認する（`llama-cpp-sys-2` の固定した rev は、
  bindings を生成済みで bindgen を使わないので、libclang は要らない）。
  Windows と macOS は**未検証**なので、plan の最初のタスクにする。
- ローカルでの llama.cpp ビルドは、並列度 4・ディスク上の target・メモリ上限付きで行う
  （例: `CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4 CARGO_TARGET_DIR=$HOME/.cache/<名前> systemd-run --user --scope -p MemoryMax=10G cargo build`）。
  素の並列ビルドで OOM になった経験があるため、`CLAUDE.md` に追記する。
- APK サイズは ABI あたり約 7MB 増える見込み（spike の実行ファイルの大きさから推定）。

## テスト

Rust:

- `prompt.rs`: PFML3 の組み立てが期待どおりであること。本文に `<|plamo:tag|>` を含めても固定部分と混ざらないこと。停止判定。
- 言語コード変換の表（対応・非対応）。
- `model_store`: wiremock で Range 再開、ハッシュ不一致時の失敗と `.part` の破棄、サイズ不一致、空き容量不足、
  キャンセル、同時開始の拒否、ライセンス版が違うと拒否されること。
- `manager`: 偽の `LocalTranslator` で、直列化、遅延ロードが一度だけであること、アイドルのアンロード
  （`tokio` の時刻を進めるテスト）、エラーの対応。
- `translate_note` の振り分け（server は既存の wiremock、local は偽実装）。`effective_translate_engine` の正規化。
- `UiPrefs`: 旧 JSON（`translate_engine` なし）が `server` で読めること。同意版が設定のエクスポート・インポートに
  含まれないこと（`settings_transfer`）。
- 通信種別: Kotlin から渡る値を `NetworkStatus` に変換する純粋関数。
- 実モデルを使うテストは `#[ignore]` にし、モデルのパスを環境変数で渡す（翻訳の確認、本文のトークン化が一括の場合と
  一致すること、スペシャルトークンを含む本文での挙動）。
- `generates_frontend_bindings` で新コマンド・Event・`NetworkStatus` が出ること、`Translation.sourceLang` が
  nullable になることを確認する。

Vitest:

- NoteMenu / NoteCard: エンジン別の表記、`sourceLang` なし、3つのエラー表示。
- ExternalIntegrationSection: エンジン保存が他の項目を消さないこと、モデルカードの各状態、ライセンス同意ダイアログ、
  通信種別ごとの警告（Cellular / 従量制 / 不明 / 非従量制）、進捗の反映。

手動確認（実環境）:

- デスクトップは Xvfb 上（`WAYLAND_DISPLAY` も unset）の `cargo tauri dev` で、実際に Hugging Face からダウンロード・翻訳・削除を行い、
  SHA-256 の検証が通ることを確認する。
- Android は、エミュレーターで UI の流れを確認し、Pixel 8 Pro で速度と通信種別（Wi-Fi / モバイル通信 / VPN）を確認する。
- E2E（wdio）はモデルが重すぎるため追加しない。

## ドキュメント

- `docs/guide/user-guide.md` に使い方（エンジン選択、モデルの取得、ライセンス、通信量の注意）を追記する。
- `CLAUDE.md` に、`translate/` の構成、`local-translate` feature、llama.cpp ビルド時のメモリ注意を追記する。
- `2026-10-02-note-translate-design.md` のスコープ外の記述に、この spec へのリンクを追記する。

## 段階（plan で PR を分ける）

1. Rust コア（デスクトップ）: プロンプト、エンジン、マネージャー、コマンド、`UiPrefs`。Windows / macOS のビルド・CI の検証を最初に行う。
2. モデルの取得とライセンス同意、設定 UI。
3. Android: 通信種別、ビルド、CI、実機検証。

## 採用しなかった案（将来の再検討用）

### B. `llama-server` を外部プロセスで動かす

- 長所: C++ のビルドを Rust から切り離せる。
- 却下理由: Android ではアプリが同梱バイナリを子プロセスとして起動しにくく、実質使えない。ポート管理・起動停止・配布物の
  増加という運用負荷があり、デスクトップと Android で別実装になる。Android を同じ spec に含める以上、選べない。

### C. Rust ネイティブ実装（candle / burn など）

- 長所: C++ 依存がなくなる（ビルドの重さ、メモリ消費、CI への影響、静的リンクの問題が消える）。
- 却下理由: 今回の時点では PLaMo 3 の対応状況が未調査で、SWA・YaRN・独自トークナイザの自前実装が必要になる見込みで、
  工数とリスクが大きい。spike で llama.cpp が実用速度を出したため、まず A で価値を出す。
- **再検討する条件**: llama.cpp のビルド負荷が開発の障害になったとき。candle / burn に PLaMo 3 の対応が入ったとき。
  `llama-cpp-2` のメンテナンスが滞ったとき。
- **再検討時に確認すること**（いずれも未検証）:
  1. アーキテクチャ: GGUF のメタデータは `general.architecture = plamo3`、`sliding_window = 2048`、`rope.scaling.type = yarn`、
     `context_length = 262144`。llama.cpp の実装（`src/models/plamo3.cpp`）は SWA のパターンを 8 層周期で読み込む。
     これらを Rust ネイティブ側で再現できるか。
  2. トークナイザ: GGUF の `tokenizer.ggml.model = plamo3`（PLaMo 3 の unigram とプリセグメンテーション。
     llama.cpp では ggml-org/llama.cpp#30045 で実装）。参考実装として `yhirose/cpp-tensorlib` が C++ で PLaMo 3 を扱っている。
  3. 量子化: Q4_K_M の GGUF を読めるか（k-quant の対応）。
  4. 速度: 基準は spike の実測（デスクトップ CPU 約 54 tok/s、Pixel 8 Pro 約 13〜17 tok/s）。
  5. Android: NDK でのビルド、バイナリサイズ、SIMD の実行時選択。

## リリース前の確認事項（コード外）

- PFN への確認: 無料の OSS アプリが、ライセンスの「商用」（1条(10)、製品・サービスの提供を含む）に当たるか。
  アプリがダウンロードを案内・実行することが、4条の「第三者へのモデルの提供」に当たるか。
  あわせて、端末内での表示やユーザー自身のコピーが、5条(4)の「第三者への提供」に当たるか、当たる場合の周知の表示方法
  （条文は方法を指定していない）も確認する。
- 商用利用の事前登録（https://forms.gle/mTL8tBLrMYXKNZD56）。年商 10 億円以下が条件（3条(3)）。
  登録は無料なので、「商用」の解釈がどちらでも3条(3)を満たせる。リリース前に提出する。
- "Built with PLaMo" の表示箇所の最終確認。

## 実装時に確定する値

- モデルの SHA-256 と HF のリビジョン: HF API の `lfs.oid` から取得して定数にする。
- 言語コード変換表: 53 言語の一覧はモデルカードで確認済み（plan に全コードを記載）。
- アイドルのアンロード時間、`n_ctx`、入力上限: 初期値は上記のとおり。デスクトップと Android の実測で見直す。

## スコープ外

- 他のモデル・量子化の選択、GPU（Vulkan / Metal / OpenCL）の利用、Hexagon NPU。
- 推論の途中キャンセル、複数ノートの一括翻訳、翻訳結果のキャッシュ。
- 翻訳結果をそのまま投稿・共有する機能（作る場合は5条(4)の周知を設計に含める。§3 参照）。
- デスクトップの通信種別の判定、ダウンロード中の従量制への切り替え検知。
- Android の CPU 最適化の実行時選択、`onTrimMemory` でのアンロード、フォアグラウンドサービスでのダウンロード。
- 翻訳元言語の表示（PLaMo は翻訳元言語を判別するが、結果を返さないため、`local` では表示しない）。
- CW 文・投票選択肢の翻訳（#440 と同じ）。
