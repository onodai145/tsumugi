# 添付ファイルのアップロードタイミングを投稿時に変える (Issue #66) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ファイル選択時ではなく投稿ボタン押下時にドライブへアップロードするよう `ComposeBar.svelte` を変更し、下書き破棄時の孤児ファイル発生を防ぐ。

**Architecture:** フロントの添付状態を `DriveFile[]` から `{kind:"local", ...} | {kind:"drive", file: DriveFile}` のUnion型 `AttachmentItem[]` に置き換える。ファイル選択時はローカルパスを保持するだけ（アップロードしない）。ローカル画像のプレビューは、新設する Rust コマンド `read_attachment_preview` で data URL(base64) に変換して表示する(`src-tauri/src/commands/mute.rs` の `read_image_data_url` と同じ既存パターンを再利用。Android の `content://` パスにも対応済み)。投稿ボタン押下時に `local` 項目を順にアップロードして `drive` に変換し、失敗時は投稿全体を中止してリトライ可能にする。

**Tech Stack:** Svelte 5 runes (`$state`/`$derived`), Rust (Tauri v2 command), `base64` crate(既存依存)、`tauri-specta` バインディング生成。

## Global Constraints

- 新規コマンドは `src-tauri/src/lib.rs` の `specta_builder()` 内 `collect_commands![...]` に登録すること(`tauri::Builder` だけへの登録は不可。CLAUDE.md参照)。
- バインディング再生成は `cd src-tauri && cargo test` (`generates_frontend_bindings` テストが `frontend/src/bindings/tauri.gen.ts` を再生成する)、または `cargo tauri dev` の起動時に自動実行される。**手で `tauri.gen.ts` を編集しないこと**。
- アプリの起動確認は `cargo tauri dev` のみを使う(`cargo run` / `./target/debug/tsumugi` は不可、devサーバー未起動で接続エラーになる)。
- コミットメッセージは1行のみ(件名だけ、本文なし)。

---

### Task 1: Rust — `read_file_as_data_url` を `pub(crate)` にし、添付プレビュー用コマンドを追加

**Files:**
- Modify: `src-tauri/src/commands/mute.rs:81`(関数を `pub(crate)` へ変更するのみ、ロジック変更なし)
- Modify: `src-tauri/src/commands/note.rs`(末尾に新規コマンドと mime 判定関数、テストを追加)
- Modify: `src-tauri/src/lib.rs:69`(`collect_commands!` に新規コマンドを追加)

**Interfaces:**
- Consumes: `crate::commands::mute::read_file_as_data_url(app: &AppHandle, path: &str, max_bytes: usize, guess_mime: fn(&str) -> &'static str) -> Result<String>`(既存関数、可視性のみ変更)
- Produces: `#[tauri::command] pub async fn read_attachment_preview(app: AppHandle, path: String) -> Result<String>` — フロントから `commands.readAttachmentPreview(path: string): Promise<Result<string, Error>>` として呼べるようになる。

- [ ] **Step 1: `read_file_as_data_url` を `pub(crate)` に変更する**

`src-tauri/src/commands/mute.rs:81` を変更:

```rust
async fn read_file_as_data_url(
```

を

```rust
pub(crate) async fn read_file_as_data_url(
```

に変更する。

- [ ] **Step 2: mime判定関数の失敗するテストを先に書く**

`src-tauri/src/commands/note.rs` の末尾に追記:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_attachment_image_mime_maps_known_extensions() {
        assert_eq!(guess_attachment_image_mime("photo.png"), "image/png");
        assert_eq!(guess_attachment_image_mime("photo.JPG"), "image/jpeg");
        assert_eq!(guess_attachment_image_mime("photo.jpeg"), "image/jpeg");
        assert_eq!(guess_attachment_image_mime("photo.gif"), "image/gif");
        assert_eq!(guess_attachment_image_mime("photo.webp"), "image/webp");
    }

    #[test]
    fn guess_attachment_image_mime_falls_back_for_unknown_or_video_extensions() {
        assert_eq!(guess_attachment_image_mime("clip.mp4"), "application/octet-stream");
        assert_eq!(guess_attachment_image_mime("clip.webm"), "application/octet-stream");
        assert_eq!(guess_attachment_image_mime("noext"), "application/octet-stream");
    }
}
```

- [ ] **Step 3: テストを実行し失敗を確認する**

Run: `cd src-tauri && cargo test guess_attachment_image_mime`
Expected: FAIL(`guess_attachment_image_mime` が未定義でコンパイルエラー)

- [ ] **Step 4: コマンドと mime判定関数を実装する**

`src-tauri/src/commands/note.rs` の先頭 `use` に `tauri::AppHandle` を追加(既存 `use tauri::State;` を変更):

```rust
use tauri::{AppHandle, State};
```

`upload_file` コマンドの直後(既存87-99行目の後)に追記:

```rust
/// プレビュー用途に許容する最大サイズ(base64化してフロントに保持するため、実アップロード上限より小さく抑える)。
const MAX_ATTACHMENT_PREVIEW_BYTES: usize = 20 * 1024 * 1024;

/// 投稿添付の未アップロードローカル画像を data URL(base64) に変換する(投稿前プレビュー用)。
/// 動画や未知拡張子は `application/octet-stream` を返す(呼び出し側でバッジ表示にフォールバックする想定)。
#[tauri::command]
#[specta::specta]
pub async fn read_attachment_preview(app: AppHandle, path: String) -> Result<String> {
    crate::commands::mute::read_file_as_data_url(
        &app,
        &path,
        MAX_ATTACHMENT_PREVIEW_BYTES,
        guess_attachment_image_mime,
    )
    .await
}

/// 拡張子から画像 MIME を推定する。動画・未知拡張子は octet-stream(呼び出し側でバッジ表示にフォールバック)。
fn guess_attachment_image_mime(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}
```

- [ ] **Step 5: テストを実行し成功を確認する**

Run: `cd src-tauri && cargo test guess_attachment_image_mime`
Expected: PASS(2 tests)

- [ ] **Step 6: コマンドを `specta_builder()` に登録する**

`src-tauri/src/lib.rs:69` の `commands::note::save_url_to_file,` の直後に追記:

```rust
            commands::note::save_url_to_file,
            commands::note::read_attachment_preview,
```

- [ ] **Step 7: ビルドとフルテストを実行し、バインディングが再生成されることを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。`generates_frontend_bindings` テストが通り、`frontend/src/bindings/tauri.gen.ts` に `readAttachmentPreview` が生成される。

Run: `grep -n "readAttachmentPreview" ../frontend/src/bindings/tauri.gen.ts`
Expected: `readAttachmentPreview: (path: string) => typedError<string, Error>(__TAURI_INVOKE("read_attachment_preview", { path })),` のような行がヒットする。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/src/commands/mute.rs src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: 添付ファイルのローカルプレビュー用コマンドread_attachment_previewを追加"
```

---

### Task 2: Frontend — `ComposeBar.svelte` の添付データモデルとファイル選択処理を変更

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: `commands.readAttachmentPreview(path: string): Promise<Result<string, Error>>`(Task 1で追加)、`commands.uploadFile(accountId: string, path: string): Promise<Result<DriveFile, Error>>`(既存)
- Produces: `type AttachmentItem = { kind: "local"; id: string; path: string; name: string; previewUrl: string | null } | { kind: "drive"; id: string; file: DriveFile }` — Task 3の `submit()`/テンプレートが参照する型。

- [ ] **Step 1: `attached` を `attachments: AttachmentItem[]` に置き換える**

`frontend/src/ui/ComposeBar.svelte:10-15` の import 型リストに変更なし(`DriveFile` は引き続き使う)。

`frontend/src/ui/ComposeBar.svelte:54` を置き換え:

```ts
  type AttachmentItem =
    | { kind: "local"; id: string; path: string; name: string; previewUrl: string | null }
    | { kind: "drive"; id: string; file: DriveFile };

  const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp"]);

  function extLower(name: string): string {
    const i = name.lastIndexOf(".");
    return i >= 0 ? name.slice(i + 1).toLowerCase() : "";
  }

  let attachments = $state<AttachmentItem[]>([]);
```

- [ ] **Step 2: `onDriveFilesSelected` を新データモデルに合わせる**

`frontend/src/ui/ComposeBar.svelte:85-88` を置き換え:

```ts
  function onDriveFilesSelected(picked: DriveFile[]) {
    const known = new Set(
      attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
    );
    const additions: AttachmentItem[] = picked
      .filter((f) => !known.has(f.id))
      .map((f) => ({ kind: "drive", id: f.id, file: f }));
    attachments = [...attachments, ...additions];
  }
```

- [ ] **Step 3: `uploading` boolean を削除し `pickFiles()` を非アップロード化する**

`frontend/src/ui/ComposeBar.svelte:89`(`let uploading = $state(false);`)を削除する。

`frontend/src/ui/ComposeBar.svelte:145-163` の `pickFiles()` を置き換え:

```ts
  async function pickFiles() {
    err = null;
    const picked = await open({
      multiple: true,
      filters: [{ name: "画像/動画", extensions: ["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm"] }],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    for (const p of paths) {
      const name = p.split(/[\\/]/).pop() ?? p;
      let previewUrl: string | null = null;
      if (IMAGE_EXTENSIONS.has(extLower(name))) {
        try {
          previewUrl = await unwrap(commands.readAttachmentPreview(p));
        } catch {
          previewUrl = null;
        }
      }
      attachments = [...attachments, { kind: "local", id: crypto.randomUUID(), path: p, name, previewUrl }];
    }
  }
```

- [ ] **Step 4: `removeAttached` を id ベースの新データモデルに合わせる**

`frontend/src/ui/ComposeBar.svelte:165-167` を置き換え:

```ts
  function removeAttached(id: string) {
    attachments = attachments.filter((a) => a.id !== id);
  }
```

- [ ] **Step 5: `pnpm check` を実行し、この時点でのエラーが `submit()`/テンプレート側の `attached` 未定義エラーのみであることを確認する**

Run: `cd frontend && pnpm check`
Expected: `attached` を参照している `submit()` とテンプレート部分でエラーが出る(Task 3で解消する)。ここでは `attachments`/`AttachmentItem` 自体に型エラーが無いことだけ確認する。

- [ ] **Step 6: コミット**

まだビルドが通らないため、Task 3 とまとめてコミットする(このステップはスキップし、Task 3 の最後で一括コミット)。

---

### Task 3: Frontend — `submit()` とテンプレートを新データモデルに合わせる

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: Task 2 で定義した `AttachmentItem`、`attachments` state
- Produces: 投稿ボタン押下で `local` 項目を順にアップロードしてから `postNote` する `submit()`。失敗時は `failedAttachmentId` state で該当サムネイルにエラー表示する。

- [ ] **Step 1: アップロード進捗・失敗を表すstateを追加する**

`frontend/src/ui/ComposeBar.svelte:90`(`let busy = $state(false);`)の直後に追記:

```ts
  let uploadingAttachmentId = $state<string | null>(null);
  let failedAttachmentId = $state<string | null>(null);
```

- [ ] **Step 2: `compact` の判定を `attachments` に合わせる**

`frontend/src/ui/ComposeBar.svelte:103`(`attached.length === 0 &&`)を:

```ts
      attachments.length === 0 &&
```

に変更する。

- [ ] **Step 3: `submit()` を新データモデル対応にする**

`frontend/src/ui/ComposeBar.svelte:169-216` の `submit()` を置き換え:

```ts
  async function submit() {
    err = null;
    if (!accountId) {
      err = "アカウントを選択してください";
      return;
    }
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !quoteOf && choices.length === 0 && attachments.length === 0) return;
    let expiresAt: number | null = null;
    if (pollExpiryMode === "at" && pollExpiresAt) {
      expiresAt = new Date(pollExpiresAt).getTime();
    } else if (pollExpiryMode === "after") {
      expiresAt = Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    }

    busy = true;
    failedAttachmentId = null;
    try {
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

      const draft: NoteDraft = {
        text: text.trim() || null,
        cw: useCw && cw.trim() ? cw.trim() : null,
        visibility,
        fileIds: attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
        poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt } : null,
        replyId: replyTo?.id ?? null,
        renoteId: quoteOf?.id ?? null,
        localOnly,
      };
      await app.postNote(accountId, draft);
      text = "";
      cw = "";
      useCw = false;
      usePoll = false;
      pollChoices = ["", ""];
      pollMultiple = false;
      pollExpiryMode = "none";
      pollExpiresAt = "";
      pollAfterAmount = 1;
      pollAfterUnit = "hour";
      localOnly = false;
      attachments = [];
      replyTo = undefined;
      quoteOf = undefined;
      onPosted?.();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
      uploadingAttachmentId = null;
    }
  }
```

- [ ] **Step 4: サムネイル一覧テンプレートを新データモデル対応にする**

`frontend/src/ui/ComposeBar.svelte:269-283` を置き換え:

```svelte
  {#if attachments.length > 0}
    <div class="thumbs">
      {#each attachments as a (a.id)}
        <div class="thumb-wrap">
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
          {#if uploadingAttachmentId === a.id}
            <span class="thumb-status" title="アップロード中">…</span>
          {:else if failedAttachmentId === a.id}
            <span class="thumb-status error" title={err ?? "アップロードに失敗しました"}>!</span>
          {/if}
          <button class="thumb-x" title="削除" onclick={() => removeAttached(a.id)}><X size={10} /></button>
        </div>
      {/each}
    </div>
  {/if}
```

- [ ] **Step 5: 添付ボタンの `disabled` 条件を `uploading` から `busy` に変える**

`frontend/src/ui/ComposeBar.svelte:346`(`disabled={uploading}`)を:

```svelte
        disabled={busy}
```

に変更する。

- [ ] **Step 6: `.thumb-status` のスタイルを追加する**

`frontend/src/ui/ComposeBar.svelte` の `<style>` ブロック内、`.thumb-x` ルール(現行570-585行目付近)の直後に追記:

```css
  .thumb-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    bottom: -4px;
    left: -4px;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    border-radius: 50%;
    width: 14px;
    height: 14px;
    font-size: 0.6rem;
  }
  .thumb-status.error {
    background: #ef4444;
  }
```

- [ ] **Step 7: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー 0件(`attached` を参照する箇所がすべて `attachments` に置き換わっている)。

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 添付ファイルのアップロードを投稿時まで遅延させる"
```

---

### Task 4: 手動動作確認

**Files:** なし(コード変更なし、動作確認のみ)

**Interfaces:**
- Consumes: Task 1〜3 で実装した一連の変更

- [ ] **Step 1: dev起動**

Run: `cargo tauri dev`
Expected: アプリが起動し、コンパイルエラーが無い。

- [ ] **Step 2: ローカル画像複数選択時、送信前はアップロードされないことを確認する**

操作: コンポーズ欄の「画像を添付」→「ローカルから選択」でPNG/JPEG画像を2枚選択する。
Expected: 選択直後にサムネイル(実画像プレビュー)が2つ表示される。この時点でMisskeyサーバのドライブに何もアップロードされていないこと(Misskeyの設定画面等のドライブ一覧を別途確認するか、devtoolsのネットワークタブで `drive/files/create` への通信が発生していないことを確認する)。

- [ ] **Step 3: 投稿時に順にアップロードされノートが作成されることを確認する**

操作: 本文を入力し「投稿」ボタンを押す。
Expected: サムネイルに「アップロード中…」バッジが順番に表示され、消えた後ノートが作成され、コンポーズ欄がクリアされる。

- [ ] **Step 4: アップロード失敗時に投稿が中止されリトライできることを確認する**

操作: ネットワークを切断する、またはOSの権限で一時的に読めないファイルを選ぶなどしてアップロードを意図的に失敗させ、投稿を実行する。
Expected: 失敗したサムネイルに赤い「!」バッジが表示され、投稿は中止される(ノートは作成されない)。ネットワーク復旧後に再度「投稿」を押すと、正常に投稿が完了する。

- [ ] **Step 5: 添付したまま投稿せずに閉じてもドライブにアップロードされないことを確認する**

操作: ローカル画像を添付した状態で、投稿せずにコンポーズ欄を閉じる(返信キャンセルや画面遷移など、`attachments` state が破棄される操作)。
Expected: ドライブに何もアップロードされていないこと(本Issueの主目的が満たされていることの最終確認)。

- [ ] **Step 6: 動画ファイルの添付が拡張子バッジ表示のままアップロード・投稿できることを確認する**

操作: mp4/webmファイルを選択して投稿する。
Expected: 選択直後は「MP4」等の拡張子バッジが表示され(画像プレビューはしない、想定通り)、投稿実行で正常にアップロード・投稿される。
