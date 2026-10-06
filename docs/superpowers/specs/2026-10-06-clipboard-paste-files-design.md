# ファイルマネージャでコピーしたファイルを投稿欄へ貼り付けて添付する (Issue #117)

## 背景・目的

Issue #57 / PR #64 でクリップボード**画像**(スクリーンショット等の生ピクセル)の貼り付けに対応済み(`docs/superpowers/specs/2026-07-25-clipboard-paste-image-design.md`)。一方、ファイルマネージャで画像・動画等の**ファイル自体**をコピーして貼り付けるケースは、OS クリップボード上で「ファイル参照」という別形式になるため対象外だった。本設計はその対応で、画像に限らず任意の種別のファイルを対象とする。

## 方式選定の経緯

公式 `tauri-plugin-clipboard-manager`(2.4.1)にはファイル参照を読む機能が無い。候補は次の通り。

- 非公式 `tauri-plugin-clipboard`(CrossCopy 製): 最終リリース 2024-10 で保守が止まっており不採用(Issue 記載の懸念どおり)。
- OS 別の自前実装(`CF_HDROP` / `public.file-url` / `text/uri-list`): 実装・保守コストが最大で不採用。
- Linux のみ GDK、Windows/macOS は `clipboard-rs`: 当初 `clipboard-rs` の Linux 対応は X11 のみと誤認して推奨したが、ソース調査で Wayland バックエンドが存在すると判明したため不要になった。
- **採用: `clipboard-rs` を全デスクトップ OS で直接使う**(プラグイン経由ではなくクレートを直接使う)。

### spike による検証結果(2026-10-06)

`clipboard-rs` 0.3.5(`wayland` feature、README の対応表は古いが `src/platform/wayland.rs` に実装あり)について、使い捨てプローブで実機確認した。

- 環境: Hyprland(`WAYLAND_DISPLAY=wayland-1`, `XDG_SESSION_TYPE=wayland`)。
- ファイルマネージャで動画ファイルをコピーした状態で `available_formats` は `text/uri-list`, `text/plain;charset=utf-8`, `UTF8_STRING`, `x-special/gnome-copied-files`。
- `get_files()` は `["file:///home/.../xxx.mp4"]` を返した(**非画像ファイルで動作を確認**)。
- `get_files()` の戻り値は `file://` URI 文字列のままで、パスへのデコードは呼び出し側の責務。
- Wayland バックエンドは内部で `wl-clipboard-rs`(`ext-data-control` / `wlr-data-control` プロトコル必須)を使う。`wl-clipboard-rs` は既に `Cargo.lock` に存在する(arboard 経由)。
- `text/plain` も同時にクリップボードに入っていた。従来の `handlePaste`(`text/plain` が非空なら介入しない)ではファイル貼り付けが素通りする。

未検証: GNOME(Mutter)の Wayland、X11、Windows、macOS、Dolphin 等の他ファイルマネージャ。

## スコープ

- 対象: デスクトップ(Linux/Windows/macOS)で、ファイルマネージャでコピーした 1 件以上のファイルを投稿欄に貼り付けて添付すること。
- 対象外: Android/iOS。添付数の上限制御、重複添付の排除、巨大ファイルのメモリ使用の改善(いずれも D&D と共通の既存挙動)。ファイルの「切り取り」は読み取って添付するだけで、元ファイルは削除しない。GNOME/X11/Windows/macOS での動作保証(コンパイルのみ CI で確認し、実機は未検証と明記する)。

## アーキテクチャ

アップロード処理は新規に書かない。取得したパスを D&D と同じ `addLocalAttachment(path)` に渡し、既存の `kind: "local"` 添付(投稿時に `uploadFile(path)`)として扱う。Issue #66 の「投稿時アップロード」原則(貼り付け直後にはアップロードしない)をそのまま満たす。

追加するのは「OS クリップボードからファイルパス一覧を読む Rust コマンド」と、`handlePaste` の分岐の見直しのみ。

## バックエンド

### 依存

`src-tauri/Cargo.toml` の `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`(`tauri-plugin-single-instance` と同じブロック)に追加する。

```toml
clipboard-rs = { version = "=0.3.5", default-features = false, features = ["wayland"] }
```

`url` クレートは既に依存にあるため追加不要。バージョンは spike で動作確認した 0.3.5 に固定する。

### コマンド `read_clipboard_files`

`commands/note.rs` の `read_clipboard_image` の隣に置き、`lib.rs` の `specta_builder()` の `collect_commands![]` に登録する。

```rust
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_files() -> Result<Vec<String>>
```

- デスクトップ: `tauri::async_runtime::spawn_blocking` 内で `ClipboardContext::new()` → `get_files()` を呼び、純粋関数 `file_uris_to_paths(uris: &[String]) -> Vec<String>` で実パスに変換して返す。
- モバイル(`cfg(any(target_os = "android", target_os = "ios"))`): `Ok(vec![])` を返す(コマンド登録は全プラットフォーム共通のため関数自体は残す)。
- ファイル参照が無い場合は `Ok(vec![])` を返し、エラーにしない(#57 の `Error::Invalid` による「画像なし」シグナルとは異なり、「ファイルが無い」は通常の分岐であるため)。
- クリップボードの初期化・読み取り失敗(data-control 非対応のコンポジタ等)も `Ok(vec![])` として扱い、`log::warn` で理由を出す(パスやトークンはログに出さない)。ユーザーにはエラーを表示せず、画像 → テキストのフォールバックに進ませる。

### `file_uris_to_paths`

- `url::Url::parse` → `to_file_path()` でパーセントデコードとプラットフォーム別パス変換(日本語・空白・`%` 入りのファイル名、Windows の `file:///C:/...`)を任せる。
- 次のものは黙って除外する: `file://` 以外のスキーム、ホスト付き URI(リモート)、変換失敗、**存在しないパス、ディレクトリ**(Misskey ドライブにフォルダはアップロードできないため。通常ファイルのみ残す)。
- 入力の CRLF・コメント行(`#` 始まり)・空行は `clipboard-rs` 側で除去済みだが、念のため空文字列は除外する。

### 権限

自前コマンドでプラグイン API を使わないため、`capabilities/default.json` の変更は不要。

## フロントエンド (`ComposeBar.svelte`)

制約: `preventDefault()` は `paste` イベントハンドラ内で**同期的に**呼ぶ必要がある。Rust への問い合わせ(非同期)の結果を待ってからでは既定のテキスト貼り付けを止められない。そのため同期的に取得できる `clipboardData.types` で判定する。

### 同期の判定 `shouldInterceptPaste`

`frontend/src/lib/` に純粋関数として切り出す。

```ts
shouldInterceptPaste(types: readonly string[], plainText: string): boolean
```

- `types` に `text/uri-list` または `Files` が**無く**、かつ `plainText` が非空 → `false`(通常のテキスト貼り付け。IPC も発生しないので通常のペーストに遅延は出ない)。
- それ以外(ファイル参照の兆候がある、または `plainText` が空)→ `true`。

### `handlePaste` の流れ

`shouldInterceptPaste(...)` が `false` なら何もしない。`true` なら `preventDefault()` して次を順に試す(優先順位: ファイル → 画像 → テキスト)。

1. `commands.readClipboardFiles()` が 1 件以上返す → 各パスを `addLocalAttachment(path)` に渡す(画像拡張子はプレビュー付き)。
2. 空なら従来どおり `commands.readClipboardImage()`(#57 の挙動。`invalid` は黙って無視)。
3. どちらも無く、元の `plainText` が非空だった場合(例: ブラウザで URL をコピーすると `text/uri-list` が付くが `file://` ではない)→ 止めたテキストを `document.execCommand("insertText", false, plainText)` でカーソル位置に挿入し直す(undo 履歴も保たれる)。

ファイルを画像より優先するのは、画像ファイルをコピーした場合に元のファイル名と形式のまま添付でき、生ピクセルから PNG へ再エンコードされないため。

### 未検証の前提とフォールバック

`clipboardData.types` に `text/uri-list` が実際に見えるかは、WebKitGTK / WKWebView / WebView2 のいずれでも**未検証**(#57 の調査では、画像は WebKitGTK で `types` が空だった)。実装計画の最初のタスクで確認する(`dbus-run-session` + Xvfb 上で `xclip -t text/uri-list` で偽装し、debug bridge で `paste` イベントの `types` を読む)。

見えなければ、フォールバックとして「常に `preventDefault` して Rust で全判定し、テキストは `insertText` で戻す」方式に切り替える。この場合、通常のテキスト貼り付けにも IPC が 1 往復入る。

## セキュリティ上の考慮

取得したパスはそのまま既存の `uploadFile` に渡る。D&D と信頼レベルは同じで、クリップボード由来でパスを受ける入口が新たに増える。Web ページが `copy` イベントで `text/uri-list` を細工してクリップボードに載せ、意図しないローカルパスを添付させられるかは**未確認**(Linux で実際に system clipboard に出るかも含めて)。仮に可能でも、添付欄にファイル名・サムネイルが表示され、**投稿ボタンを押すまでアップロードされない**ため、低リスクと判断する。

## エラーハンドリング

- `read_clipboard_files` の失敗は「ファイル参照なし」として扱い、ユーザーには表示しない(上述)。
- 添付後のアップロード失敗は、既存の `local` 項目と同じ経路(`failedAttachmentId` / `err`)で扱う。

## テスト方針

- Rust(単体): `file_uris_to_paths` を、空白・日本語・`%` 記号入りのファイル名、複数 URI、`file://` 以外、ホスト付き URI、存在しないパス、ディレクトリ、空文字列で検証する(一時ディレクトリに実ファイルを作る)。Windows のドライブレター形式は `#[cfg(windows)]` のテスト。クリップボード I/O 自体は OS 依存のため単体テスト対象外。
- フロント(Vitest): `shouldInterceptPaste` の真理値表を検証する。
- 型チェック: `cargo test`(bindings 再生成を含む)と `pnpm check`。

## 手動確認(Hyprland 実機、`cargo tauri dev`)

1. 動画ファイルをコピーして Ctrl+V → 添付欄に出る(投稿前にアップロードされていない)→ 投稿できる。
2. 画像ファイルをコピーして Ctrl+V → サムネイル付きで元のファイル名のまま添付される。
3. 複数ファイルを同時にコピーして Ctrl+V → 全件が添付される。
4. スクリーンショット画像の貼り付け(#57)、通常のテキスト貼り付け、ブラウザで URL をコピーして貼り付け → 従来どおり動く。
5. ディレクトリのみをコピーして貼り付け → 添付はされず、従来どおりクリップボードの `text/plain`(パス等)が本文に挿入される(ファイル参照が得られない場合は常に従来のテキスト貼り付けに劣化する、という設計どおりの挙動)。
