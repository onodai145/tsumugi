# クリップボード画像貼り付け(Issue #57) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 投稿欄(`ComposeBar.svelte`)にフォーカスした状態でCtrl/Cmd+Vを押すと、OSクリップボード上の画像(スクリーンショットツール等でコピーしたもの)をPNGとして添付できるようにする。アップロードは他のローカル添付ファイルと同じく投稿ボタン押下時まで遅延させ、投稿せずに閉じた場合にドライブへ孤児ファイルが残らないようにする。

**Architecture:** フロントは`paste`イベントでテキストが無い場合のみ新規Tauriコマンド`read_clipboard_image()`(アカウント不要)を呼ぶ。バックエンドは`tauri-plugin-clipboard-manager`でOSクリップボードの生RGBA画像を読み、`png` crateでPNGへエンコードし、ファイル名とバイト列を返すだけでアップロードはしない。フロントはそれを`kind: "clipboard"`の未アップロード添付として保持し、投稿ボタン押下時に既存の汎用コマンド`upload_bytes`(PR #64で追加済み)でドライブへアップロードしてから投稿する。クリップボードに画像が無い場合は`Error::Invalid`を返し、フロントはそれを「実質何もしなかった」として黙って処理する。

**Tech Stack:** Rust(Tauri v2, tauri-specta, `tauri-plugin-clipboard-manager`, `png` crate, `chrono`), TypeScript/Svelte 5

## Global Constraints

- ブランチ: `feat/issue-57-paste-clipboard-attachment`(PR #64)。既にmainへmerge済みで最新。作業はこのブランチ上で続ける。
- `src-tauri/src/lib.rs`の`specta_builder()`に新コマンドを登録しないとTSバインディングに出てこない(CLAUDE.md参照)。
- コマンド追加後は必ず`cd src-tauri && cargo test`を実行し、`frontend/src/bindings/tauri.gen.ts`を再生成すること(手動編集しない)。
- フロントの型チェックは`cd frontend && pnpm check`。
- 画像貼り付けのみが対象。ファイルパスコピーや動画は対象外(設計書 `docs/superpowers/specs/2026-07-25-clipboard-paste-image-design.md` 参照)。
- ファイル名は `clipboard-YYYYMMDD-HHMMSS-mmm.png`(UTC基準、ミリ秒3桁)。
- クリップボード画像は**貼り付け時にアップロードしない**。アップロードは`submit()`(投稿時)まで遅延させる(Issue #66の設計原則を踏襲)。

---

### Task 1: `tauri-plugin-clipboard-manager` / `png` crate の追加とプラグイン登録

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `tauri_plugin_clipboard_manager::ClipboardExt`(`app.clipboard()`拡張メソッド)と`png`crateがビルド可能な状態。Task 2で使用する。

- [ ] **Step 1: Cargo.toml に依存を追加**

`src-tauri/Cargo.toml`の`tauri-plugin-os = "2"`の行の直後に以下を追加する。

```toml
tauri-plugin-os = "2"
tauri-plugin-clipboard-manager = "2"
png = "0.18"
```

- [ ] **Step 2: capabilities に権限を追加**

`src-tauri/capabilities/default.json`の`"permissions"`配列に`"os:allow-platform"`の直後、以下を追加する。

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "enables the default permissions",
  "windows": [
    "main"
  ],
  "permissions": [
    "core:default",
    {
      "identifier": "opener:allow-open-url",
      "allow": [{ "url": "https://*" }, { "url": "http://*" }]
    },
    "dialog:allow-open",
    "dialog:allow-save",
    "notification:default",
    "os:allow-platform",
    "clipboard-manager:allow-read-image"
  ]
}
```

- [ ] **Step 3: lib.rs にプラグインを登録**

`src-tauri/src/lib.rs`の`.plugin(tauri_plugin_os::init())`の行の直後に以下を追加する(既存の`.plugin(...)`チェーンに1行差し込む形)。

```rust
    tauri_builder
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
```

- [ ] **Step 4: ビルドして依存追加のみで壊れていないことを確認**

Run: `cd src-tauri && cargo build`
Expected: `Finished` で正常終了(新規コマンドはまだ無いので警告等は出ない)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/capabilities/default.json src-tauri/src/lib.rs
git commit -m "feat: tauri-plugin-clipboard-managerとpng crateを追加"
```

---

### Task 2: PNGエンコード・ファイル名生成の純粋関数を実装(TDD)

**Files:**
- Modify: `src-tauri/src/commands/note.rs`

**Interfaces:**
- Consumes: なし(標準ライブラリ・`png`・`chrono`のみ)
- Produces:
  - `fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>>`
  - `fn clipboard_filename(millis: i64) -> String`
  - Task 3の`read_clipboard_image`コマンドがこの2つを使用する。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/note.rs`末尾の`#[cfg(test)] mod tests { ... }`ブロック内、既存の`guess_attachment_image_mime_falls_back_for_unknown_or_video_extensions`テストの直後に追加する。

```rust
    #[test]
    fn encode_png_rgba_produces_valid_png_signature() {
        // 2x1 の赤・青ピクセル(RGBA)
        let rgba = [255u8, 0, 0, 255, 0, 0, 255, 255];
        let png_bytes = encode_png_rgba(&rgba, 2, 1).expect("encode should succeed");
        // PNG シグネチャ: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&png_bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn clipboard_filename_formats_utc_datetime_with_millis() {
        // 2026-07-25T15:30:45.123Z の Unix ミリ秒
        let millis = 1784993445123;
        assert_eq!(clipboard_filename(millis), "clipboard-20260725-153045-123.png");
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd src-tauri && cargo test --lib commands::note::tests::encode_png_rgba_produces_valid_png_signature commands::note::tests::clipboard_filename_formats_utc_datetime_with_millis`
Expected: コンパイルエラー(`encode_png_rgba`と`clipboard_filename`が未定義)。

- [ ] **Step 3: 最小実装を書く**

`src-tauri/src/commands/note.rs`の`guess_attachment_image_mime`関数の直後に追加する。

```rust
/// RGBA8 のピクセル配列を PNG バイト列にエンコードする(クリップボード貼り付け画像用)。
fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Invalid(format!("PNGヘッダ書き込みに失敗しました: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| Error::Invalid(format!("PNGエンコードに失敗しました: {e}")))?;
    writer
        .finish()
        .map_err(|e| Error::Invalid(format!("PNG書き込みの完了に失敗しました: {e}")))?;
    Ok(buf)
}

/// ミリ秒Unix時刻から `clipboard-YYYYMMDD-HHMMSS-mmm.png` 形式のファイル名を生成する(UTC基準)。
fn clipboard_filename(millis: i64) -> String {
    let dt = chrono::DateTime::from_timestamp_millis(millis)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp_millis(0).expect("timestamp 0 is valid"));
    format!("clipboard-{}.png", dt.format("%Y%m%d-%H%M%S-%3f"))
}
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd src-tauri && cargo test --lib commands::note::tests::`
Expected: `test result: ok.` (新規2件を含む全テストがpass)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/note.rs
git commit -m "feat: クリップボード画像用のPNGエンコード/ファイル名生成関数を追加"
```

---

### Task 3: `read_clipboard_image` コマンドを追加

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `encode_png_rgba`, `clipboard_filename`(Task 2)
- Produces:
  - `pub struct ClipboardImage { pub filename: String, pub bytes: Vec<u8> }`(specta::Type + Serialize、TS側は`{ filename: string; bytes: number[] }`)
  - `#[tauri::command] async fn read_clipboard_image(app: AppHandle) -> Result<ClipboardImage>`。Task 4のフロントが`commands.readClipboardImage()`として呼ぶ(引数なし)。

- [ ] **Step 1: note.rs にコマンドを追加**

`src-tauri/src/commands/note.rs`冒頭のimportを以下のように変更する(`Serialize`, `specta::Type`, `ClipboardExt`を追加)。

```rust
use crate::api::drive::{
    list_files as api_list_files, list_folders as api_list_folders, upload_bytes as api_upload_bytes,
    upload_file as api_upload_file,
};
use crate::api::meta::list_emojis;
use crate::api::notes::{
    create_favorite, create_note, create_reaction, delete_favorite, delete_note, delete_reaction,
    renote as api_renote, vote_poll as api_vote_poll, NoteDraft, VisibilityInput,
};
use crate::domain::{DriveFile, EmojiDef, Note, SourceItem};
use crate::error::{Error, Result};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
```

`upload_bytes`コマンド定義の直後(`list_drive_folders`より後、`save_url_to_file`より前)に以下を追加する。

```rust
/// `read_clipboard_image` の戻り値。アップロードはせず、フロントは投稿時まで保持してから
/// `upload_bytes` へ渡す(Issue #66 の「投稿時アップロード」原則を踏襲するため)。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardImage {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// クリップボードの画像を読み、PNGへエンコードして返す(アップロードはしない)。
/// クリップボードに画像が無い場合や PNG エンコードに失敗した場合は `Error::Invalid` を返す
/// (このコマンド内では Invalid を「実質画像が無い/内部処理異常」を表す専用シグナルとして扱う)。
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_image(app: AppHandle) -> Result<ClipboardImage> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let image = app
            .clipboard()
            .read_image()
            .map_err(|e| Error::Invalid(format!("クリップボードに画像がありません: {e}")))?;
        let width = image.width();
        let height = image.height();
        let rgba = image.rgba().to_vec();
        Ok::<(Vec<u8>, u32, u32), Error>((rgba, width, height))
    })
    .await
    .map_err(|e| Error::Invalid(format!("クリップボード読み取りに失敗しました: {e}")))??;

    let png_bytes = encode_png_rgba(&rgba, width, height)?;

    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let filename = clipboard_filename(millis);

    Ok(ClipboardImage { filename, bytes: png_bytes })
}
```

- [ ] **Step 2: commands/mod.rs の再エクスポートに追加**

`src-tauri/src/commands/mod.rs`の`pub use note::{ ... };`に`read_clipboard_image`を追加する。

```rust
pub use note::{
    delete_note_cmd, favorite_note, list_custom_emojis, post_note, react, read_clipboard_image, renote,
    unfavorite_note, unreact, upload_bytes, upload_file,
};
```

- [ ] **Step 3: lib.rs の specta_builder に登録**

`src-tauri/src/lib.rs`の`commands::note::upload_bytes,`の直後に追加する。

```rust
            commands::note::upload_file,
            commands::note::upload_bytes,
            commands::note::read_clipboard_image,
            commands::note::list_drive_files,
```

- [ ] **Step 4: ビルドとテスト実行(TSバインディング再生成を含む)**

Run: `cd src-tauri && cargo build && cargo test`
Expected: ビルド成功、`test result: ok.`(`specta_export::generates_frontend_bindings`含め全件pass)。

- [ ] **Step 5: TSバインディングに `readClipboardImage` と `ClipboardImage` 型が生成されたことを確認**

Run: `grep -n "readClipboardImage\|ClipboardImage" frontend/src/bindings/tauri.gen.ts`
Expected: `readClipboardImage: () => typedError<ClipboardImage, Error>(__TAURI_INVOKE("read_clipboard_image")),` に相当する行と、`type ClipboardImage = { filename: string; bytes: number[] }` に相当する型定義が出力される。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: read_clipboard_image コマンドを追加"
```

---

### Task 4: `ComposeBar.svelte` にクリップボード貼り付けを実装

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: `commands.readClipboardImage()`(Task 3)、`commands.uploadBytes(accountId, filename, bytes)`(既存)、`formatError`(`../lib/ipc`、既存)
- Produces: `handlePaste(e: ClipboardEvent)`関数と`AttachmentItem`型への`"clipboard"`バリアント追加。他ファイルからは参照されない(このファイル内で完結)。

- [ ] **Step 1: import に formatError を追加**

`frontend/src/ui/ComposeBar.svelte`冒頭の以下の行を変更する。

```ts
  import { commands, unwrap } from "../lib/ipc";
```

を

```ts
  import { commands, unwrap, formatError } from "../lib/ipc";
```

に変更する。

- [ ] **Step 2: AttachmentItem 型に clipboard バリアントを追加**

以下の型定義を変更する。

```ts
  type AttachmentItem =
    | { kind: "local"; id: string; path: string; name: string; previewUrl: string | null }
    | { kind: "drive"; id: string; file: DriveFile };
```

を

```ts
  type AttachmentItem =
    | { kind: "local"; id: string; path: string; name: string; previewUrl: string | null }
    | { kind: "drive"; id: string; file: DriveFile }
    | { kind: "clipboard"; id: string; name: string; bytes: number[]; previewUrl: string };
```

に変更する。

- [ ] **Step 3: handlePaste 関数を追加**

`removeAttached`関数の直後に以下を追加する。

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

- [ ] **Step 4: textarea に onpaste を追加**

投稿本文の`<textarea>`要素の属性を変更する。

```svelte
    bind:value={text}
    bind:this={textarea}
    onkeydown={onKey}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
  ></textarea>
```

を

```svelte
    bind:value={text}
    bind:this={textarea}
    onkeydown={onKey}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    onpaste={handlePaste}
  ></textarea>
```

に変更する。

- [ ] **Step 5: submit() のアップロードループに clipboard 分岐を追加**

以下のブロックを変更する。

```ts
      for (const a of attachments) {
        if (a.kind === "drive") continue;
        uploadingAttachmentId = a.id;
        let file: DriveFile;
        try {
          file = await unwrap(commands.uploadFile(accountId, a.path));
        } catch (e) {
          failedAttachmentId = a.id;
          err = String(e);
          return;
        } finally {
          uploadingAttachmentId = null;
        }
        attachments = attachments.map((x) => (x.id === a.id ? { kind: "drive", id: file.id, file } : x));
      }
```

を

```ts
      for (const a of attachments) {
        if (a.kind === "drive") continue;
        uploadingAttachmentId = a.id;
        let file: DriveFile;
        try {
          file =
            a.kind === "clipboard"
              ? await unwrap(commands.uploadBytes(accountId, a.name, a.bytes))
              : await unwrap(commands.uploadFile(accountId, a.path));
        } catch (e) {
          failedAttachmentId = a.id;
          err = String(e);
          return;
        } finally {
          uploadingAttachmentId = null;
        }
        attachments = attachments.map((x) => (x.id === a.id ? { kind: "drive", id: file.id, file } : x));
      }
```

に変更する。

- [ ] **Step 6: サムネイル表示の name アクセスを "local" 限定に絞る**

`kind === "clipboard"` は `previewUrl` が常に非nullの文字列のため既存の `{:else if a.previewUrl}` 分岐にそのまま入るが、その次の `{:else}` 分岐(バッジ表示)は型上 `"local" | "clipboard"` の両方に到達しうるにもかかわらず `a.name` は両方に存在する(Step 2 で `clipboard` にも `name` を持たせているため)ので、この分岐自体は変更不要。念のため以下のブロックが変更なしで型エラーが出ないことを Step 7 の `pnpm check` で確認する。

```svelte
          {#if a.kind === "drive"}
            {#if a.file.mimeType.startsWith("image/")}
              <img class="thumb" src={a.file.thumbnailUrl ?? a.file.url} alt="" />
            {:else}
              <span class="thumb badge">{a.file.mimeType.split("/")[0]}</span>
            {/if}
          {:else if a.previewUrl}
            <img class="thumb" src={a.previewUrl} alt="" />
          {:else}
            <span class="thumb badge">{extLower(a.name).toUpperCase() || "FILE"}</span>
          {/if}
```

- [ ] **Step 7: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: エラー0件。

- [ ] **Step 8: Commit**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 投稿欄でクリップボード画像の貼り付けに対応"
```

---

### Task 5: 実機確認

**Files:**
- なし(手動確認のみ)

**Interfaces:**
- Consumes: Task 1〜4の全成果物
- Produces: なし(検証結果の確認のみ)

- [ ] **Step 1: dev起動**

Run: `cargo tauri dev`

- [ ] **Step 2: 通常のテキスト貼り付けが壊れていないことを確認**

任意のテキストをコピーし、投稿欄にフォーカスしてCtrl+V(Linux/Windows)またはCmd+V(macOS)を押す。
Expected: テキストがそのまま投稿欄に貼り付けられる(添付は増えない)。

- [ ] **Step 3: クリップボード画像の貼り付けを確認(アップロードされないこと)**

スクリーンショットツール(例: `gnome-screenshot -a`や`flameshot gui`のクリップボードコピー)で画像をコピーし、投稿欄にフォーカスしてCtrl+V/Cmd+Vを押す。
Expected: サムネイル欄に画像プレビューがすぐ表示される(ネットワーク待ちなし。まだドライブへアップロードされていない)。

- [ ] **Step 4: 投稿せずに閉じても孤児ファイルが残らないことを確認**

Step 3で画像を貼り付けた状態で、投稿せずに本文をクリアする(またはコンポーズ欄を閉じる)。Misskey側のドライブ(Web UIやドライブピッカー)を確認する。
Expected: ドライブに何もアップロードされていない。

- [ ] **Step 5: 投稿できることを確認**

改めてクリップボード画像を貼り付け、投稿を実行する。
Expected: 投稿時にアップロードが走り(サムネイルに「アップロード中」表示が出る)、ノートが作成され、添付画像が表示される。ドライブにもアップロード済みファイルが1件増えている。

- [ ] **Step 6: クリップボードが空/画像が無い場合の挙動を確認**

何もコピーしていない状態(または前回コピーが空)で投稿欄にフォーカスしてCtrl+V/Cmd+Vを押す。
Expected: 何も起きない(エラー表示もされない、添付も増えない)。

- [ ] **Step 7: PRの説明・チェックリストを更新**

PR #64の本文(Test plan)を実装済みの内容に合わせて更新する。

Run: `gh pr view 64 --repo onodai145/tsumugi` で現状のPR本文を確認してから、`gh pr edit 64 --repo onodai145/tsumugi --body "..."` で更新する(具体的な文面はTask 1〜5の完了後に実際の変更内容を反映して作成する)。
