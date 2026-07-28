# リアクション・Renoteしたユーザー一覧表示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `NoteCard.svelte` のリアクションバッジ／Renote件数をホバーすると、そのリアクション（絵文字ごと）／Renoteを行ったユーザー一覧をポップオーバー表示する。

**Architecture:** Rust側に Misskey `notes/reactions`（`type`で絞り込み）・`notes/renotes` を叩く薄いAPI関数とTauriコマンドを追加し、`User`/新規`ReactionUser`ドメイン型で返す。フロントは新規 `ReactionUsersPopover.svelte` が150msのホバーディレイでコマンドを呼び、既存の `portal` アクション（`NoteCard.svelte` に定義済み）で `position: fixed` 表示する。同一 note+key の結果はモジュールスコープの `Map` にキャッシュする。

**Tech Stack:** Rust (Tauri v2, tauri-specta, reqwest), Svelte 5 (runes)

## Global Constraints

- 1リクエストあたりの取得上限は100件固定（Misskey API側の上限でもある）。ページングは実装しない。
- 既存のクリック操作（リアクショントグル・Renote実行）の挙動は変更しない。ホバーによる一覧表示は追加のみ。
- `specta_builder()`（`src-tauri/src/lib.rs`）に新規コマンドを登録し、`cargo test` でTSバインディングを再生成すること。

---

### Task 1: ドメイン型 `ReactionUser` の追加

**Files:**
- Modify: `src-tauri/src/domain/reaction.rs`
- Modify: `src-tauri/src/domain/mod.rs:30`

**Interfaces:**
- Produces: `domain::ReactionUser { user: User, reaction: String }`（`specta::Type` 付き、TS export対象）

- [ ] **Step 1: `ReactionUser` 型を追加**

`src-tauri/src/domain/reaction.rs` の先頭 import に `User` を追加し、末尾に型を追加する。

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

use super::User;
```

(既存の `use` 行はこの2行の後に続ける形で、ファイル末尾に追記)

```rust
/// リアクション付与ユーザー一覧のエントリ（`notes/reactions` のレスポンス正規化後）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReactionUser {
    pub user: User,
    /// Misskey形式キー（Unicode生 or :name@host:）
    pub reaction: String,
}
```

- [ ] **Step 2: `domain/mod.rs` の re-export に追加**

`src-tauri/src/domain/mod.rs:30` を以下に変更:

```rust
pub use reaction::{EmojiDef, ReactionSummary, ReactionUser};
```

- [ ] **Step 3: コンパイル確認**

Run: `cd src-tauri && cargo build 2>&1 | tail -30`
Expected: エラーなくビルドが通ること（`ReactionUser` が未使用でも `#![allow(dead_code, unused_imports)]` が `domain/mod.rs` 冒頭にあるため warning にならない）

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/domain/reaction.rs src-tauri/src/domain/mod.rs
git commit -m "feat: ReactionUserドメイン型を追加"
```

---

### Task 2: 生レスポンス正規化 `RawReactionUser`

**Files:**
- Modify: `src-tauri/src/api/normalize.rs`

**Interfaces:**
- Consumes: `domain::ReactionUser`（Task 1）, `normalize::RawUser`（既存）
- Produces: `normalize::RawReactionUser`（`Deserialize`）、`impl From<RawReactionUser> for ReactionUser`

- [ ] **Step 1: import に `ReactionUser` を追加**

`src-tauri/src/api/normalize.rs:3` を変更:

```rust
use crate::domain::{DriveFile, Note, Notification, Poll, PollChoice, ReactionUser, User, Visibility};
```

- [ ] **Step 2: `RawReactionUser` と `From` 実装を追加**

`RawUser` の定義ブロック（`impl From<RawUser> for User { ... }` の直後、`RawNotification` 定義の前）に追加する:

```rust
/// Misskey の NoteReaction オブジェクト（`notes/reactions` のレスポンス要素）を受ける生型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReactionUser {
    pub user: RawUser,
    /// Misskey形式キー（Unicode生 or :name@host:）。JSON上のフィールド名は `type`。
    #[serde(rename = "type")]
    pub reaction: String,
}

impl From<RawReactionUser> for ReactionUser {
    fn from(r: RawReactionUser) -> Self {
        ReactionUser {
            user: r.user.into(),
            reaction: r.reaction,
        }
    }
}
```

- [ ] **Step 3: 失敗するテストを書く**

`src-tauri/src/api/normalize.rs` の `#[cfg(test)] mod tests` ブロック内、`parses_user_emojis_for_display_name` テストの後に追加:

```rust
    #[test]
    fn parses_reaction_user() {
        let raw: RawReactionUser = serde_json::from_str(
            r#"{"id":"r1","createdAt":"2026-07-05T00:00:00Z","type":"👍",
                "user":{"id":"u1","username":"alice"}}"#,
        )
        .unwrap();
        let ru: ReactionUser = raw.into();
        assert_eq!(ru.reaction, "👍");
        assert_eq!(ru.user.id, "u1");
        assert_eq!(ru.user.username, "alice");
    }
```

- [ ] **Step 4: テスト実行して失敗を確認**

Run: `cd src-tauri && cargo test parses_reaction_user`
Expected: FAIL（`RawReactionUser` 未定義、または型不一致のコンパイルエラー）— Step 2 を先に書いた場合はここで PASS するはずなので、Step 2/3 の順序は「実装が先」でも構わない。もし Step 2 を飛ばしていた場合は `cannot find type RawReactionUser` で失敗することを確認する。

- [ ] **Step 5: テスト実行してパスを確認**

Run: `cd src-tauri && cargo test parses_reaction_user`
Expected: `test api::normalize::tests::parses_reaction_user ... ok`

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/api/normalize.rs
git commit -m "feat: notes/reactionsレスポンスの正規化を追加"
```

---

### Task 3: API関数 `get_reactions` / `get_renotes`

**Files:**
- Modify: `src-tauri/src/api/notes.rs`

**Interfaces:**
- Consumes: `MisskeyClient::post`（既存）, `normalize::RawReactionUser`（Task 2）, `normalize::RawNote`（既存）
- Produces:
  - `pub async fn get_reactions(client: &MisskeyClient, note_id: &str, reaction_type: Option<&str>) -> Result<Vec<ReactionUser>>`
  - `pub async fn get_renotes(client: &MisskeyClient, note_id: &str) -> Result<Vec<User>>`

- [ ] **Step 1: import を更新**

`src-tauri/src/api/notes.rs:3-5` を以下に変更:

```rust
use crate::api::normalize::{RawNote, RawReactionUser};
use crate::api::MisskeyClient;
use crate::domain::{Note, ReactionUser, User, Visibility};
```

- [ ] **Step 2: 関数を追加**

`delete_reaction` 関数（`src-tauri/src/api/notes.rs:121-126` 付近）の直後に追加:

```rust
/// リアクション付与ユーザー一覧取得。`reaction_type` を指定すると絵文字キーで絞り込む。最大100件。
pub async fn get_reactions(
    client: &MisskeyClient,
    note_id: &str,
    reaction_type: Option<&str>,
) -> Result<Vec<ReactionUser>> {
    let mut body = json!({ "noteId": note_id, "limit": 100 });
    if let Some(t) = reaction_type {
        body["type"] = json!(t);
    }
    let raw: Vec<RawReactionUser> = client.post("notes/reactions", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// Renoteしたユーザー一覧取得。最大100件。
pub async fn get_renotes(client: &MisskeyClient, note_id: &str) -> Result<Vec<User>> {
    let raw: Vec<RawNote> = client
        .post("notes/renotes", &json!({ "noteId": note_id, "limit": 100 }))
        .await?;
    Ok(raw.into_iter().map(|n| n.user.into()).collect())
}
```

- [ ] **Step 3: ビルド確認**

Run: `cd src-tauri && cargo build 2>&1 | tail -30`
Expected: エラーなくビルドが通ること

- [ ] **Step 4: 既存テストが壊れていないことを確認**

Run: `cd src-tauri && cargo test --lib notes::`
Expected: 既存の `notes.rs` 内テスト（`draft_serializes_only_present_fields` 等）が全て PASS

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/notes.rs
git commit -m "feat: notes/reactions・notes/renotesを叩くAPI関数を追加"
```

---

### Task 4: Tauriコマンド登録とバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `api::notes::get_reactions` / `get_renotes`（Task 3）, `state.client_for`（既存）
- Produces:
  - TSコマンド `commands.getNoteReactions(accountId: string, noteId: string, reactionType: string | null): Promise<Result<ReactionUser[], Error>>`
  - TSコマンド `commands.getNoteRenotes(accountId: string, noteId: string): Promise<Result<User[], Error>>`

- [ ] **Step 1: `commands/note.rs` の import を更新**

`src-tauri/src/commands/note.rs:6-11` を以下に変更:

```rust
use crate::api::notes::{
    create_favorite, create_note, create_reaction, delete_favorite, delete_note, delete_reaction,
    get_reactions, get_renotes, renote as api_renote, vote_poll as api_vote_poll, NoteDraft,
    VisibilityInput,
};
use crate::domain::{DriveFile, EmojiDef, Note, ReactionUser, SourceItem, User};
```

- [ ] **Step 2: コマンド関数を追加**

`unreact` 関数（`src-tauri/src/commands/note.rs:64-72` 付近）の直後に追加:

```rust
/// リアクション付与ユーザー一覧取得（絵文字ごと、最大100件）。
#[tauri::command]
#[specta::specta]
pub async fn get_note_reactions(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    reaction_type: Option<String>,
) -> Result<Vec<ReactionUser>> {
    let client = state.client_for(&account_id)?;
    get_reactions(&client, &note_id, reaction_type.as_deref()).await
}

/// Renoteしたユーザー一覧取得（最大100件）。
#[tauri::command]
#[specta::specta]
pub async fn get_note_renotes(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    get_renotes(&client, &note_id).await
}
```

- [ ] **Step 3: `lib.rs` の `specta_builder()` に登録**

`src-tauri/src/lib.rs:68` (`commands::note::unreact,` の行) の直後に追加:

```rust
            commands::note::get_note_reactions,
            commands::note::get_note_renotes,
```

- [ ] **Step 4: バインディング再生成テストを実行**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: `test lib::tests::generates_frontend_bindings ... ok`

- [ ] **Step 5: 生成されたTS型を確認**

Run: `grep -n "getNoteReactions\|getNoteRenotes\|ReactionUser" frontend/src/bindings/tauri.gen.ts`
Expected: `getNoteReactions` / `getNoteRenotes` コマンドと `export type ReactionUser` が出力されている

- [ ] **Step 6: 全体テストを実行**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS（`#[ignore]` の実接続テストはスキップされる）

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: リアクション・Renoteユーザー一覧取得コマンドを追加"
```

---

### Task 5: `ReactionUsersPopover.svelte` の新規作成

**Files:**
- Create: `frontend/src/ui/ReactionUsersPopover.svelte`

**Interfaces:**
- Consumes: `commands.getNoteReactions` / `commands.getNoteRenotes`（Task 4）, `unwrap`（`../lib/ipc`）, `Mfm`（`../render/Mfm.svelte`）
- Produces: コンポーネント props `{ accountId: string; noteId: string; reactionKey: string | null; totalCount: number; left: number; top: number }`。`reactionKey === null` の場合はRenote一覧モード。

- [ ] **Step 1: コンポーネントを作成**

`frontend/src/ui/ReactionUsersPopover.svelte` を新規作成:

```svelte
<script lang="ts" module>
  import type { User } from "../bindings/tauri.gen";
  import { commands, unwrap } from "../lib/ipc";

  // note+key単位のキャッシュ。モジュールスコープなのでコンポーネントの再マウントを跨いで保持される。
  const cache = new Map<string, Promise<User[]>>();

  function cacheKey(noteId: string, reactionKey: string | null): string {
    return `${noteId}:${reactionKey ?? "\0renote"}`;
  }

  function fetchUsers(accountId: string, noteId: string, reactionKey: string | null): Promise<User[]> {
    const key = cacheKey(noteId, reactionKey);
    let p = cache.get(key);
    if (!p) {
      p =
        reactionKey !== null
          ? unwrap(commands.getNoteReactions(accountId, noteId, reactionKey)).then((rs) => rs.map((r) => r.user))
          : unwrap(commands.getNoteRenotes(accountId, noteId));
      p.catch(() => cache.delete(key));
      cache.set(key, p);
    }
    return p;
  }
</script>

<script lang="ts">
  import Mfm from "../render/Mfm.svelte";

  let {
    accountId,
    noteId,
    reactionKey,
    totalCount,
    left,
    top,
  }: {
    accountId: string;
    noteId: string;
    reactionKey: string | null;
    totalCount: number;
    left: number;
    top: number;
  } = $props();

  let users = $state<User[] | null>(null);
  let failed = $state(false);

  $effect(() => {
    const acc = accountId;
    const nid = noteId;
    const key = reactionKey;
    users = null;
    failed = false;
    fetchUsers(acc, nid, key)
      .then((u) => (users = u))
      .catch(() => (failed = true));
  });

  const displayName = (u: User) => u.name ?? u.username;
  const acct = (u: User) => (u.host ? `@${u.username}@${u.host}` : `@${u.username}`);
  const moreCount = $derived(users ? Math.max(0, totalCount - users.length) : 0);
</script>

<div class="popover" style={`left:${left}px;top:${top}px`}>
  {#if failed}
    <div class="status">取得に失敗しました</div>
  {:else if users === null}
    <div class="status">読み込み中…</div>
  {:else if users.length === 0}
    <div class="status">なし</div>
  {:else}
    <ul>
      {#each users as u (u.id)}
        <li>
          {#if u.avatarUrl}
            <img class="avatar" src={u.avatarUrl} alt="" loading="lazy" />
          {:else}
            <div class="avatar placeholder"></div>
          {/if}
          <span class="name"><Mfm text={displayName(u)} emojis={u.emojis} simple /></span>
          <span class="acct">{acct(u)}</span>
        </li>
      {/each}
    </ul>
    {#if moreCount > 0}
      <div class="more">他{moreCount}件</div>
    {/if}
  {/if}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 1000;
    min-width: 160px;
    max-width: 240px;
    max-height: 280px;
    overflow-y: auto;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    padding: 4px;
  }
  .status {
    padding: 6px 8px;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    font-size: 0.8rem;
  }
  .avatar {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    flex-shrink: 0;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--border);
  }
  .name {
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.72rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    padding: 3px 6px;
    font-size: 0.74rem;
    color: var(--text-dim);
  }
</style>
```

- [ ] **Step 2: 型チェック**

Run: `cd frontend && pnpm check`
Expected: `ReactionUsersPopover.svelte` に関するエラーが無いこと（既存の他ファイルのエラーが元々ある場合はこの限りではないため、新規ファイル由来のエラーが無いことを確認する）

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/ReactionUsersPopover.svelte
git commit -m "feat: リアクション・Renoteユーザー一覧のポップオーバーを追加"
```

---

### Task 6: `NoteCard.svelte` へのホバー配線

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`

**Interfaces:**
- Consumes: `ReactionUsersPopover`（Task 5）, 既存の `portal` アクション（`NoteCard.svelte:18-21`）

- [ ] **Step 1: import に `ReactionUsersPopover` を追加**

`frontend/src/ui/NoteCard.svelte:9` (`import ConfirmDialog from "./ConfirmDialog.svelte";` の行) の直後に追加:

```svelte
  import ReactionUsersPopover from "./ReactionUsersPopover.svelte";
```

- [ ] **Step 2: ホバー状態とハンドラを追加**

`frontend/src/ui/NoteCard.svelte:126` (`doRenote` 関数の直後、`// キーボード選択中はスクロールで見える位置へ` コメントの前) に追加:

```svelte
  // リアクション/Renoteの「誰が」ポップオーバー。ホバーで表示、150msのin/outディレイで
  // ボタン→ポップオーバー間のマウス移動中に消えないようにする。
  type HoverTarget = { kind: "reaction"; key: string } | { kind: "renote" };
  let hoverTarget = $state<HoverTarget | null>(null);
  let hoverBtn = $state<HTMLElement | null>(null);
  let hoverShowTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverHideTimer: ReturnType<typeof setTimeout> | null = null;
  const POPOVER_W = 240;

  function enterHover(target: HoverTarget, btn: HTMLElement) {
    if (!accountId) return;
    if (hoverHideTimer) {
      clearTimeout(hoverHideTimer);
      hoverHideTimer = null;
    }
    if (hoverShowTimer) clearTimeout(hoverShowTimer);
    hoverShowTimer = setTimeout(() => {
      hoverTarget = target;
      hoverBtn = btn;
    }, 150);
  }
  function leaveHover() {
    if (hoverShowTimer) {
      clearTimeout(hoverShowTimer);
      hoverShowTimer = null;
    }
    hoverHideTimer = setTimeout(() => {
      hoverTarget = null;
      hoverBtn = null;
    }, 150);
  }
  function keepHover() {
    if (hoverHideTimer) {
      clearTimeout(hoverHideTimer);
      hoverHideTimer = null;
    }
  }

  let hoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!hoverTarget || !hoverBtn) {
      hoverPos = null;
      return;
    }
    const r = hoverBtn.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.left), window.innerWidth - POPOVER_W - 8);
    hoverPos = { left, top: r.bottom + 4 };
  });
```

- [ ] **Step 3: リアクションバッジにホバーハンドラを追加**

`frontend/src/ui/NoteCard.svelte:248-254` (`.reaction` ボタン) を以下に変更:

```svelte
            <button
              class="reaction"
              class:mine={inner.myReaction === key}
              disabled={!accountId || isRemoteCustomEmoji(key)}
              title={isRemoteCustomEmoji(key) ? "このインスタンスに無い絵文字のためリアクションできません" : undefined}
              onclick={() => react(key)}
              onmouseenter={(e) => enterHover({ kind: "reaction", key }, e.currentTarget as HTMLElement)}
              onmouseleave={leaveHover}
            >
```

- [ ] **Step 4: Renote件数にホバーハンドラを追加**

`frontend/src/ui/NoteCard.svelte:272-275` を以下に変更:

```svelte
          {#if canRenote}
            <button title="Renote" onclick={doRenote}>
              <Repeat2 size={15} />
              {#if inner.renoteCount > 0}
                <span
                  onmouseenter={(e) => enterHover({ kind: "renote" }, e.currentTarget as HTMLElement)}
                  onmouseleave={leaveHover}
                >{inner.renoteCount}</span>
              {/if}
            </button>
```

- [ ] **Step 5: ポップオーバー本体を配置**

`frontend/src/ui/NoteCard.svelte:339` (`</article>` の直前) に追加:

```svelte
  {#if hoverTarget && hoverPos && accountId}
    <div
      use:portal
      style={`position:fixed; left:0; top:0;`}
      onmouseenter={keepHover}
      onmouseleave={leaveHover}
    >
      <ReactionUsersPopover
        {accountId}
        noteId={inner.id}
        reactionKey={hoverTarget.kind === "reaction" ? hoverTarget.key : null}
        totalCount={hoverTarget.kind === "reaction" ? (inner.reactions[hoverTarget.key] ?? 0) : inner.renoteCount}
        left={hoverPos.left}
        top={hoverPos.top}
      />
    </div>
  {/if}
```

- [ ] **Step 6: 型チェック**

Run: `cd frontend && pnpm check`
Expected: `NoteCard.svelte` / `ReactionUsersPopover.svelte` に関するエラーが無いこと

- [ ] **Step 7: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte
git commit -m "feat: リアクション・Renoteバッジにユーザー一覧ホバーを配線"
```

---

### Task 7: 手動確認

**Files:** なし（動作確認のみ）

- [ ] **Step 1: 開発サーバーを起動**

Run: `cargo tauri dev`
Expected: アプリが起動し、既存のカラムが表示される

- [ ] **Step 2: リアクション一覧の確認**

実際のMisskeyインスタンスでリアクションが付いたノートを表示し、リアクションバッジにマウスを乗せて約150ms待つ。
Expected: バッジ直下にユーザー一覧のポップオーバーが表示され、アバター・表示名・acctが並ぶ。マウスをバッジから外すとポップオーバーが消える。

- [ ] **Step 3: 複数絵文字の絞り込み確認**

同一ノートに複数種類のリアクションが付いている場合、それぞれのバッジにホバーして表示内容が別々の絵文字のユーザーになっていることを確認する。

- [ ] **Step 4: Renote一覧の確認**

Renoteされたノートで、Renote件数部分（アイコンではなく数字）にホバーし、Renoteしたユーザー一覧が表示されることを確認する。件数部分をクリックすると従来通りRenoteが実行されることも確認する。

- [ ] **Step 5: 既存操作の非破壊確認**

リアクションバッジをクリックして自分のリアクションがトグルされること、Renoteアイコンをクリックすると従来通りRenoteが実行されることを確認する（ポップオーバー追加によって既存のクリック挙動が壊れていないこと）。

- [ ] **Step 6: 型チェック最終確認**

Run: `cd frontend && pnpm check`
Expected: エラーなし

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS
