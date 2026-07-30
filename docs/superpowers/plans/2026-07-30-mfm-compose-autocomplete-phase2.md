# MFM補完(ComposeBar) Phase2実装計画: メンション/ハッシュタグ

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ComposeBar.svelte` の本文textareaで、メンション(`@user`/`@user@host`)とハッシュタグ(`#tag`)を、Misskey REST API(`users/search`/`hashtags/search`)を叩いてデバウンス付きで補完できるようにする。

**Architecture:** Phase 1で作った同期・純粋な補完基盤(`lib/mfmCompletion.ts`)にトリガー検出だけ追加し、APIを叩く非同期部分は新規 `lib/mfmSearch.ts` に分離する。バックエンドはRustに薄いAPIラッパー2本(`api/users.rs`/`api/hashtags.rs`)とTauriコマンド2本を追加するのみ。UI(`CompletionPopover.svelte`)はサムネイル種別を1つ増やすだけでほぼ無改修、`ComposeBar.svelte` にデバウンス付き非同期候補取得を追加する。

**Tech Stack:** Rust(Tauri v2 command)、TypeScript/Svelte 5、Vitest。新規npm/cratesクレート依存の追加なし(既存の `reqwest`/`serde_json` 等のみ使用)。

## Global Constraints

- 対象は `ComposeBar.svelte` の本文textareaのみ(Phase 1と同じ)。
- メンション検索は `origin: "combined"`(ローカル+リモート)。
- 検索デバウンスは300ms。
- 検索発火の最小クエリ長は1文字以上(`@`/`#`のみでは発火しない)。
- メンション挿入形式: ローカルは `@username`、リモートは `@username@host`。
- ローディング中はポップアップを表示しない。検索失敗時は黙って0件扱い(エラー表示なし)。
- キー操作(↑/↓移動・矢印キーで明示的に選ぶまでEnterでは確定しない・Tab/クリックは常に確定・Escapeで閉じる・Ctrl+Enter最優先)はPhase 1と同じ挙動を維持する。
- 新規npm/cratesパッケージを追加しない。
- 設計書: `docs/superpowers/specs/2026-07-30-mfm-compose-autocomplete-phase2-design.md`。

---

## Task 1: `api/users.rs` / `api/hashtags.rs` — 検索REST(Rust)

**Files:**
- Create: `src-tauri/src/api/users.rs`
- Create: `src-tauri/src/api/hashtags.rs`
- Modify: `src-tauri/src/api/mod.rs`

**Interfaces:**
- Consumes: `crate::api::normalize::RawUser`、`crate::api::MisskeyClient`(`post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>`、`src-tauri/src/api/client.rs:47`)、`crate::domain::User`、`crate::error::Result`。
- Produces: `pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>>`、`pub async fn search_hashtags(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<String>>`

薄いラッパーで専用の自動テストは追加しない(`RawUser`→`User`変換自体は既存の`From`実装で担保済み。`src-tauri/src/api/notes.rs`の`fetch_notes`と同水準の薄さ)。

- [ ] **Step 1: `api/users.rs` を作成する**

```rust
//! ユーザー検索 REST（メンション補完用）。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
use serde_json::json;

pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>> {
    let body = json!({
        "query": query,
        "limit": limit,
        "origin": "combined",
        "detail": false,
    });
    let raw: Vec<RawUser> = client.post("users/search", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}
```

- [ ] **Step 2: `api/hashtags.rs` を作成する**

```rust
//! ハッシュタグ検索 REST（ハッシュタグ補完用）。

use crate::api::MisskeyClient;
use crate::error::Result;
use serde_json::json;

pub async fn search_hashtags(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<String>> {
    let body = json!({
        "query": query,
        "limit": limit,
    });
    client.post("hashtags/search", &body).await
}
```

- [ ] **Step 3: `api/mod.rs` にモジュールを登録する**

`src-tauri/src/api/mod.rs` の `pub mod notifications;` の下に追記:

```rust
pub mod hashtags;
pub mod users;
```

(アルファベット順に並んでいる既存の並びに合わせ、`clips`/`drive`/`meta`/`mutes`/`notes`/`notifications`の並びの中で`hashtags`は`drive`と`meta`の間、`users`は`notifications`と`normalize`の間に挿入するのが厳密なアルファベット順だが、既存ファイルも完全なアルファベット順ではない(`normalize`が最後)ため、追加2行を`notifications`の直後に置けば十分。既存の並び順を壊さないことを優先する。)

- [ ] **Step 4: ビルド確認**

Run: `cd src-tauri && cargo build`
Expected: エラーなくコンパイルが通る(この時点ではまだどこからも呼ばれていないため `unused` warning が出ても許容 — Task 2でコマンドから呼ばれれば解消する)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/users.rs src-tauri/src/api/hashtags.rs src-tauri/src/api/mod.rs
git commit -m "feat: ユーザー検索/ハッシュタグ検索のRESTラッパーを追加"
```

---

## Task 2: Tauriコマンド登録とTSバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/note.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `api::users::search_users`、`api::hashtags::search_hashtags`(Task 1)、`crate::state::AppState`(`state.client_for(&account_id) -> Result<MisskeyClient>`、既存パターン、`src-tauri/src/commands/note.rs:29`)。
- Produces: Tauriコマンド `search_users(account_id: String, query: String) -> Result<Vec<User>, Error>`、`search_hashtags(account_id: String, query: String) -> Result<Vec<String>, Error>`(フロントエンドからは `commands.searchUsers`/`commands.searchHashtags` として camelCase で呼べる)。

- [ ] **Step 1: `commands/note.rs` の先頭importに追記する**

既存の `use crate::api::notes::{ ... };` の下に追記:

```rust
use crate::api::hashtags::search_hashtags as api_search_hashtags;
use crate::api::users::search_users as api_search_users;
```

- [ ] **Step 2: コマンドを追加する**

ファイル末尾に追記:

```rust
/// メンション補完用のユーザー検索。
#[tauri::command]
#[specta::specta]
pub async fn search_users(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<User>> {
    let client = state.client_for(&account_id)?;
    api_search_users(&client, &query, 10).await
}

/// ハッシュタグ補完用のハッシュタグ検索。
#[tauri::command]
#[specta::specta]
pub async fn search_hashtags(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
) -> Result<Vec<String>> {
    let client = state.client_for(&account_id)?;
    api_search_hashtags(&client, &query, 10).await
}
```

(`User` は `commands/note.rs` の先頭で既に `crate::domain::{..., User}` としてimport済み。`Result` は `crate::error::Result` のエイリアスとして既にimport済み。)

- [ ] **Step 3: `lib.rs` の `specta_builder()` に登録する**

`src-tauri/src/lib.rs` の `commands::note::read_attachment_preview,` の下に追記:

```rust
            commands::note::search_users,
            commands::note::search_hashtags,
```

- [ ] **Step 4: TSバインディングを再生成して確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `searchUsers`/`searchHashtags` が生成される。

- [ ] **Step 5: 生成結果を確認する**

Run: `grep -n "searchUsers\|searchHashtags" frontend/src/bindings/tauri.gen.ts`
Expected: 両コマンドの呼び出し関数が出力される(`export const commands = { ... searchUsers: (accountId, query) => ..., searchHashtags: (accountId, query) => ... }` 相当)

- [ ] **Step 6: Rust側の全体テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: 既存テストを含め全PASS(実Misskey接続が要る `#[ignore]` テストは対象外のまま)

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: search_users/search_hashtagsコマンドを追加しTSバインディングを再生成"
```

---

## Task 3: `lib/mfmCompletion.ts` — メンション/ハッシュタグのトリガー検出

**Files:**
- Modify: `frontend/src/lib/mfmCompletion.ts`
- Modify: `frontend/src/lib/mfmCompletion.test.ts`

**Interfaces:**
- Produces:
  - `Trigger` に `{ kind: "mention"; query: string; start: number; end: number }` と `{ kind: "hashtag"; query: string; start: number; end: number }` を追加
  - `export type SyncTrigger = Exclude<Trigger, { kind: "mention" } | { kind: "hashtag" }>`
  - `buildCompletionItems(trigger: SyncTrigger, customEmojis: EmojiDef[]): CompletionItem[]`(引数型のみ変更、実装ロジックは変更なし)
  - `CompletionThumbnail` の `type` に `"avatar"` を追加

### Cycle A: トリガー検出

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mfmCompletion.test.ts` の `describe("detectTrigger", ...)` ブロック内に追記:

```ts
it("detects a mention trigger at the start of the text", () => {
  expect(detectTrigger("@ali", 4)).toEqual({ kind: "mention", query: "ali", start: 0, end: 4 });
});

it("detects a mention trigger with a host part as one trigger", () => {
  expect(detectTrigger("hello @alice@example.com", 25)).toEqual({
    kind: "mention", query: "alice@example.com", start: 6, end: 25,
  });
});

it("does not treat an email-address-like '@' as a mention trigger", () => {
  // "user@" の直前が英数字("r")なので境界外(誤検出しない)
  expect(detectTrigger("user@example.com", 16)).toBeNull();
});

it("detects a hashtag trigger", () => {
  expect(detectTrigger("hello #misskey", 14)).toEqual({
    kind: "hashtag", query: "misskey", start: 6, end: 14,
  });
});

it("does not treat a '#' glued to a word as a hashtag trigger", () => {
  expect(detectTrigger("C#lang", 6)).toBeNull();
});

it("still detects a hashtag trigger inside an fn's content (after whitespace)", () => {
  expect(detectTrigger("$[tada hi #tag", 14)).toEqual({
    kind: "hashtag", query: "tag", start: 10, end: 14,
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: FAIL — `mention`/`hashtag` を検出できず `null` が返る

- [ ] **Step 3: 実装する**

`frontend/src/lib/mfmCompletion.ts` の `Trigger` 型に2行追加:

```ts
export type Trigger =
  | { kind: "emoji"; query: string; start: number; end: number }
  | { kind: "fnName"; query: string; start: number; end: number }
  | { kind: "argName"; fnName: string; query: string; start: number; end: number }
  | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number }
  | { kind: "mention"; query: string; start: number; end: number }
  | { kind: "hashtag"; query: string; start: number; end: number };
```

`EMOJI_TRIGGER` の定義の下に追記:

```ts
const MENTION_TRIGGER = /(?:^|[\s([{"'>])(@[a-zA-Z0-9_-]+(?:@[a-zA-Z0-9_.-]+)?)$/;
const HASHTAG_TRIGGER = /(?:^|[\s([{"'>])(#\S+)$/;
```

`detectEmojiTrigger` の下に追記:

```ts
function detectMentionTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(MENTION_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // "@query"
  return { kind: "mention", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}

function detectHashtagTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(HASHTAG_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // "#query"
  return { kind: "hashtag", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}
```

`detectTrigger` を置き換える:

```ts
export function detectTrigger(text: string, cursor: number): Trigger | null {
  return (
    detectFnTrigger(text, cursor) ??
    detectEmojiTrigger(text, cursor) ??
    detectMentionTrigger(text, cursor) ??
    detectHashtagTrigger(text, cursor)
  );
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfmCompletion.ts frontend/src/lib/mfmCompletion.test.ts
git commit -m "feat: メンション/ハッシュタグのトリガー検出(detectTrigger)を追加"
```

### Cycle B: `CompletionThumbnail`拡張と`buildCompletionItems`の型narrowing

このCycleはランタイムの挙動を一切変えない型レベルの変更のみ(`CompletionThumbnail`に`"avatar"`という取りうる値を1つ増やす、`buildCompletionItems`の引数型を`Trigger`から`SyncTrigger`に絞る)。Vitestは型を検査しないため、TDDのRed/Greenサイクルではなく実装 → `pnpm check`での型チェックのみで検証する(新しいテストは追加しない。既存のテストが変更後も全PASSすることが回帰確認になる)。

- [ ] **Step 1: 実装する**

`CompletionThumbnail` を変更:

```ts
export interface CompletionThumbnail {
  type: "custom" | "unicode" | "avatar";
  url?: string;
  char?: string;
}
```

`buildCompletionItems` の直前に型エイリアスを追加し、シグネチャを変更する:

```ts
export type SyncTrigger = Exclude<Trigger, { kind: "mention" } | { kind: "hashtag" }>;

export function buildCompletionItems(trigger: SyncTrigger, customEmojis: EmojiDef[]): CompletionItem[] {
```

(関数の中身・`switch`文は変更しない。)

- [ ] **Step 2: 型チェックとテストを実行する**

Run: `cd frontend && pnpm check`
Expected: 0 errors(`buildCompletionItems` の呼び出し元は `ComposeBar.svelte` のみで、Task 6でそちらも `SyncTrigger` に整合する形に更新するため、Task 6完了までは一時的に型エラーが出ても構わない。もし今の時点で `ComposeBar.svelte` 側の型エラーが出た場合は、そのエラー内容だけ確認してこのタスクでは無視してよい — Task 6で解消する)

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: PASS

- [ ] **Step 3: コミット**

```bash
git add frontend/src/lib/mfmCompletion.ts frontend/src/lib/mfmCompletion.test.ts
git commit -m "feat: CompletionThumbnailにavatar種別を追加しbuildCompletionItemsの引数型をSyncTriggerに限定"
```

---

## Task 4: `lib/mfmSearch.ts` — IPC呼び出しと`CompletionItem`変換

**Files:**
- Create: `frontend/src/lib/mfmSearch.ts`
- Create: `frontend/src/lib/mfmSearch.test.ts`

**Interfaces:**
- Consumes: `commands.searchUsers`/`commands.searchHashtags`/`unwrap` from `./ipc`(Task 2で生成)。`CompletionItem` type from `./mfmCompletion`(Task 3)。
- Produces: `export async function searchMentionItems(accountId: string, query: string): Promise<CompletionItem[]>`、`export async function searchHashtagItems(accountId: string, query: string): Promise<CompletionItem[]>`

- [ ] **Step 1: 失敗するテストを書く**

Create `frontend/src/lib/mfmSearch.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({
  commands: {
    searchUsers: vi.fn(),
    searchHashtags: vi.fn(),
  },
  unwrap: async <T>(p: Promise<{ status: "ok"; data: T } | { status: "error"; error: unknown }>) => {
    const r = await p;
    if (r.status === "ok") return r.data;
    throw new Error("unwrap failed in test");
  },
}));

import { commands } from "./ipc";
import { searchHashtagItems, searchMentionItems } from "./mfmSearch";

function user(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    id: "1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: "https://example.com/a.png",
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    emojis: {},
    ...overrides,
  };
}

describe("searchMentionItems", () => {
  it("maps a local user to @username with an avatar thumbnail", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({ status: "ok", data: [user()] } as never);
    const items = await searchMentionItems("acc1", "ali");
    expect(items).toEqual([
      {
        key: "user:1",
        label: "@alice",
        insertText: "@alice",
        thumbnail: { type: "avatar", url: "https://example.com/a.png" },
      },
    ]);
    expect(commands.searchUsers).toHaveBeenCalledWith("acc1", "ali");
  });

  it("maps a remote user to @username@host", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({
      status: "ok",
      data: [user({ id: "2", username: "bob", host: "example.com" })],
    } as never);
    const items = await searchMentionItems("acc1", "bob");
    expect(items[0]).toMatchObject({ key: "user:2", label: "@bob@example.com", insertText: "@bob@example.com" });
  });

  it("omits the thumbnail when the user has no avatar", async () => {
    vi.mocked(commands.searchUsers).mockResolvedValue({
      status: "ok",
      data: [user({ avatarUrl: null })],
    } as never);
    const items = await searchMentionItems("acc1", "ali");
    expect(items[0].thumbnail).toBeUndefined();
  });
});

describe("searchHashtagItems", () => {
  it("maps tag strings to #tag items", async () => {
    vi.mocked(commands.searchHashtags).mockResolvedValue({
      status: "ok",
      data: ["misskey", "tsumugi"],
    } as never);
    const items = await searchHashtagItems("acc1", "mi");
    expect(items).toEqual([
      { key: "tag:misskey", label: "#misskey", insertText: "#misskey" },
      { key: "tag:tsumugi", label: "#tsumugi", insertText: "#tsumugi" },
    ]);
    expect(commands.searchHashtags).toHaveBeenCalledWith("acc1", "mi");
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmSearch.test.ts`
Expected: FAIL — `./mfmSearch` が存在しない

- [ ] **Step 3: 実装する**

Create `frontend/src/lib/mfmSearch.ts`:

```ts
// メンション/ハッシュタグ補完のIPC呼び出し + CompletionItem変換。
// mfmCompletion.ts は DOM非依存の純粋関数のみという責務を保つため、
// 副作用(IPC呼び出し)を持つこのロジックは別ファイルに分離する。
import { commands, unwrap } from "./ipc";
import type { CompletionItem } from "./mfmCompletion";

export async function searchMentionItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const users = await unwrap(commands.searchUsers(accountId, query));
  return users.map((u) => {
    const acct = u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
    return {
      key: `user:${u.id}`,
      label: acct,
      insertText: acct,
      thumbnail: u.avatarUrl ? { type: "avatar" as const, url: u.avatarUrl } : undefined,
    };
  });
}

export async function searchHashtagItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const tags = await unwrap(commands.searchHashtags(accountId, query));
  return tags.map((tag) => ({ key: `tag:${tag}`, label: `#${tag}`, insertText: `#${tag}` }));
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmSearch.test.ts`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfmSearch.ts frontend/src/lib/mfmSearch.test.ts
git commit -m "feat: メンション/ハッシュタグ検索のIPC呼び出し(mfmSearch)を追加"
```

---

## Task 5: `CompletionPopover.svelte` — アバターサムネイル対応

**Files:**
- Modify: `frontend/src/ui/CompletionPopover.svelte`
- Modify: `frontend/src/ui/CompletionPopover.test.ts`

**Interfaces:**
- Consumes: `CompletionThumbnail`(Task 3で `"avatar"` を追加済み)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/CompletionPopover.test.ts` に追記(既存の `emojiItem`/`unicodeItem`/`textItem` 定義の下):

```ts
const avatarItem: CompletionItem = {
  key: "user:1",
  label: "@alice",
  insertText: "@alice",
  thumbnail: { type: "avatar", url: "https://example.com/avatar.png" },
};
```

`describe("CompletionPopover", ...)` 内に追記:

```ts
it("renders a thumbnail image for an avatar item", () => {
  const { getByRole } = render(CompletionPopover, {
    props: { items: [avatarItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
  });
  expect(getByRole("img").getAttribute("src")).toBe("https://example.com/avatar.png");
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/CompletionPopover.test.ts`
Expected: FAIL — アバター種別が描画されず `img` が見つからない

- [ ] **Step 3: 実装する**

`frontend/src/ui/CompletionPopover.svelte` の描画分岐を変更:

```svelte
      {#if item.thumbnail?.type === "custom" || item.thumbnail?.type === "avatar"}
        <img class="completion-thumb" src={item.thumbnail.url} alt="" />
      {:else if item.thumbnail?.type === "unicode"}
        <span class="completion-thumb completion-thumb-unicode">{item.thumbnail.char}</span>
      {/if}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/CompletionPopover.test.ts`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/CompletionPopover.svelte frontend/src/ui/CompletionPopover.test.ts
git commit -m "feat: CompletionPopoverでavatarサムネイルをcustomと同じimg描画にする"
```

---

## Task 6: `ComposeBar.svelte` への配線(非同期候補取得)

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: `searchMentionItems`/`searchHashtagItems` from `../lib/mfmSearch`(Task 4)。`buildCompletionItems`(引数型が `SyncTrigger` になった、Task 3)。既存の `trigger`/`candidates`/`selectedIndex`/`selectionMoved`/`popoverOpen`/`popoverPos` などPhase 1の状態一式(そのまま流用)。

Phase 1同様、`ComposeBar.svelte` 自体には新規の自動テストを追加しない(非同期・デバウンス・グローバルストア一式のモックコストが不釣り合いなため。ロジックはTask 3/4の純粋関数・IPCラッパーで網羅テスト済み)。`cargo tauri dev` での手動確認で検証する。

- [ ] **Step 1: importを追加する**

`frontend/src/ui/ComposeBar.svelte` の既存import群に追加:

```ts
import { searchHashtagItems, searchMentionItems } from "../lib/mfmSearch";
```

- [ ] **Step 2: 非同期候補の状態を追加する**

既存の `let selectedIndex = $state(0);` `let selectionMoved = $state(false);` の下に追加:

```ts
  let asyncCandidates = $state<CompletionItem[]>([]);
  let asyncSearchToken = 0; // 古い応答を無視するための世代カウンタ
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
```

(`CompletionItem` 型は既にimport済みの前提。importされていなければ `import type { CompletionItem, ... } from "../lib/mfmCompletion";` の型一覧に追加する。)

- [ ] **Step 3: デバウンス付き検索の`$effect`を追加する**

既存の `candidates` を計算している `$derived` の直前に追加:

```ts
  $effect(() => {
    const t = trigger;
    clearTimeout(debounceTimer);
    if (!t || (t.kind !== "mention" && t.kind !== "hashtag") || t.query.length < 1) {
      asyncCandidates = [];
      return;
    }
    const token = ++asyncSearchToken;
    debounceTimer = setTimeout(async () => {
      if (!accountId) return;
      try {
        const items =
          t.kind === "mention" ? await searchMentionItems(accountId, t.query) : await searchHashtagItems(accountId, t.query);
        if (token === asyncSearchToken) asyncCandidates = items;
      } catch {
        if (token === asyncSearchToken) asyncCandidates = [];
      }
    }, 300);
  });
```

- [ ] **Step 4: `candidates` の`$derived`をトリガー種別で分岐させる**

既存の `const candidates = $derived<CompletionItem[]>(trigger ? buildCompletionItems(trigger, customEmojiList) : []);` を置き換える:

```ts
  const candidates = $derived<CompletionItem[]>(
    !trigger
      ? []
      : trigger.kind === "mention" || trigger.kind === "hashtag"
        ? asyncCandidates
        : buildCompletionItems(trigger, customEmojiList),
  );
```

- [ ] **Step 5: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: 0 errors(Task 3で `buildCompletionItems` の引数型を `SyncTrigger` にしたことによる型エラーが、このStepの分岐により解消されていることを確認する — `trigger.kind === "mention" || trigger.kind === "hashtag"` のelse節でTypeScriptが `trigger` を自動的に `SyncTrigger` に絞り込む)

- [ ] **Step 6: 単体テストを一括実行する(回帰確認)**

Run: `cd frontend && pnpm vitest run`
Expected: 既存分含め全PASS

- [ ] **Step 7: 手動確認する**

```bash
cargo tauri dev
```

ComposeBarの本文欄で以下を確認する:

1. `@ali` のように入力 → 300ms程度の間を置いてユーザー候補(アバター画像+`@username`)が出る。連合先ユーザーは `@username@host` 表示。
2. `@a`→`@al`→`@ali` と素早く連続入力しても、最終的な入力内容に対応した検索結果だけが表示される(古い応答による上書きが起きない)。
3. `#mis` のように入力 → ハッシュタグ候補(`#`付きテキストのみ、アバターなし)が出る。
4. `@`/`#` だけ入力した直後は検索が発火しない(1文字目を打った時点で発火する)。
5. ↑/↓で選択→Tab/Enterで確定、または未選択のままTab/クリックで確定 → `@username`/`#tag` が本文に挿入される。矢印キーで選ばずにEnterを押すと改行される(Phase 1と同じ挙動)。
6. 存在しないユーザー名/タグを入力した場合、候補0件のままポップアップが出ない(エラー表示が出ないこと)。
7. `$[tada hi @user` のように、MFM関数の内容部分(空白の後)でもメンショントリガーが効くこと。
8. ネットワークを切断するなどして検索が失敗するケースでも、アプリがクラッシュしたりエラーモーダルが出たりしないこと(黙って候補なし扱いになること)。

問題が見つかった場合はこのタスク内で修正し、再度手動確認する。

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: ComposeBarにメンション/ハッシュタグの非同期補完を配線"
```

---

## Self-Review Notes

- **Spec coverage:** メンション/ハッシュタグ両トリガーの検出(Task 3)、検索API・コマンド(Task 1-2)、非同期変換(Task 4)、UI表示(Task 5)、デバウンス・世代管理・エラー時の黙殺(Task 6)を各タスクでカバー。`origin: combined`・デバウンス300ms・最小クエリ長1文字・挿入形式・ローディング/エラー時の非表示は、いずれもGlobal Constraints・各タスクのコードに明記済み。
- **Placeholder scan:** 各Stepのコードは完全な実装/テストコードで、TODOや「後で実装」は無い。
- **Type consistency:** `Trigger`(Task 3で`mention`/`hashtag`追加)→`SyncTrigger`(Task 3、`buildCompletionItems`の引数型)→`CompletionItem`/`CompletionThumbnail`(Task 3で`avatar`追加、Task 4・5・6で一貫して使用)の型はタスク間で一貫している。`ComposeBar.svelte`側の`trigger.kind==="mention"||"hashtag"`分岐によるTypeScriptの型絞り込みがTask 3の型変更と整合することをTask 6 Step 5で明示的に確認する手順にした。
