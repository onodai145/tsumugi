# クリップ / お気に入り Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートに対する「お気に入り登録/解除」(Issue #15)と「クリップへの追加」(Issue #14)を、NoteCard の新設 `⋯` メニューから実行できるようにする。

**Architecture:** Rust 側は既存の `api/notes.rs`(リアクション実装と同じ形)に favorite の2関数を追加し、クリップ用に新設する `api/clips.rs` / `domain/clip.rs` / `commands/clip.rs` を追加する。フロントは `store.svelte.ts` に楽観的更新込みの3メソッド(`toggleFavorite`/`listClips`+`createClip`+`addNoteToClip`)を追加し、`NoteCard.svelte` に `⋯` ボタンと新設 `NoteMenu.svelte`(ReactionPicker と同じ portal オーバーレイパターン)を組み込む。

**Tech Stack:** Rust (Tauri v2, specta/tauri-specta, reqwest), Svelte 5 (runes), `@lucide/svelte` アイコン。

## Global Constraints

- Misskey REST は全 POST・JSON ボディに `i`(token)同梱。`MisskeyClient::post` を通すこと(手書き reqwest 呼び出しを増やさない)。
- 新規 command は必ず `commands/mod.rs` の re-export と `lib.rs` の `specta_builder()` の両方に登録すること(片方だけだと TS バインディングが生成されない、または invoke ハンドラに載らない)。
- `domain/` の新規型には `specta::Type` を derive すること(TS export 対象)。
- コミットメッセージは既存スタイル(prefix: feat/fix/docs 等 + 日本語の一文、本文なし)に合わせる。
- クリップの `isPublic`/`description` は今回 UI から渡さない(Misskey 側デフォルト = 非公開・説明なし)。
- お気に入りの起動時バックフィルは行わない(`is_favorited_by_me` は楽観的更新でのみ変化させる)。

---

### Task 1: `domain::Clip` 型を追加

**Files:**
- Create: `src-tauri/src/domain/clip.rs`
- Modify: `src-tauri/src/domain/mod.rs`

**Interfaces:**
- Produces: `pub struct Clip { pub id: String, pub name: String, pub description: Option<String>, pub is_public: bool, pub notes_count: i64 }`(`specta::Type` 付き、TS へは camelCase で export)

- [ ] **Step 1: `domain/clip.rs` を作成**

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// Misskey のクリップ（名前付きノート集合）。今回のスコープでは一覧表示と
/// ノート追加にしか使わないため、フィールドは最小限。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_public: bool,
    pub notes_count: i64,
}
```

- [ ] **Step 2: `domain/mod.rs` に登録**

`src-tauri/src/domain/mod.rs` の `mod` 宣言群に `mod clip;` を追加し、`pub use` 群に `pub use clip::Clip;` を追加する。

編集後の該当箇所（アルファベット順に既存が並んでいるので `account` の次に挿入):

```rust
mod account;
mod clip;
mod column;
```

```rust
pub use account::Account;
pub use clip::Clip;
pub use column::{Column, ColumnGroup, ColumnKind, FilterQuery};
```

- [ ] **Step 3: コンパイル確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなくビルドが通る(`Clip` はまだどこからも参照されないため `dead_code` 警告が出ても無視してよい。`domain/mod.rs` 冒頭に既存の `#![allow(dead_code, unused_imports)]` がある)。

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/domain/clip.rs src-tauri/src/domain/mod.rs
git commit -m "feat: domain::Clip型を追加"
```

---

### Task 2: お気に入り REST ラッパを追加(`api/notes.rs`)

**Files:**
- Modify: `src-tauri/src/api/notes.rs`

**Interfaces:**
- Consumes: `MisskeyClient::post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>`(`src-tauri/src/api/client.rs`)
- Produces: `pub async fn create_favorite(client: &MisskeyClient, note_id: &str) -> Result<()>`, `pub async fn delete_favorite(client: &MisskeyClient, note_id: &str) -> Result<()>`

- [ ] **Step 1: `delete_reaction` の直後に2関数を追加**

`src-tauri/src/api/notes.rs` の `delete_reaction` 関数の直後(`vote_poll` の手前)に挿入:

```rust
/// お気に入り登録。`notes/favorites/create`。
pub async fn create_favorite(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/favorites/create", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}

/// お気に入り解除。`notes/favorites/delete`。
pub async fn delete_favorite(client: &MisskeyClient, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post("notes/favorites/delete", &json!({ "noteId": note_id }))
        .await?;
    Ok(())
}
```

- [ ] **Step 2: ビルド確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなくビルドが通る(既存の `create_reaction`/`delete_reaction` と同じ形なので新規テストは不要 — 既存の `client.rs` 側テストがボディ組み立てをカバー済み)。

- [ ] **Step 3: コミット**

```bash
git add src-tauri/src/api/notes.rs
git commit -m "feat: notes/favorites REST ラッパを追加"
```

---

### Task 3: クリップ REST ラッパを追加(`api/clips.rs` 新設)

**Files:**
- Create: `src-tauri/src/api/clips.rs`
- Modify: `src-tauri/src/api/mod.rs`

**Interfaces:**
- Consumes: `MisskeyClient::post`, `domain::Clip`
- Produces: `pub async fn list_clips(client: &MisskeyClient) -> Result<Vec<Clip>>`, `pub async fn create_clip(client: &MisskeyClient, name: &str) -> Result<Clip>`, `pub async fn add_note_to_clip(client: &MisskeyClient, clip_id: &str, note_id: &str) -> Result<()>`

- [ ] **Step 1: `api/clips.rs` を作成**

```rust
//! クリップ系 REST（一覧取得・作成・ノート追加）。

use crate::api::MisskeyClient;
use crate::domain::Clip;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

/// Misskey の Clip オブジェクトの生型。`favoritedCount`/`user`/`lastClippedAt` 等
/// 今回使わないフィールドは受けない。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawClip {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    is_public: bool,
    notes_count: i64,
}

impl From<RawClip> for Clip {
    fn from(r: RawClip) -> Self {
        Clip {
            id: r.id,
            name: r.name,
            description: r.description,
            is_public: r.is_public,
            notes_count: r.notes_count,
        }
    }
}

/// 自分のクリップ一覧。`clips/list`（引数なし）。
pub async fn list_clips(client: &MisskeyClient) -> Result<Vec<Clip>> {
    let raw: Vec<RawClip> = client.post("clips/list", &json!({})).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// クリップ新規作成。`clips/create`。isPublic/description は既定値(非公開・なし)のまま。
pub async fn create_clip(client: &MisskeyClient, name: &str) -> Result<Clip> {
    let raw: RawClip = client.post("clips/create", &json!({ "name": name })).await?;
    Ok(raw.into())
}

/// クリップへノートを追加。`clips/add-note`。
pub async fn add_note_to_clip(client: &MisskeyClient, clip_id: &str, note_id: &str) -> Result<()> {
    let _: serde_json::Value = client
        .post(
            "clips/add-note",
            &json!({ "clipId": clip_id, "noteId": note_id }),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_clip_converts_to_domain() {
        let raw = RawClip {
            id: "c1".into(),
            name: "あとで読む".into(),
            description: Some("desc".into()),
            is_public: false,
            notes_count: 3,
        };
        let clip: Clip = raw.into();
        assert_eq!(clip.id, "c1");
        assert_eq!(clip.name, "あとで読む");
        assert_eq!(clip.description.as_deref(), Some("desc"));
        assert!(!clip.is_public);
        assert_eq!(clip.notes_count, 3);
    }
}
```

- [ ] **Step 2: `api/mod.rs` に登録**

`src-tauri/src/api/mod.rs` の `pub mod` 群に `pub mod clips;` を追加(アルファベット順、`pub mod client;` の直後):

```rust
pub mod client;
pub mod clips;
pub mod drive;
```

- [ ] **Step 3: テスト実行**

Run: `cd src-tauri && cargo test raw_clip_converts_to_domain`
Expected: `test api::clips::tests::raw_clip_converts_to_domain ... ok`

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/api/clips.rs src-tauri/src/api/mod.rs
git commit -m "feat: clips系REST ラッパを追加"
```

---

### Task 4: Tauri command を追加し TS バインディングを再生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Create: `src-tauri/src/commands/clip.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AppState::client_for(&self, account_id: &str) -> Result<MisskeyClient>`(`src-tauri/src/state.rs:123`)、Task 2/3 の api 関数
- Produces: commands `favorite_note(state, account_id, note_id) -> Result<()>`, `unfavorite_note(state, account_id, note_id) -> Result<()>`, `list_clips(state, account_id) -> Result<Vec<Clip>>`, `create_clip(state, account_id, name) -> Result<Clip>`, `add_note_to_clip(state, account_id, clip_id, note_id) -> Result<()>`

- [ ] **Step 1: `commands/note.rs` に favorite command を追加**

`src-tauri/src/commands/note.rs` 冒頭の import に favorite 関数を追加(`create_reaction, delete_reaction` の並びに追記):

```rust
use crate::api::notes::{
    create_favorite, create_note, create_reaction, delete_favorite, delete_note, delete_reaction,
    renote as api_renote, vote_poll as api_vote_poll, NoteDraft, VisibilityInput,
};
```

`unreact` 関数の直後に追加:

```rust
/// お気に入り登録。
#[tauri::command]
#[specta::specta]
pub async fn favorite_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    create_favorite(&client, &note_id).await
}

/// お気に入り解除。
#[tauri::command]
#[specta::specta]
pub async fn unfavorite_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    delete_favorite(&client, &note_id).await
}
```

- [ ] **Step 2: `commands/clip.rs` を新設**

```rust
//! クリップ系 command（一覧取得・作成・ノート追加）。

use crate::api::clips::{add_note_to_clip as api_add_note_to_clip, create_clip as api_create_clip, list_clips as api_list_clips};
use crate::domain::Clip;
use crate::error::Result;
use crate::state::AppState;
use tauri::State;

/// 自分のクリップ一覧を取得。
#[tauri::command]
#[specta::specta]
pub async fn list_clips(state: State<'_, AppState>, account_id: String) -> Result<Vec<Clip>> {
    let client = state.client_for(&account_id)?;
    api_list_clips(&client).await
}

/// クリップを新規作成する。
#[tauri::command]
#[specta::specta]
pub async fn create_clip(state: State<'_, AppState>, account_id: String, name: String) -> Result<Clip> {
    let client = state.client_for(&account_id)?;
    api_create_clip(&client, &name).await
}

/// ノートをクリップへ追加する。
#[tauri::command]
#[specta::specta]
pub async fn add_note_to_clip(
    state: State<'_, AppState>,
    account_id: String,
    clip_id: String,
    note_id: String,
) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api_add_note_to_clip(&client, &clip_id, &note_id).await
}
```

- [ ] **Step 3: `commands/mod.rs` に登録**

`pub mod` 群に `pub mod clip;` を追加(アルファベット順、`pub mod app;` の直後):

```rust
pub mod account;
pub mod app;
pub mod clip;
pub mod column;
pub mod mute;
pub mod note;
```

`pub use note::{...}` の行に favorite/unfavorite を追加:

```rust
pub use note::{
    delete_note_cmd, favorite_note, list_custom_emojis, post_note, react, renote, unfavorite_note,
    unreact, upload_file,
};
```

- [ ] **Step 4: `lib.rs` の `specta_builder()` に5コマンドを登録**

`src-tauri/src/lib.rs` の `commands::note::unreact,` の直後に挿入:

```rust
            commands::note::react,
            commands::note::unreact,
            commands::note::favorite_note,
            commands::note::unfavorite_note,
            commands::note::vote_poll,
```

`commands::mute::sync_server_mutes,` の直後(commands マクロの末尾付近)に挿入:

```rust
            commands::mute::sync_server_mutes,
            commands::clip::list_clips,
            commands::clip::create_clip,
            commands::clip::add_note_to_clip,
```

- [ ] **Step 5: ビルド + TS バインディング再生成 + 既存テスト確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなくビルドが通る。

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: `test ... generates_frontend_bindings ... ok`。これで `frontend/src/bindings/tauri.gen.ts` に `favoriteNote`/`unfavoriteNote`/`listClips`/`createClip`/`addNoteToClip`/`Clip` が生成される。

Run: `grep -c "favoriteNote\|listClips\|Clip" frontend/src/bindings/tauri.gen.ts`
Expected: 0 より大きい数値

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/commands/clip.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: お気に入り/クリップのTauri commandを追加"
```

---

### Task 5: フロント store に favorite/clip 操作を追加

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.favoriteNote(accountId, noteId)`, `commands.unfavoriteNote(accountId, noteId)`, `commands.listClips(accountId)`, `commands.createClip(accountId, name)`, `commands.addNoteToClip(accountId, clipId, noteId)`(すべて Task 4 で生成済み)、既存 `unwrap`, `this.#collectNotes(noteId)`, `this.#fail(e)`, `this.#log(level, text)`
- Produces: `AppStore.toggleFavorite(accountId: string, noteId: string): Promise<void>`, `AppStore.listClips(accountId: string): Promise<Clip[]>`, `AppStore.createClip(accountId: string, name: string): Promise<Clip>`, `AppStore.addNoteToClip(accountId: string, clipId: string, noteId: string): Promise<void>`

- [ ] **Step 1: import に `Clip` 型を追加**

`frontend/src/lib/store.svelte.ts` 冒頭の `import type { ... } from "../bindings/tauri.gen";` ブロックに `Clip` を追加(`LatestRelease` の並びに追記):

```typescript
import type {
  Account,
  Note,
  ConnectionState,
  EmojiDef,
  NoteDraft_Deserialize as NoteDraft,
  VisibilityInput,
  OpenedColumn,
  NoteUpdate,
  ColumnKind,
  FilterQuery,
  Notification,
  MuteConfig,
  NotifyConfig,
  UiPrefs,
  LatestRelease,
  Clip,
} from "../bindings/tauri.gen";
```

- [ ] **Step 2: `toggleFavorite` を `toggleReaction` の直後に追加**

`async toggleReaction(...)` メソッドの閉じ `}` の直後に挿入:

```typescript
  async toggleFavorite(accountId: string, noteId: string) {
    const targets = this.#collectNotes(noteId);
    if (targets.length === 0) return;
    const backups = targets.map((n) => ({ n, was: n.isFavoritedByMe }));
    const already = targets[0].isFavoritedByMe;
    targets.forEach((n) => (n.isFavoritedByMe = !already));

    try {
      if (already) {
        await unwrap(commands.unfavoriteNote(accountId, noteId));
        this.#log("info", "お気に入りを解除しました");
      } else {
        await unwrap(commands.favoriteNote(accountId, noteId));
        this.#log("success", "お気に入りに登録しました");
      }
    } catch (e) {
      backups.forEach(({ n, was }) => (n.isFavoritedByMe = was));
      this.#fail(e);
    }
  }
```

- [ ] **Step 3: `listClips`/`createClip`/`addNoteToClip` を `toggleFavorite` の直後に追加**

```typescript
  async listClips(accountId: string): Promise<Clip[]> {
    return unwrap(commands.listClips(accountId));
  }

  async createClip(accountId: string, name: string): Promise<Clip> {
    const clip = await unwrap(commands.createClip(accountId, name));
    this.#log("success", `クリップを作成しました: ${clip.name}`);
    return clip;
  }

  async addNoteToClip(accountId: string, clipId: string, noteId: string) {
    try {
      await unwrap(commands.addNoteToClip(accountId, clipId, noteId));
      this.#log("success", "クリップに追加しました");
    } catch (e) {
      this.#fail(e);
    }
  }
```

- [ ] **Step 4: 型チェック確認**

Run: `cd frontend && pnpm check`
Expected: エラーなし(`isFavoritedByMe` は Task 4 で再生成済みの `Note` 型に既存フィールドとして存在する)。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: お気に入り/クリップ操作をstoreに追加"
```

---

### Task 6: `NoteMenu.svelte` を新設

**Files:**
- Create: `frontend/src/ui/NoteMenu.svelte`

**Interfaces:**
- Consumes: `app.toggleFavorite`, `app.listClips`, `app.createClip`, `app.addNoteToClip`(Task 5)、`Note`/`Clip` 型
- Produces: `NoteMenu` コンポーネント。Props: `{ accountId: string; note: Note; onclose: () => void }`。ノート本体・お気に入り状態の判定はこのコンポーネント内で完結させ、呼び出し元(NoteCard)は開閉制御のみ行う。

- [ ] **Step 1: コンポーネントを作成**

```svelte
<script lang="ts">
  import type { Note, Clip } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { Star, Paperclip, ChevronRight } from "@lucide/svelte";

  let { accountId, note, onclose }: { accountId: string; note: Note; onclose: () => void } = $props();

  let clipSubmenuOpen = $state(false);
  let clips = $state<Clip[] | null>(null);
  let clipsLoading = $state(false);
  let creatingClip = $state(false);
  let newClipName = $state("");

  function toggleFavorite() {
    app.toggleFavorite(accountId, note.id);
    onclose();
  }

  function openClipSubmenu() {
    clipSubmenuOpen = true;
    if (clips === null && !clipsLoading) {
      clipsLoading = true;
      app
        .listClips(accountId)
        .then((list) => (clips = list))
        .finally(() => (clipsLoading = false));
    }
  }

  function pickClip(clip: Clip) {
    app.addNoteToClip(accountId, clip.id, note.id);
    onclose();
  }

  function startCreateClip() {
    creatingClip = true;
    newClipName = "";
  }

  async function confirmCreateClip() {
    const name = newClipName.trim();
    if (!name) return;
    const clip = await app.createClip(accountId, name);
    await app.addNoteToClip(accountId, clip.id, note.id);
    onclose();
  }
</script>

<div class="menu">
  <button class="item" onclick={toggleFavorite}>
    <Star size={14} />
    {note.isFavoritedByMe ? "お気に入り解除" : "お気に入り登録"}
  </button>

  <div class="item-wrap" onmouseenter={openClipSubmenu}>
    <button class="item" onclick={openClipSubmenu}>
      <Paperclip size={14} />
      クリップに追加
      <ChevronRight size={14} class="chevron" />
    </button>

    {#if clipSubmenuOpen}
      <div class="submenu">
        {#if creatingClip}
          <div class="create-row">
            <input
              class="name-input"
              placeholder="クリップ名"
              bind:value={newClipName}
              onkeydown={(e) => e.key === "Enter" && confirmCreateClip()}
            />
            <button class="confirm-btn" disabled={!newClipName.trim()} onclick={confirmCreateClip}>
              作成
            </button>
          </div>
        {:else}
          {#if clipsLoading}
            <span class="hint">読み込み中…</span>
          {:else if clips && clips.length === 0}
            <span class="hint">クリップがありません</span>
          {:else if clips}
            {#each clips as clip (clip.id)}
              <button class="item" onclick={() => pickClip(clip)}>{clip.name}</button>
            {/each}
          {/if}
          <button class="item new-clip" onclick={startCreateClip}>＋ 新規クリップを作成</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .menu {
    width: 200px;
    padding: 4px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .item-wrap {
    position: relative;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    color: var(--text);
    font-size: 0.82rem;
    text-align: left;
    cursor: pointer;
    border-radius: 5px;
    box-sizing: border-box;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item :global(.chevron) {
    margin-left: auto;
  }
  .submenu {
    position: absolute;
    left: 100%;
    top: 0;
    width: 200px;
    max-height: 280px;
    overflow-y: auto;
    padding: 4px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .new-clip {
    color: var(--accent);
  }
  .hint {
    display: block;
    padding: 6px 8px;
    font-size: 0.78rem;
    color: var(--text-dim);
  }
  .create-row {
    display: flex;
    gap: 4px;
    padding: 4px;
  }
  .name-input {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 0.82rem;
    box-sizing: border-box;
  }
  .confirm-btn {
    padding: 4px 8px;
    border: none;
    border-radius: 5px;
    background: var(--accent);
    color: var(--surface-1);
    font-size: 0.78rem;
    cursor: pointer;
  }
  .confirm-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
```

- [ ] **Step 2: 型チェック確認**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/NoteMenu.svelte
git commit -m "feat: NoteMenuコンポーネントを追加"
```

---

### Task 7: `NoteCard.svelte` に `⋯` ボタンを組み込む

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`

**Interfaces:**
- Consumes: `NoteMenu`(Task 6)、既存の `portal` 関数(`frontend/src/ui/NoteCard.svelte:17`)、既存の `react-wrap`/`picker-overlay`/`picker-pop` スタイルパターン

- [ ] **Step 1: import に `MoreHorizontal` アイコンと `NoteMenu` を追加**

`frontend/src/ui/NoteCard.svelte` の import 群を変更:

```svelte
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import NoteMenu from "./NoteMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Self from "./NoteCard.svelte";
  import { relativeTime } from "../lib/time";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji, isRemoteCustomEmoji } from "../lib/emoji";
  import { Reply, Repeat2, Quote, SmilePlus, Globe, House, Lock, Mail, MoreHorizontal } from "@lucide/svelte";
```

- [ ] **Step 2: メニュー開閉の state とボタン位置計算を追加**

`react()` 関数の直後(`pollExpired` の手前)に追加:

```svelte
  // ノートメニュー(お気に入り/クリップ)。リアクションピッカーと同じ position:fixed
  // portal パターンで .notes の overflow クリップを脱出させる。
  let noteMenuOpen = $state(false);
  let noteMenuBtn = $state<HTMLElement | null>(null);
  let noteMenuPos = $state<{ left: number; top: number } | null>(null);
  const MENU_W = 200;
  $effect(() => {
    if (!noteMenuOpen || !noteMenuBtn) return;
    const r = noteMenuBtn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.right - MENU_W), window.innerWidth - MENU_W - 8);
    noteMenuPos = { left, top: r.bottom + 6 };
  });
```

- [ ] **Step 3: アクション行に `⋯` ボタンを追加**

`.react-wrap` の閉じタグ(`</div>`、`footer` 直前)の直後に追加:

```svelte
          <div class="menu-wrap">
            <button
              bind:this={noteMenuBtn}
              title="その他"
              class:on={noteMenuOpen}
              onclick={() => (noteMenuOpen = !noteMenuOpen)}
            >
              <MoreHorizontal size={15} />
            </button>
            {#if noteMenuOpen && noteMenuPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="picker-overlay" use:portal onclick={() => (noteMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="picker-pop"
                  style={`left:${noteMenuPos.left}px;top:${noteMenuPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <NoteMenu {accountId} note={inner} onclose={() => (noteMenuOpen = false)} />
                </div>
              </div>
            {/if}
          </div>
```

正確な挿入位置を明示するため、変更後の `.react-wrap` ブロック全体は以下の形になる(既存内容は変更しない、直後に `.menu-wrap` を追加するだけ):

```svelte
          <div class="react-wrap">
            <button
              bind:this={pickerBtn}
              title="リアクション"
              class:on={showPicker}
              onclick={togglePicker}
            >
              <SmilePlus size={15} /> {inner.reactionCount || ""}
            </button>
            {#if showPicker && pickerPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="picker-overlay" use:portal onclick={() => (app.reactPicker = null)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="picker-pop"
                  style={`left:${pickerPos.left}px;top:${pickerPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <ReactionPicker {accountId} onpick={react} />
                </div>
              </div>
            {/if}
          </div>
          <div class="menu-wrap">
            <button
              bind:this={noteMenuBtn}
              title="その他"
              class:on={noteMenuOpen}
              onclick={() => (noteMenuOpen = !noteMenuOpen)}
            >
              <MoreHorizontal size={15} />
            </button>
            {#if noteMenuOpen && noteMenuPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="picker-overlay" use:portal onclick={() => (noteMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="picker-pop"
                  style={`left:${noteMenuPos.left}px;top:${noteMenuPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <NoteMenu {accountId} note={inner} onclose={() => (noteMenuOpen = false)} />
                </div>
              </div>
            {/if}
          </div>
```

- [ ] **Step 4: `.menu-wrap` スタイルを追加**

`<style>` ブロック内、`.react-wrap { position: relative; }` の直後に追加:

```css
  .menu-wrap {
    position: relative;
  }
```

- [ ] **Step 5: 型チェック確認**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte
git commit -m "feat: NoteCardに⋯メニューを追加"
```

---

### Task 8: 手動動作確認

**Files:** なし(コード変更なし、確認のみ)

- [ ] **Step 1: アプリを起動**

Run: `cargo tauri dev`
Expected: 起動し、タイムラインが表示される。

- [ ] **Step 2: お気に入りトグルを確認**

任意のノートの `⋯` → 「お気に入り登録」をクリック。ボタンラベルが「お気に入り解除」に変わること、再度クリックで「お気に入り登録」に戻ることを目視確認。同じノートが複数カラムに表示されている場合は両方に反映されることも確認。

- [ ] **Step 3: 既存クリップへの追加を確認**

事前に Misskey Web UI 等でクリップを1つ作成しておく。`⋯` → 「クリップに追加」にホバー → サブメニューにそのクリップ名が表示されること、クリックで成功ログ(「クリップに追加しました」)が出ることを確認。

- [ ] **Step 4: 新規クリップ作成を確認**

`⋯` → 「クリップに追加」→「＋ 新規クリップを作成」→ 名前入力 → 「作成」。成功ログ(「クリップを作成しました: {name}」→「クリップに追加しました」)が出ること、Misskey Web UI 側でそのクリップにノートが追加されていることを確認。

- [ ] **Step 5: `pnpm check` と `cargo test` を通しで実行**

Run: `cd frontend && pnpm check && cd ../src-tauri && cargo test`
Expected: すべて成功(ネットワーク依存の `#[ignore]` テストはスキップされる)。
