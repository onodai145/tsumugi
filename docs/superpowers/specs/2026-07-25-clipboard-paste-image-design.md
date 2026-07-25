# クリップボードから画像を貼り付けて添付する (Issue #57)

## 背景・目的

Issue #57 のコメントに記録済みの調査結果: DOM の `paste` イベント(`clipboardData.types`/`items`/`files`)は WebKitGTK(Linux)では画像コンテンツに対して常に空になる既知の制限があり、テキストのみ取得できる。そのため画像貼り付けは JS 側の DOM paste イベントに頼らず、OS クリップボードから直接読む方式が必要。

本設計ではスクリーンショットツール等でコピーした**画像**の貼り付けのみを対象とする。ファイルマネージャでコピーしたファイルパス(画像以外を含む)の貼り付けは対象外とし、別Issueとする。動画の直接貼り付けというクリップボード形式は主要OSに存在せず(動画の「コピー」は実質ファイル参照コピーであり上記ファイルパス貼り付けと同じカテゴリ)、今回のスコープには含まれない。

## 採用プラグイン

公式 `tauri-plugin-clipboard-manager`(`@tauri-apps` 配下、Tauri v2 対応)を採用する。`read_image()` は生 RGBA ピクセル配列(+width/height)を返すため、Rust側で PNG エンコードが必要(軽量な `png` crate を追加する。画像処理全般が要る `image` crate までは不要)。

非公式の `tauri-plugin-clipboard`(CrossCopy製)は `read_files` を持ち画像・ファイル両対応だが、最終リリースが約1年以上更新なく doc coverage も薄いため保守性の観点で今回は採用しない。クリップボード読み取り部分は後述の通り1コマンドに閉じ込めて設計するため、将来ファイルパス貼り付けが必要になった際の乗り換えコストは小さい。

## アーキテクチャ / データフロー

1. 投稿欄(`ComposeBar.svelte` の textarea)に `onpaste` ハンドラを追加する。
2. `e.clipboardData?.getData("text/plain")` が非空なら何もしない(`preventDefault` しない)。ブラウザ標準のテキスト貼り付けをそのまま働かせる。
3. 空の場合のみ `e.preventDefault()` し、`commands.uploadClipboardImage(accountId)` を呼ぶ。
4. Rust側の新規コマンドが `tauri-plugin-clipboard-manager` 経由でOSクリップボードの画像を読み、PNGへエンコードし、`drive/files/create` へアップロードして `DriveFile` を返す。
5. フロントは返ってきた `DriveFile` を `attachments` に追加する。

## 既存ブランチとの関係

本Issue向けの土台として、ブランチ `feat/issue-57-paste-clipboard-attachment`(PR #64、ドラフト)に以下がコミット済み(`32f2c07`):

- `api::drive::upload_bytes(http, host, token, bytes: Vec<u8>, filename: String) -> Result<DriveFile>`: マルチパートアップロード処理を `upload_file` から切り出した内部関数。本設計の `upload_clipboard_image` はこれをそのまま再利用する(追加のリファクタは不要)。
- `commands::note::upload_bytes(account_id, filename, bytes: Vec<u8>) -> Result<DriveFile>`: フロントから直接バイト列を受け取る汎用コマンド。差し戻し済みのDOM paste方式向けに追加されたもので、本設計(クリップボード読み取り〜PNGエンコードまで全てRust側の`upload_clipboard_image`に閉じ込める)では呼び出し元が無くなるが、将来の汎用バイト列アップロード用途(D&D等)に備えて削除せず残す。

## バックエンド変更

- `Cargo.toml`: `tauri-plugin-clipboard-manager` と `png` crate を追加。
- `lib.rs`: `specta_builder()` の `collect_commands![]` に新コマンド `upload_clipboard_image` を登録し、`tauri_builder.plugin(tauri_plugin_clipboard_manager::init())` を追加。
- `commands/note.rs`: 新規コマンド

  ```rust
  #[tauri::command]
  #[specta::specta]
  pub async fn upload_clipboard_image(
      app: AppHandle,
      state: State<'_, AppState>,
      account_id: String,
  ) -> Result<DriveFile>
  ```

  - `tauri-plugin-clipboard-manager` の `read_image()` で画像を取得(取得失敗 = クリップボードに画像が無い場合は `Error::Invalid(...)` を返す)。
  - `png` crate で RGBA バイト列を PNG にエンコード(エンコード失敗も `Error::Invalid(...)`)。
  - ファイル名はローカル日時ベースで `clipboard-YYYYMMDD-HHMMSS-mmm.png`(例: `clipboard-20260725-153045-123.png`)の形式で生成する(`chrono` crateを使用)。秒単位だけだと連続貼り付けで衝突しうるためミリ秒まで含める。
  - `state.host_token(&account_id)` で取得したホスト/トークンと共に `api::drive::upload_bytes`(既存・上記参照)を呼ぶ(このアップロード自体の失敗は `Network`/`Unauthorized`/`Api` 等、通常の `upload_file` と同じエラー種別がそのまま伝播する)。

  このコマンド内では `Error::Invalid` を「画像が実質無い/内部処理異常」を表す専用シグナルとして扱い、それ以外のエラー種別(Network/Unauthorized/Forbidden/RateLimited/Api)は実アップロード失敗として区別する。

## フロントエンド変更 (`ComposeBar.svelte`)

- `AttachmentItem` に `{ kind: "uploading"; id: string }` を追加する。
- `onpaste` ハンドラ:

  ```ts
  async function handlePaste(e: ClipboardEvent) {
    if (e.clipboardData?.getData("text/plain")) return;
    e.preventDefault();
    if (!accountId) {
      err = "アカウントを選択してください";
      return;
    }
    const placeholderId = crypto.randomUUID();
    attachments = [...attachments, { kind: "uploading", id: placeholderId }];
    const r = await commands.uploadClipboardImage(accountId);
    if (r.status === "error") {
      if (r.error.kind !== "invalid") err = formatError(r.error);
      attachments = attachments.filter((a) => a.id !== placeholderId);
      return;
    }
    attachments = attachments.map((a) =>
      a.id === placeholderId ? { kind: "drive", id: r.data.id, file: r.data } : a,
    );
  }
  ```

  `unwrap()` は使わず、`unwrapAcc` と同様に生の IPC バインディングを直接呼んで `r.error.kind` を判定する。
- サムネイル表示部(`{#each attachments as a}`)に `kind === "uploading"` の分岐を追加し、既存の `.thumb.badge` と同じ見た目で「アップロード中」を示す暫定表示を出す(画像プレビューは無いため汎用バッジのみ)。

## エラーハンドリング

- `text/plain` が非空の場合は一切介入しない。
- アカウント未選択時は `preventDefault` のみ行い、ネイティブ読み取りは呼ばない。
- クリップボードに画像が無い場合(`kind === "invalid"`)はプレースホルダを黙って削除するのみで `err` は出さない(そもそも貼るものが無かっただけのため)。
- ネットワーク/認証/レート制限等の実アップロード失敗は `err` に表示し、プレースホルダを削除する。

## テスト方針

- Rust: PNG エンコードやファイル名生成(`clipboard-<timestamp>.png`)など純粋関数部分を単体テストする。`read_image()` 自体はOSクリップボード依存でモック困難なため単体テスト対象外とする。
- フロント: `pnpm check` の型チェックのみ。
- 手動確認(`cargo tauri dev`): スクリーンショットツールで画像をコピー → 投稿欄にフォーカスして Ctrl+V → サムネイルに反映 → 投稿できることを確認する。あわせて、通常のテキスト貼り付け(Ctrl+V でテキストのみクリップボードにある場合)が従来通り動作することも確認する。
