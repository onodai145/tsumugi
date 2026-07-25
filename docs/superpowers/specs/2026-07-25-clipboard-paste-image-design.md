# クリップボードから画像を貼り付けて添付する (Issue #57)

## 背景・目的

Issue #57 のコメントに記録済みの調査結果: DOM の `paste` イベント(`clipboardData.types`/`items`/`files`)は WebKitGTK(Linux)では画像コンテンツに対して常に空になる既知の制限があり、テキストのみ取得できる。そのため画像貼り付けは JS 側の DOM paste イベントに頼らず、OS クリップボードから直接読む方式が必要。

本設計ではスクリーンショットツール等でコピーした**画像**の貼り付けのみを対象とする。ファイルマネージャでコピーしたファイルパス(画像以外を含む)の貼り付けは対象外とし、別Issueとする。動画の直接貼り付けというクリップボード形式は主要OSに存在せず(動画の「コピー」は実質ファイル参照コピーであり上記ファイルパス貼り付けと同じカテゴリ)、今回のスコープには含まれない。

## 採用プラグイン

公式 `tauri-plugin-clipboard-manager`(`@tauri-apps` 配下、Tauri v2 対応)を採用する。`read_image()` は生 RGBA ピクセル配列(+width/height)を返すため、Rust側で PNG エンコードが必要(軽量な `png` crate を追加する。画像処理全般が要る `image` crate までは不要)。

非公式の `tauri-plugin-clipboard`(CrossCopy製)は `read_files` を持ち画像・ファイル両対応だが、最終リリースが約1年以上更新なく doc coverage も薄いため保守性の観点で今回は採用しない。クリップボード読み取り部分は1コマンドに閉じ込めて設計するため、将来ファイルパス貼り付けが必要になった際の乗り換えコストは小さい。

## アップロードタイミング(投稿時に遅延させる)

`docs/superpowers/plans`の過去実装(Issue #66、`2026-07-20-upload-on-submit-design.md`)により、ローカルファイル添付は選択直後ではなく**投稿ボタン押下時**にアップロードされる設計になっている(投稿せずにコンポーズ欄を閉じた場合にドライブへ孤児ファイルが残らないようにするため)。クリップボード画像も同じ原則に従い、**貼り付け直後にはアップロードしない**。

そのため、クリップボード読み取り用のコマンドは「OSクリップボードの画像を読んでPNGバイト列を返すだけ」に留め、ドライブへのアップロードは行わない。実際のアップロードは、既存の投稿時アップロードループ(`submit()`)が担う。ここで PR #64 で追加済みの汎用コマンド `commands::note::upload_bytes(account_id, filename, bytes: Vec<u8>) -> Result<DriveFile>`(フロントから直接バイト列を受け取り `api::drive::upload_bytes` を呼ぶ)をそのまま使う。

## アーキテクチャ / データフロー

1. 投稿欄(`ComposeBar.svelte` の textarea)に `onpaste` ハンドラを追加する。
2. `e.clipboardData?.getData("text/plain")` が非空なら何もしない(`preventDefault` しない)。ブラウザ標準のテキスト貼り付けをそのまま働かせる。
3. 空の場合のみ `e.preventDefault()` し、新規コマンド `commands.readClipboardImage()` を呼ぶ(アカウント選択は不要。読み取りだけなのでアカウントに依存しない)。
4. Rust側の新規コマンドが `tauri-plugin-clipboard-manager` 経由でOSクリップボードの画像を読み、PNGへエンコードし、ファイル名(`clipboard-YYYYMMDD-HHMMSS-mmm.png`)とPNGバイト列を返す(アップロードはしない)。
5. フロントは返ってきたバイト列を `Blob`/`URL.createObjectURL` でプレビューURLに変換し、`attachments` に「ローカル未アップロード」項目として追加する(挙動は `pickFiles()` で選んだローカルファイルと同列)。
6. `submit()` 時、この項目に対して `commands.uploadBytes(accountId, filename, bytes)` を呼びドライブへアップロードしてから投稿する(ローカルファイル項目が `uploadFile` を呼ぶのと同じ扱い)。

## 既存ブランチとの関係

本Issue向けの土台として、ブランチ `feat/issue-57-paste-clipboard-attachment`(PR #64、ドラフト)に以下がコミット済み(`32f2c07`、mainへrebase/merge済み):

- `api::drive::upload_bytes(http, host, token, bytes: Vec<u8>, filename: String) -> Result<DriveFile>`: マルチパートアップロード処理を `upload_file` から切り出した内部関数。
- `commands::note::upload_bytes(account_id, filename, bytes: Vec<u8>) -> Result<DriveFile>`: フロントから直接バイト列を受け取る汎用コマンド。当初はDOM paste方式向けに追加されたが、本設計で「投稿時にクリップボード画像をアップロードする」用途として使う(温存しておいた判断がそのまま活きる)。

## バックエンド変更

- `Cargo.toml`: `tauri-plugin-clipboard-manager` と `png` crate を追加。
- `capabilities/default.json`: `clipboard-manager:allow-read-image` 権限を追加。
- `lib.rs`: `specta_builder()` の `collect_commands![]` に新コマンド `read_clipboard_image` を登録し、`tauri_builder.plugin(tauri_plugin_clipboard_manager::init())` を追加。
- `commands/note.rs`: 新規コマンド

  ```rust
  #[derive(Debug, Clone, Serialize, specta::Type)]
  #[serde(rename_all = "camelCase")]
  pub struct ClipboardImage {
      pub filename: String,
      pub bytes: Vec<u8>,
  }

  #[tauri::command]
  #[specta::specta]
  pub async fn read_clipboard_image(app: AppHandle) -> Result<ClipboardImage>
  ```

  - `tauri-plugin-clipboard-manager` の `read_image()` で画像を取得(取得失敗 = クリップボードに画像が無い場合は `Error::Invalid(...)` を返す)。ブロッキングI/Oのため `tauri::async_runtime::spawn_blocking` 内で実行する。
  - `png` crate で RGBA バイト列を PNG にエンコード(エンコード失敗も `Error::Invalid(...)`)。
  - ファイル名はローカル日時ベースで `clipboard-YYYYMMDD-HHMMSS-mmm.png`(例: `clipboard-20260725-153045-123.png`、UTC基準)の形式で生成する。秒単位だけだと連続貼り付けで衝突しうるためミリ秒まで含める。
  - アカウント情報は不要(読み取りのみでアップロードしないため `account_id` 引数を持たない)。

  このコマンド内では `Error::Invalid` を「画像が実質無い/内部処理異常」を表す専用シグナルとして扱う。

## フロントエンド変更 (`ComposeBar.svelte`)

- `AttachmentItem` に `{ kind: "clipboard"; id: string; name: string; bytes: number[]; previewUrl: string }` を追加する(`kind: "local"` のクリップボード版。パスの代わりにバイト列を持つ。フィールド名 `name` は `local` と揃え、アップロード時のファイル名として使う)。
- `onpaste` ハンドラ:

  ```ts
  async function handlePaste(e: ClipboardEvent) {
    if (e.clipboardData?.getData("text/plain")) return;
    e.preventDefault();
    const r = await commands.readClipboardImage();
    if (r.status === "error") {
      if (r.error.kind !== "invalid") err = formatError(r.error);
      return;
    }
    const blob = new Blob([new Uint8Array(r.data.bytes)], { type: "image/png" });
    const previewUrl = URL.createObjectURL(blob);
    attachments = [
      ...attachments,
      { kind: "clipboard", id: crypto.randomUUID(), name: r.data.filename, bytes: r.data.bytes, previewUrl },
    ];
  }
  ```

  `unwrap()` は使わず、`unwrapAcc` と同様に生の IPC バインディングを直接呼んで `r.error.kind` を判定する。アカウント未選択でも貼り付け自体は可能(既存の `pickFiles()` と同様、アップロードは投稿時まで発生しないため)。
- `submit()` のアップロードループ(`kind === "local"` の項目を `uploadFile` する箇所)に `kind === "clipboard"` の分岐を追加し、`commands.uploadBytes(accountId, a.name, a.bytes)` を呼んで成功したら `kind: "drive"` に置き換える(`local` 項目の扱いと同一パターン)。
- サムネイル表示部は `kind === "clipboard"` を `previewUrl` があるケースとして扱い、既存の `<img class="thumb" src={a.previewUrl} alt="" />` 分岐に含める(`kind === "local" && a.previewUrl` と同様の見た目)。`previewUrl` は常に非nullの文字列だが、`local`の`previewUrl: string | null`と型を合わせるため`AttachmentItem`上は`string`のまま(nullチェックは`local`側のみ意味を持つ)とする。

## エラーハンドリング

- `text/plain` が非空の場合は一切介入しない。
- クリップボードに画像が無い場合(`kind === "invalid"`)は何もしない(`err` は出さない。そもそも貼るものが無かっただけのため)。
- 投稿時のアップロード失敗(ネットワーク/認証/レート制限等)は、既存の `local` 項目のアップロード失敗と同じ経路(`failedAttachmentId` / `err`)で扱う。

## テスト方針

- Rust: PNG エンコードやファイル名生成(`clipboard-YYYYMMDD-HHMMSS-mmm.png`)など純粋関数部分を単体テストする。`read_image()` 自体はOSクリップボード依存でモック困難なため単体テスト対象外とする。
- フロント: `pnpm check` の型チェックのみ。
- 手動確認(`cargo tauri dev`): スクリーンショットツールで画像をコピー → 投稿欄にフォーカスして Ctrl+V → サムネイルにプレビューが反映される(まだアップロードされていない) → 投稿してアップロード・ノート作成まで確認する。あわせて、通常のテキスト貼り付け(Ctrl+V でテキストのみクリップボードにある場合)が従来通り動作すること、貼り付けたまま投稿せずコンポーズ欄を閉じてもドライブに何もアップロードされていないことも確認する。
