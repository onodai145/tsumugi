# ドライブ添付ピッカー Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ComposeBar の画像添付ボタンから、ローカルアップロードに加えて Misskey ドライブ上の既存ファイルをフォルダ移動しながら複数選択して添付できるようにする（Issue #13）。

**Architecture:** Rust側に `drive/files` / `drive/folders` REST 呼び出しを追加し（`MisskeyClient::post` を使った薄いラッパ、既存 `api/meta.rs` と同じパターン）、`#[tauri::command]` として公開する。フロントは新規 `DrivePicker.svelte` モーダルでフォルダナビゲーション・複数選択・ページングを行い、選択結果を `ComposeBar.svelte` の既存 `attached` 配列にそのまま連結する（既にドライブ上のファイルなので再アップロード不要）。

**Tech Stack:** Rust（Tauri v2, reqwest, serde, specta）/ Svelte 5（runes: `$state`, `$derived`）/ `@lucide/svelte` アイコン。

## Global Constraints

- ファイル種別フィルタは一切かけない（Misskey の添付に種別制限は無いため、ドライブの全ファイルを一覧表示する）。
- フォルダ階層ナビゲーション対応（ルート→フォルダに入る→パンくずで戻る）が必須。
- 複数ファイル選択 + 「もっと見る」ボタンでの追加読み込み（`untilId` ページング、1ページ30件）。
- 起動導線は既存 `ImagePlus` ボタンにポップオーバーを付与する形（新規ボタンは追加しない）。
- センシティブファイル（`isSensitive`）は `MediaGrid.svelte` と同じ「閲覧注意（クリックで表示）」カバーで隠す。
- ドライブへの新規アップロード・フォルダ作成・ファイル削除は対象外。
- 新規コマンドは `src-tauri/src/lib.rs` の `specta_builder()` に登録し、TSバインディングを再生成すること。

---

### Task 1: Rust API層 — ドライブ一覧・フォルダ一覧の取得関数

**Files:**
- Modify: `src-tauri/src/api/drive.rs`

**Interfaces:**
- Consumes: `crate::api::MisskeyClient::post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>`（既存、`src-tauri/src/api/client.rs`）。`crate::api::normalize::RawFile`（既存、`From<RawFile> for DriveFile` も既存）。`crate::domain::SourceItem { id: String, name: String }`（既存、`src-tauri/src/domain/list.rs`、フォルダは id+name の軽量参照として流用する）。
- Produces: `pub async fn list_files(client: &MisskeyClient, folder_id: Option<&str>, until_id: Option<&str>) -> Result<Vec<DriveFile>>` と `pub async fn list_folders(client: &MisskeyClient, folder_id: Option<&str>) -> Result<Vec<SourceItem>>`。Task 2 がこの2関数を呼ぶ。

- [ ] **Step 1: 現在の `drive.rs` の内容を確認する**

Run: `cat src-tauri/src/api/drive.rs`

既存は `upload_file`（multipart、生 `reqwest::Client` + host + token を直接使う）のみであることを確認する。今回追加する2関数は通常の JSON POST なので、`upload_file` とは別経路の `MisskeyClient::post`（token 埋め込み・エラーマッピング済み）を使う。

- [ ] **Step 2: import を更新する**

`src-tauri/src/api/drive.rs` 冒頭の import を以下に置き換える（既存の4行を置き換え）:

```rust
use crate::api::normalize::RawFile;
use crate::api::MisskeyClient;
use crate::domain::{DriveFile, SourceItem};
use crate::error::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
```

- [ ] **Step 3: リクエストボディを組み立てる純粋関数とファイル一覧取得関数を追記する**

`src-tauri/src/api/drive.rs` の末尾（`upload_file` 関数の後）に追記:

```rust
/// 1ページあたりの取得件数。フロント側の「もっと見る」判定（返却件数がこの値未満なら
/// 終端とみなす）と一致させる。
const DRIVE_LIST_LIMIT: u8 = 30;

fn list_files_body(folder_id: Option<&str>, until_id: Option<&str>) -> Value {
    let mut body = json!({ "limit": DRIVE_LIST_LIMIT, "folderId": folder_id });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    body
}

/// ドライブのファイル一覧。`folder_id: None` はルート直下、`until_id` はページング
/// （直前に取得した最後のファイルIDを渡す）。種別フィルタはかけない
/// （Misskey の投稿添付に種別制限は無いため）。
pub async fn list_files(
    client: &MisskeyClient,
    folder_id: Option<&str>,
    until_id: Option<&str>,
) -> Result<Vec<DriveFile>> {
    let raw: Vec<RawFile> = client
        .post("drive/files", &list_files_body(folder_id, until_id))
        .await?;
    Ok(raw.into_iter().map(DriveFile::from).collect())
}

fn list_folders_body(folder_id: Option<&str>) -> Value {
    json!({ "limit": 100, "folderId": folder_id })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFolder {
    id: String,
    #[serde(default)]
    name: String,
}

/// 指定フォルダ直下のサブフォルダ一覧（`folder_id: None` はルート直下）。
pub async fn list_folders(client: &MisskeyClient, folder_id: Option<&str>) -> Result<Vec<SourceItem>> {
    let raw: Vec<RawFolder> = client
        .post("drive/folders", &list_folders_body(folder_id))
        .await?;
    Ok(raw
        .into_iter()
        .map(|f| SourceItem { id: f.id, name: f.name })
        .collect())
}
```

- [ ] **Step 4: リクエストボディ組み立ての単体テストを書く**

`src-tauri/src/api/drive.rs` の末尾に追記（ネットワーク接続不要、`notes.rs` の `rest_request_builds_bodies` と同じ「純粋関数のボディ組み立てを検証する」パターン）:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_files_body_root_has_null_folder_id_and_no_until_id() {
        let body = list_files_body(None, None);
        assert_eq!(body["limit"], 30);
        assert_eq!(body["folderId"], Value::Null);
        assert!(body.get("untilId").is_none());
    }

    #[test]
    fn list_files_body_includes_folder_and_until_id_when_present() {
        let body = list_files_body(Some("f1"), Some("n9"));
        assert_eq!(body["folderId"], "f1");
        assert_eq!(body["untilId"], "n9");
    }

    #[test]
    fn list_folders_body_root_has_null_folder_id() {
        let body = list_folders_body(None);
        assert_eq!(body["limit"], 100);
        assert_eq!(body["folderId"], Value::Null);
    }

    #[test]
    fn list_folders_body_includes_folder_id_when_present() {
        let body = list_folders_body(Some("f1"));
        assert_eq!(body["folderId"], "f1");
    }
}
```

- [ ] **Step 5: テストを実行して通ることを確認する**

Run: `cd src-tauri && cargo test --lib api::drive:: -- --nocapture`
Expected: `test api::drive::tests::list_files_body_root_has_null_folder_id_and_no_until_id ... ok` を含む4件すべて `ok`。

- [ ] **Step 6: commit**

```bash
git add src-tauri/src/api/drive.rs
git commit -m "feat: ドライブのファイル/フォルダ一覧取得APIを追加"
```

---

### Task 2: Tauri コマンド公開とTSバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 の `crate::api::drive::list_files` / `crate::api::drive::list_folders`。既存 `AppState::client_for(&self, account_id: &str) -> Result<MisskeyClient>`（`src-tauri/src/state.rs`、`post_note` 等で使用済み）。
- Produces: `#[tauri::command] list_drive_files(state, account_id: String, folder_id: Option<String>, until_id: Option<String>) -> Result<Vec<DriveFile>>` と `#[tauri::command] list_drive_folders(state, account_id: String, folder_id: Option<String>) -> Result<Vec<SourceItem>>`。フロント（Task 3/4）はこれを `commands.listDriveFiles(accountId, folderId, untilId)` / `commands.listDriveFolders(accountId, folderId)` として呼ぶ（`tauri-specta` の camelCase 変換）。

- [ ] **Step 1: `commands/note.rs` の import を更新する**

`src-tauri/src/commands/note.rs` の1行目の import ブロックを編集する。

置き換え前:
```rust
use crate::api::drive::upload_file as api_upload_file;
```
置き換え後:
```rust
use crate::api::drive::{list_files as api_list_files, list_folders as api_list_folders, upload_file as api_upload_file};
```

同ファイルの以下の行:
```rust
use crate::domain::{DriveFile, EmojiDef, Note};
```
を以下に置き換える:
```rust
use crate::domain::{DriveFile, EmojiDef, Note, SourceItem};
```

- [ ] **Step 2: コマンド2つを追加する**

`src-tauri/src/commands/note.rs` の `upload_file` 関数の直後（`save_url_to_file` の手前）に挿入:

```rust
/// ドライブのファイル一覧（添付ピッカー用）。folder_id: None はルート直下、
/// until_id は直前に取得した最後のファイルIDを渡してページングする。
#[tauri::command]
#[specta::specta]
pub async fn list_drive_files(
    state: State<'_, AppState>,
    account_id: String,
    folder_id: Option<String>,
    until_id: Option<String>,
) -> Result<Vec<DriveFile>> {
    let client = state.client_for(&account_id)?;
    api_list_files(&client, folder_id.as_deref(), until_id.as_deref()).await
}

/// ドライブのフォルダ一覧（添付ピッカーのフォルダナビゲーション用）。
/// folder_id: None はルート直下のフォルダ一覧。
#[tauri::command]
#[specta::specta]
pub async fn list_drive_folders(
    state: State<'_, AppState>,
    account_id: String,
    folder_id: Option<String>,
) -> Result<Vec<SourceItem>> {
    let client = state.client_for(&account_id)?;
    api_list_folders(&client, folder_id.as_deref()).await
}
```

- [ ] **Step 3: `specta_builder()` にコマンドを登録する**

`src-tauri/src/lib.rs` の以下の行:
```rust
            commands::note::upload_file,
            commands::note::save_url_to_file,
```
を以下に置き換える:
```rust
            commands::note::upload_file,
            commands::note::list_drive_files,
            commands::note::list_drive_folders,
            commands::note::save_url_to_file,
```

- [ ] **Step 4: コンパイルとTSバインディング再生成を確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings -- --nocapture`
Expected: `test generates_frontend_bindings ... ok`（このテストが `frontend/src/bindings/tauri.gen.ts` を再生成する）。

- [ ] **Step 5: 生成されたバインディングに新コマンドが camelCase で入っていることを確認する**

Run: `grep -n "listDriveFiles\|listDriveFolders" frontend/src/bindings/tauri.gen.ts`
Expected: `listDriveFiles` と `listDriveFolders` の両方がヒットする（`commands` オブジェクトの関数として生成されている）。

- [ ] **Step 6: Rust テストスイート全体が通ることを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テスト `ok`（`#[ignore]` の実接続テストはスキップされる）。

- [ ] **Step 7: commit**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: ドライブ一覧取得コマンドを公開しTSバインディングを再生成"
```

---

### Task 3: `DrivePicker.svelte` — フォルダナビゲーション付き複数選択モーダル

**Files:**
- Create: `frontend/src/ui/DrivePicker.svelte`

**Interfaces:**
- Consumes: `commands.listDriveFiles(accountId: string, folderId: string | null, untilId: string | null): Promise<Result<DriveFile[]>>`、`commands.listDriveFolders(accountId: string, folderId: string | null): Promise<Result<SourceItem[]>>`（Task 2、`../lib/ipc` の `commands` / `unwrap` 経由）。型 `DriveFile`, `SourceItem` は `../bindings/tauri.gen`。
- Produces: コンポーネント props `{ accountId: string; onSelect: (files: DriveFile[]) => void; onclose: () => void }`。Task 4（`ComposeBar.svelte`）がこの props で使用する。

- [ ] **Step 1: 参考にする既存モーダルとセンシティブカバーの実装を確認する**

Run: `sed -n '1,30p;337,360p' frontend/src/ui/AddColumnModal.svelte` （オーバーレイ＋モーダルの構造）
Run: `sed -n '70,80p' frontend/src/render/MediaGrid.svelte` （`isSensitive` カバーの見た目パターン）

- [ ] **Step 2: `DrivePicker.svelte` を新規作成する**

`frontend/src/ui/DrivePicker.svelte` を作成:

```svelte
<script lang="ts">
  import { commands, unwrap } from "../lib/ipc";
  import { X, Check } from "@lucide/svelte";
  import type { DriveFile, SourceItem } from "../bindings/tauri.gen";

  let {
    accountId,
    onSelect,
    onclose,
  }: { accountId: string; onSelect: (files: DriveFile[]) => void; onclose: () => void } = $props();

  // バックエンド側の1ページ件数(DRIVE_LIST_LIMIT)と一致させる。返却件数がこれ未満なら終端。
  const FILES_LIMIT = 30;

  // ルートからの経路(パンくず)。空配列 = ルート。末尾が現在のフォルダ。
  let path = $state<SourceItem[]>([]);
  const currentFolderId = $derived(path.length > 0 ? path[path.length - 1].id : null);

  let folders = $state<SourceItem[]>([]);
  let files = $state<DriveFile[]>([]);
  let selected = $state<Map<string, DriveFile>>(new Map());
  let revealed = $state<Record<string, boolean>>({});
  let loading = $state(false);
  let loadingMore = $state(false);
  let noMoreFiles = $state(false);
  let err = $state<string | null>(null);

  async function refresh() {
    err = null;
    loading = true;
    noMoreFiles = false;
    try {
      const [f, fl] = await Promise.all([
        unwrap(commands.listDriveFolders(accountId, currentFolderId)),
        unwrap(commands.listDriveFiles(accountId, currentFolderId, null)),
      ]);
      folders = f;
      files = fl;
      if (fl.length < FILES_LIMIT) noMoreFiles = true;
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (files.length === 0 || loadingMore || noMoreFiles) return;
    loadingMore = true;
    err = null;
    try {
      const older = await unwrap(
        commands.listDriveFiles(accountId, currentFolderId, files[files.length - 1].id),
      );
      files = [...files, ...older];
      if (older.length < FILES_LIMIT) noMoreFiles = true;
    } catch (e) {
      err = String(e);
    } finally {
      loadingMore = false;
    }
  }

  function enterFolder(folder: SourceItem) {
    path = [...path, folder];
    void refresh();
  }

  // index === -1 でルートへ。それ以外は path[0..=index] まで残す。
  function goToBreadcrumb(index: number) {
    path = index < 0 ? [] : path.slice(0, index + 1);
    void refresh();
  }

  function toggleSelect(f: DriveFile) {
    const next = new Map(selected);
    if (next.has(f.id)) next.delete(f.id);
    else next.set(f.id, f);
    selected = next;
  }

  function onFileClick(f: DriveFile) {
    if (f.isSensitive && !revealed[f.id]) {
      revealed = { ...revealed, [f.id]: true };
      return;
    }
    toggleSelect(f);
  }

  function confirm() {
    onSelect([...selected.values()]);
    onclose();
  }

  void refresh();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>ドライブから選択</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>

    <nav class="breadcrumb">
      <button class="crumb" class:active={path.length === 0} onclick={() => goToBreadcrumb(-1)}>ドライブ</button>
      {#each path as p, i (p.id)}
        <span class="sep">/</span>
        <button class="crumb" class:active={i === path.length - 1} onclick={() => goToBreadcrumb(i)}>
          {p.name || "(無題)"}
        </button>
      {/each}
    </nav>

    {#if loading}
      <p class="hint">読み込み中…</p>
    {:else}
      {#if folders.length === 0 && files.length === 0}
        <p class="hint">ファイルがありません</p>
      {/if}
      <div class="grid">
        {#each folders as f (f.id)}
          <button class="cell folder" onclick={() => enterFolder(f)}>📁 {f.name || "(無題)"}</button>
        {/each}
        {#each files as f (f.id)}
          <button class="cell file" class:selected={selected.has(f.id)} onclick={() => onFileClick(f)}>
            {#if f.isSensitive && !revealed[f.id]}
              <span class="sensitive-cover">閲覧注意（クリックで表示）</span>
            {:else if f.mimeType.startsWith("image/")}
              <img src={f.thumbnailUrl ?? f.url} alt={f.name} loading="lazy" />
            {:else}
              <span class="badge">{f.mimeType.split("/")[0] || "file"}</span>
            {/if}
            {#if selected.has(f.id)}
              <span class="check"><Check size={14} /></span>
            {/if}
          </button>
        {/each}
      </div>
      {#if !noMoreFiles && files.length > 0}
        <button class="mini more" disabled={loadingMore} onclick={loadMore}>
          {loadingMore ? "読み込み中…" : "もっと見る"}
        </button>
      {/if}
    {/if}

    {#if err}<p class="err">{err}</p>{/if}

    <div class="actions">
      <span class="count">選択中 {selected.size}件</span>
      <button class="submit" disabled={selected.size === 0} onclick={confirm}>添付</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 60;
  }
  .modal {
    width: min(520px, 92vw);
    max-height: 78vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 10px;
    flex: none;
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    margin-bottom: 10px;
    flex: none;
  }
  .crumb {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px 4px;
    font: inherit;
  }
  .crumb.active {
    color: var(--text);
    font-weight: 600;
  }
  .sep {
    color: var(--text-dim);
  }
  .hint {
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 8px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .cell {
    position: relative;
    aspect-ratio: 1;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    overflow: hidden;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
  }
  .cell.folder {
    font-size: 0.72rem;
    padding: 6px;
    word-break: break-all;
  }
  .cell.file img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cell.file.selected {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .sensitive-cover {
    padding: 4px;
    color: var(--text-dim);
  }
  .badge {
    color: var(--text-dim);
  }
  .check {
    position: absolute;
    top: 4px;
    right: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
  }
  .mini.more {
    margin-top: 8px;
    align-self: center;
    padding: 5px 14px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
    flex: none;
  }
  .mini.more:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 12px;
    flex: none;
  }
  .count {
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .submit {
    border: none;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    border-radius: 8px;
    padding: 7px 20px;
    cursor: pointer;
  }
  .submit:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
    word-break: break-word;
    flex: none;
  }
</style>
```

- [ ] **Step 3: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー0件（`DrivePicker.svelte` に関する型エラーが出ないこと）。

- [ ] **Step 4: commit**

```bash
git add frontend/src/ui/DrivePicker.svelte
git commit -m "feat: ドライブ添付ピッカーのモーダルコンポーネントを追加"
```

---

### Task 4: `ComposeBar.svelte` への統合

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: Task 3 の `DrivePicker` コンポーネント（props: `accountId`, `onSelect`, `onclose`）。
- Produces: なし（末端UI）。

- [ ] **Step 1: `DrivePicker` の import を追加する**

`frontend/src/ui/ComposeBar.svelte` の既存 import ブロックに追加:

```svelte
  import DrivePicker from "./DrivePicker.svelte";
```

- [ ] **Step 2: ポップオーバーとドライブピッカー用の state / 関数を追加する**

`let attached = $state<DriveFile[]>([]);` の直後に追記:

```svelte
  let attachTrigger = $state<HTMLElement | undefined>(undefined);
  let showAttachMenu = $state(false);
  let attachMenuPos = $state<{ left: number; top: number } | null>(null);
  let showDrivePicker = $state(false);

  function toggleAttachMenu() {
    if (showAttachMenu) {
      showAttachMenu = false;
      return;
    }
    const r = attachTrigger?.getBoundingClientRect();
    if (r) attachMenuPos = { left: r.left, top: r.bottom + 4 };
    showAttachMenu = true;
  }

  function attachPortal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  async function chooseLocalUpload() {
    showAttachMenu = false;
    await pickFiles();
  }

  function chooseDrivePicker() {
    showAttachMenu = false;
    showDrivePicker = true;
  }

  function onDriveFilesSelected(picked: DriveFile[]) {
    const known = new Set(attached.map((f) => f.id));
    attached = [...attached, ...picked.filter((f) => !known.has(f.id))];
  }
```

- [ ] **Step 3: ツールバーの `ImagePlus` ボタンをポップオーバートリガーに変更する**

`frontend/src/ui/ComposeBar.svelte` の以下の行:
```svelte
      <button class="icon" title="画像を添付" onclick={pickFiles} disabled={uploading}><ImagePlus size={16} /></button>
```
を以下に置き換える:
```svelte
      <button
        class="icon"
        title="画像を添付"
        bind:this={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={uploading}
      ><ImagePlus size={16} /></button>
```

- [ ] **Step 4: ポップオーバーメニューと `DrivePicker` の描画を追加する**

`</div>`（`.composewrap` を閉じる最後のタグ、`</div>` の直前、テンプレート末尾）の直前に追記:

```svelte
{#if showAttachMenu && attachMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="attach-overlay" use:attachPortal onclick={() => (showAttachMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="attach-menu"
      style={`left:${attachMenuPos.left}px;top:${attachMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
    >
      <button class="attach-item" type="button" onclick={chooseLocalUpload}>ローカルから選択</button>
      <button
        class="attach-item"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseDrivePicker}
      >ドライブから選択</button>
    </div>
  </div>
{/if}

{#if showDrivePicker && accountId}
  <DrivePicker {accountId} onSelect={onDriveFilesSelected} onclose={() => (showDrivePicker = false)} />
{/if}
```

具体的な挿入位置を確認するため、まず現在のテンプレート末尾を確認する:

Run: `tail -8 frontend/src/ui/ComposeBar.svelte`

出力は次の形になっている（`.composewrap` を閉じる最後の `</div>` の後、空行を挟んで `<style>` が続く）:

```
  </div>
  </div>
</div>

<style>
```

一番外側の `</div>`（`.composewrap` を閉じるタグ、3行のうち最後のもの）の直後・空行の前に、上記の `{#if showAttachMenu ...}` ブロックと `{#if showDrivePicker ...}` ブロックを挿入する。

- [ ] **Step 5: ポップオーバーのスタイルを追加する**

`frontend/src/ui/ComposeBar.svelte` の `<style>` ブロック末尾（`.err { ... }` の後）に追記:

```css
  .attach-overlay {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .attach-menu {
    position: fixed;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
    min-width: 160px;
  }
  .attach-item {
    display: block;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 0.82rem;
  }
  .attach-item:hover {
    background: var(--surface-2);
  }
  .attach-item:disabled {
    opacity: 0.5;
    cursor: default;
  }
```

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー0件。

- [ ] **Step 7: commit**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 画像添付ボタンからドライブピッカーを開けるようにする"
```

---

### Task 5: 手動動作確認

**Files:** なし（動作確認のみ）

**Interfaces:**
- Consumes: Task 1〜4 の全成果物。
- Produces: なし。

- [ ] **Step 1: `cargo tauri dev` を起動する**

Run: `cargo tauri dev`（フォアグラウンドで起動したままにする。`cargo run` や `./target/debug/tsumugi` は使わない — devUrl 経由でないと接続エラーになるため）

- [ ] **Step 2: 画像添付ボタンの新しいメニューを確認する**

ComposeBar の画像添付アイコン（`ImagePlus`）をクリックし、「ローカルから選択」「ドライブから選択」の2項目メニューが出ることを確認する。

- [ ] **Step 3: ドライブピッカーの基本動作を確認する**

「ドライブから選択」をクリックし、モーダルが開くこと、フォルダがあれば📁アイコンでクリックして中に入れること、パンくずで「ドライブ」に戻ってルートへ戻れることを確認する。

- [ ] **Step 4: 複数選択とページングを確認する**

複数のファイルをクリックして選択状態（チェックマーク表示）になること、ドライブに30件超のファイルがあれば「もっと見る」で追加読み込みされることを確認する（無ければこの項目はスキップして良い）。

- [ ] **Step 5: センシティブファイルのカバーを確認する**

`isSensitive` なファイルがドライブにあれば、初回クリックで「閲覧注意（クリックで表示）」が解除され、再クリックで選択状態になることを確認する（無ければスキップして良い）。

- [ ] **Step 6: 添付結果がComposeBarに反映されることを確認する**

「添付」ボタンを押すとモーダルが閉じ、ComposeBar 下部のサムネイル一覧に選択したファイルが追加されていること、そのまま投稿できることを確認する。

- [ ] **Step 7: `cargo tauri dev` を終了する**

Ctrl+C で終了する。
