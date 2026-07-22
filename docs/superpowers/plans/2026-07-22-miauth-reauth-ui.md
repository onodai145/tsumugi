# MiAuth 再認証 UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a manual "再認証" entry point in Settings→アカウント and an automatic one triggered from 403 (Forbidden) errors in the Backstage log, both routing into a reauth mode of `AddAccount.svelte` that skips host entry and reuses the existing `account.id`-preserving backend (PR #104).

**Architecture:** All backend logic is already in place (`complete_miauth` reuses `account.id` when `host`+`userId` match an existing account — see `src-tauri/src/commands/account.rs`). This plan is frontend-only: (1) a typed `ForbiddenError`/`unwrapAcc` helper in `lib/ipc.ts` that lets the store detect 403s without string-matching, (2) a `reauthAccountId` field on `LogEntry` so Backstage can render an action button, (3) a `reauthAccount` prop on `AddAccount.svelte` that skips the host-input step, and (4) wiring in `App.svelte`/`Settings.svelte`/`AccountsSection.svelte`/`Backstage.svelte` to reach that prop from both entry points.

**Tech Stack:** Svelte 5 (runes), TypeScript, Tauri v2 commands via generated bindings (`frontend/src/bindings/tauri.gen.ts`).

## Global Constraints

- Backend (`src-tauri`) is not touched by this plan — `account.id` preservation on reauth is already implemented and tested in PR #104.
- No frontend test runner exists in this repo (`frontend/package.json` only has `pnpm check` = `svelte-check` + `tsc`). Verification for every frontend task is `pnpm check` passing, plus a final manual walkthrough via `cargo tauri dev` (never `cargo run` directly — see root `CLAUDE.md`).
- Error `kind` strings come from `src-tauri/src/error.rs`'s `#[serde(tag = "kind", ...)]` — the Forbidden variant serializes as `kind: "forbidden"` (camelCase via `rename_all`). Use this exact string, not the old regex (`/PERMISSION_DENIED|forbidden/i`) it replaces.
- Existing UI copy/behavior for non-reauth flows (plain "アカウントを追加") must not change.
- Follow existing code style: no comments explaining *what* the code does, only non-obvious *why* (matches this repo's existing files).

---

### Task 1: `unwrapAcc` + `ForbiddenError` in `lib/ipc.ts`

**Files:**
- Modify: `frontend/src/lib/ipc.ts`

**Interfaces:**
- Produces: `export class ForbiddenError extends Error { accountId: string }`, `export async function unwrapAcc<T>(accountId: string, p: Promise<Result<T>>): Promise<T>`

- [ ] **Step 1: Add `ForbiddenError` and `unwrapAcc` to `ipc.ts`**

Replace the full contents of `frontend/src/lib/ipc.ts` with:

```ts
// Tauri command のラッパ。生成された bindings の Result を unwrap し、
// 失敗時は型付き Error を Error オブジェクトに変換して throw する。
import { commands, type Error as ApiError } from "../bindings/tauri.gen";

export { commands };
export * from "../bindings/tauri.gen";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: ApiError };

export async function unwrap<T>(p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  throw new Error(formatError(r.error));
}

/// 403 (kind: "forbidden") を ForbiddenError として throw する unwrap。
/// 呼び出し元は accountId を渡し、store 側で「再認証」アクションをログに出せるようにする。
export class ForbiddenError extends Error {
  accountId: string;
  constructor(accountId: string, message: string) {
    super(message);
    this.name = "ForbiddenError";
    this.accountId = accountId;
  }
}

export async function unwrapAcc<T>(accountId: string, p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  if (r.error.kind === "forbidden") throw new ForbiddenError(accountId, formatError(r.error));
  throw new Error(formatError(r.error));
}

export function formatError(e: ApiError): string {
  return "message" in e ? `${e.kind}: ${e.message}` : e.kind;
}
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes with no new errors (nothing consumes `unwrapAcc`/`ForbiddenError` yet, so this is purely additive).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/ipc.ts
git commit -m "feat: 403検知用のForbiddenError/unwrapAccを追加"
```

---

### Task 2: `LogEntry.reauthAccountId` + `#fail`/`#log` support + `#syncServerMutes` migration

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:3` (import), `:101-106` (`LogEntry`), `:345-362` (`#log`/`#fail`), `:809-826` (`#syncServerMutes`)

**Interfaces:**
- Consumes: `ForbiddenError`, `unwrapAcc` from `./ipc` (Task 1)
- Produces: `LogEntry.reauthAccountId?: string`; `#fail(e: unknown)` now sets `reauthAccountId` on the logged entry when `e instanceof ForbiddenError`. Every later task that routes an error through `this.#fail(e)` picks this up automatically — no per-call-site changes needed for the log-tagging part.

- [ ] **Step 1: Import `ForbiddenError`/`unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:3`, change:

```ts
import { commands, events, unwrap, formatError } from "./ipc";
```

to:

```ts
import { commands, events, unwrap, unwrapAcc, formatError, ForbiddenError } from "./ipc";
```

- [ ] **Step 2: Add `reauthAccountId` to `LogEntry`**

In `frontend/src/lib/store.svelte.ts:100-106`, change:

```ts
/// Backstage（操作ログ/エラー）の1エントリ。
export interface LogEntry {
  id: number;
  at: number; // epoch ms
  level: LogLevel;
  text: string;
}
```

to:

```ts
/// Backstage（操作ログ/エラー）の1エントリ。
export interface LogEntry {
  id: number;
  at: number; // epoch ms
  level: LogLevel;
  text: string;
  reauthAccountId?: string; // 403由来のエラーなら、再認証を促すボタンをBackstageが描画する
}
```

- [ ] **Step 3: Extend `#log` to accept `reauthAccountId`, update `#fail` to detect `ForbiddenError`**

In `frontend/src/lib/store.svelte.ts:345-362`, change:

```ts
  #log(level: LogLevel, text: string) {
    this.logs = [{ id: ++this.#logSeq, at: Date.now(), level, text }, ...this.logs].slice(
      0,
      AppStore.#LOG_CAP,
    );
    if (this.ui.enableFileLogging) void commands.logFrontendEvent(AppStore.#RUST_LEVEL[level], text);
  }
  /// Backstage(UI)には出さず、設定でONならRust側のファイルログにのみ書く
  /// (Issue #12調査用の詳細ログ。debugレベルなのでBackstageの通常ログよりファイル側でのみ見える)。
  #logDebug(text: string) {
    if (this.ui.enableFileLogging) void commands.logFrontendEvent("debug", text);
  }
  /// エラーをバナー表示＋Backstage へ記録する共通処理。
  #fail(e: unknown) {
    const msg = String(e);
    this.error = msg;
    this.#log("error", msg);
  }
```

to:

```ts
  #log(level: LogLevel, text: string, reauthAccountId?: string) {
    this.logs = [
      { id: ++this.#logSeq, at: Date.now(), level, text, reauthAccountId },
      ...this.logs,
    ].slice(0, AppStore.#LOG_CAP);
    if (this.ui.enableFileLogging) void commands.logFrontendEvent(AppStore.#RUST_LEVEL[level], text);
  }
  /// Backstage(UI)には出さず、設定でONならRust側のファイルログにのみ書く
  /// (Issue #12調査用の詳細ログ。debugレベルなのでBackstageの通常ログよりファイル側でのみ見える)。
  #logDebug(text: string) {
    if (this.ui.enableFileLogging) void commands.logFrontendEvent("debug", text);
  }
  /// エラーをバナー表示＋Backstage へ記録する共通処理。
  /// ForbiddenError なら「再認証」アクションをログ行に付与する。
  #fail(e: unknown) {
    const msg = String(e);
    this.error = msg;
    this.#log("error", msg, e instanceof ForbiddenError ? e.accountId : undefined);
  }
```

- [ ] **Step 4: Replace the regex-based 403 check in `#syncServerMutes` with `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:809-826`, change:

```ts
  /// サーバ側ミュート/ブロックを同期（失敗しても致命的でないのでログのみ）。
  async #syncServerMutes(accountId: string) {
    try {
      const n = await unwrap(commands.syncServerMutes(accountId));
      if (n > 0) this.#log("info", `サーバのミュート/ブロックを同期: ${n}件`);
    } catch (e) {
      const msg = String(e);
      // 旧トークンは read:mutes/read:blocks 権限が無い → 再認可が必要
      if (/PERMISSION_DENIED|forbidden/i.test(msg)) {
        this.#log(
          "warn",
          "サーバミュート同期: 権限不足。設定→アカウントで一度削除し再追加すると反映されます",
        );
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${msg}`);
      }
    }
  }
```

to:

```ts
  /// サーバ側ミュート/ブロックを同期（失敗しても致命的でないのでログのみ）。
  async #syncServerMutes(accountId: string) {
    try {
      const n = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (n > 0) this.#log("info", `サーバのミュート/ブロックを同期: ${n}件`);
    } catch (e) {
      if (e instanceof ForbiddenError) {
        this.#log("warn", "サーバミュート同期: 権限不足。再認証してください", e.accountId);
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${String(e)}`);
      }
    }
  }
```

- [ ] **Step 5: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes. `unwrap` is still used elsewhere in the file so the import stays valid.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: LogEntryに再認証アクションを持たせる"
```

---

### Task 3: Migrate remaining `accountId` call sites to `unwrapAcc`

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts` — `postNote` (:1179-1182), `renote` (:1184-1191), `deleteNote` (:1193-1197), `toggleReaction` (:1229-1251), `toggleFavorite` (:1253-1279), `listClips` (:1281-1288), `createClip` (:1290-1294), `addNoteToClip` (:1296-1303), `votePoll` (:1305-1318), `addColumn` (:839-855), `fetchUserLists` (:890-892), `fetchAntennas` (:894-896), `fetchChannels` (:898-900), `resolveUser` (:902-905)

**Interfaces:**
- Consumes: `unwrapAcc`, `ForbiddenError` (Task 1), `#fail` with automatic `reauthAccountId` tagging (Task 2)
- Produces: no new interfaces — all of these keep their existing signatures and return types. Every function that previously had no `try/catch` now catches, calls `this.#fail(e)` (which logs to Backstage with the reauth action when applicable), and rethrows so callers' existing local error handling (e.g. `ComposeBar.svelte`'s `catch (e) { err = String(e); }`) is unaffected.

- [ ] **Step 1: `postNote` — wrap and use `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1179-1182`, change:

```ts
  async postNote(accountId: string, draft: NoteDraft) {
    await unwrap(commands.postNote(accountId, draft));
    this.#log("success", "投稿しました");
  }
```

to:

```ts
  async postNote(accountId: string, draft: NoteDraft) {
    try {
      await unwrapAcc(accountId, commands.postNote(accountId, draft));
      this.#log("success", "投稿しました");
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 2: `renote` — swap `unwrap` for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1184-1191`, change:

```ts
  async renote(accountId: string, noteId: string, visibility: VisibilityInput = "public") {
    try {
      await unwrap(commands.renote(accountId, noteId, visibility));
      this.#log("success", "Renote しました");
    } catch (e) {
      this.#fail(e);
    }
  }
```

to:

```ts
  async renote(accountId: string, noteId: string, visibility: VisibilityInput = "public") {
    try {
      await unwrapAcc(accountId, commands.renote(accountId, noteId, visibility));
      this.#log("success", "Renote しました");
    } catch (e) {
      this.#fail(e);
    }
  }
```

- [ ] **Step 3: `deleteNote` — wrap and use `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1193-1197`, change:

```ts
  async deleteNote(accountId: string, noteId: string) {
    await unwrap(commands.deleteNoteCmd(accountId, noteId));
    for (const t of this.#allTabs()) t.notes = t.notes.filter((n) => n.id !== noteId);
    this.#log("info", "ノートを削除しました");
  }
```

to:

```ts
  async deleteNote(accountId: string, noteId: string) {
    try {
      await unwrapAcc(accountId, commands.deleteNoteCmd(accountId, noteId));
      for (const t of this.#allTabs()) t.notes = t.notes.filter((n) => n.id !== noteId);
      this.#log("info", "ノートを削除しました");
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 4: `toggleReaction` — swap the two `unwrap` calls for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1238-1250`, change:

```ts
    try {
      if (already === reaction) {
        await unwrap(commands.unreact(accountId, noteId));
        this.#log("info", "リアクションを取り消しました");
      } else {
        if (already) await unwrap(commands.unreact(accountId, noteId));
        await unwrap(commands.react(accountId, noteId, reaction));
        this.#log("success", `リアクション ${reaction}`);
      }
    } catch (e) {
      backups.forEach(restoreReaction);
      this.#fail(e);
    }
```

to:

```ts
    try {
      if (already === reaction) {
        await unwrapAcc(accountId, commands.unreact(accountId, noteId));
        this.#log("info", "リアクションを取り消しました");
      } else {
        if (already) await unwrapAcc(accountId, commands.unreact(accountId, noteId));
        await unwrapAcc(accountId, commands.react(accountId, noteId, reaction));
        this.#log("success", `リアクション ${reaction}`);
      }
    } catch (e) {
      backups.forEach(restoreReaction);
      this.#fail(e);
    }
```

- [ ] **Step 5: `toggleFavorite` — swap the two `unwrap` calls for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1260-1267`, change:

```ts
    try {
      if (already) {
        await unwrap(commands.unfavoriteNote(accountId, noteId));
        this.#log("info", "お気に入りを解除しました");
      } else {
        await unwrap(commands.favoriteNote(accountId, noteId));
        this.#log("success", "お気に入りに登録しました");
      }
    } catch (e) {
```

to:

```ts
    try {
      if (already) {
        await unwrapAcc(accountId, commands.unfavoriteNote(accountId, noteId));
        this.#log("info", "お気に入りを解除しました");
      } else {
        await unwrapAcc(accountId, commands.favoriteNote(accountId, noteId));
        this.#log("success", "お気に入りに登録しました");
      }
    } catch (e) {
```

(The rest of `toggleFavorite`'s `catch` block, including the `ALREADY_FAVORITED`/`NOT_FAVORITED` stale-state check, is unchanged — `ForbiddenError` doesn't match those message substrings so it falls through to `this.#fail(e)` as before.)

- [ ] **Step 6: `listClips` — swap `unwrap` for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1281-1288`, change:

```ts
  async listClips(accountId: string): Promise<Clip[]> {
    try {
      return await unwrap(commands.listClips(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

to:

```ts
  async listClips(accountId: string): Promise<Clip[]> {
    try {
      return await unwrapAcc(accountId, commands.listClips(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 7: `createClip` — wrap and use `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1290-1294`, change:

```ts
  async createClip(accountId: string, name: string): Promise<Clip> {
    const clip = await unwrap(commands.createClip(accountId, name));
    this.#log("success", `クリップを作成しました: ${clip.name}`);
    return clip;
  }
```

to:

```ts
  async createClip(accountId: string, name: string): Promise<Clip> {
    try {
      const clip = await unwrapAcc(accountId, commands.createClip(accountId, name));
      this.#log("success", `クリップを作成しました: ${clip.name}`);
      return clip;
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 8: `addNoteToClip` — swap `unwrap` for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1296-1303`, change:

```ts
  async addNoteToClip(accountId: string, clipId: string, noteId: string) {
    try {
      await unwrap(commands.addNoteToClip(accountId, clipId, noteId));
      this.#log("success", "クリップに追加しました");
    } catch (e) {
      this.#fail(e);
    }
  }
```

to:

```ts
  async addNoteToClip(accountId: string, clipId: string, noteId: string) {
    try {
      await unwrapAcc(accountId, commands.addNoteToClip(accountId, clipId, noteId));
      this.#log("success", "クリップに追加しました");
    } catch (e) {
      this.#fail(e);
    }
  }
```

- [ ] **Step 9: `votePoll` — swap `unwrap` for `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:1311-1317`, change:

```ts
    try {
      await unwrap(commands.votePoll(accountId, noteId, choice));
      this.#log("success", "投票しました");
    } catch (e) {
      backups.forEach(restorePoll);
      this.#fail(e);
    }
```

to:

```ts
    try {
      await unwrapAcc(accountId, commands.votePoll(accountId, noteId, choice));
      this.#log("success", "投票しました");
    } catch (e) {
      backups.forEach(restorePoll);
      this.#fail(e);
    }
```

- [ ] **Step 10: `addColumn` — wrap and use `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:838-855`, change:

```ts
  /// タブを追加する。`groupId` を指定するとそのカラムに、None なら新しいカラムを作る。
  async addColumn(
    accountId: string,
    kind: ColumnKind,
    filter: FilterQuery,
    groupId?: string,
    title?: string,
  ) {
    const opened = await unwrap(commands.addColumn(accountId, kind, filter, groupId ?? null));
    const tab = this.#insertTab(opened);
    const g = this.groups.find((x) => x.id === opened.group.id);
    if (g) g.activeTabId = tab.id;
    this.#captureInitial(opened.column.id, opened.notes);
    const name = title?.trim();
    if (name) await this.renameTab(tab.id, name);
    this.#log("success", `カラムを追加: ${name || kindLabel(kind)}`);
    return tab;
  }
```

to:

```ts
  /// タブを追加する。`groupId` を指定するとそのカラムに、None なら新しいカラムを作る。
  async addColumn(
    accountId: string,
    kind: ColumnKind,
    filter: FilterQuery,
    groupId?: string,
    title?: string,
  ) {
    try {
      const opened = await unwrapAcc(
        accountId,
        commands.addColumn(accountId, kind, filter, groupId ?? null),
      );
      const tab = this.#insertTab(opened);
      const g = this.groups.find((x) => x.id === opened.group.id);
      if (g) g.activeTabId = tab.id;
      this.#captureInitial(opened.column.id, opened.notes);
      const name = title?.trim();
      if (name) await this.renameTab(tab.id, name);
      this.#log("success", `カラムを追加: ${name || kindLabel(kind)}`);
      return tab;
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 11: `fetchUserLists`/`fetchAntennas`/`fetchChannels`/`resolveUser` — wrap and use `unwrapAcc`**

In `frontend/src/lib/store.svelte.ts:890-905`, change:

```ts
  async fetchUserLists(accountId: string) {
    return await unwrap(commands.listUserLists(accountId));
  }

  async fetchAntennas(accountId: string) {
    return await unwrap(commands.listAntennas(accountId));
  }

  async fetchChannels(accountId: string) {
    return await unwrap(commands.listChannels(accountId));
  }

  /// acct（@user@host）から userId を解決する。
  async resolveUser(accountId: string, acct: string) {
    return await unwrap(commands.resolveUserAcct(accountId, acct));
  }
```

to:

```ts
  async fetchUserLists(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listUserLists(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async fetchAntennas(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listAntennas(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async fetchChannels(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listChannels(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// acct（@user@host）から userId を解決する。
  async resolveUser(accountId: string, acct: string) {
    try {
      return await unwrapAcc(accountId, commands.resolveUserAcct(accountId, acct));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }
```

- [ ] **Step 12: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes. `unwrap` remains used elsewhere in the file (e.g. `boot`, `updateColumn`, `setUiPrefs`), so no unused-import error.

- [ ] **Step 13: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: accountId付きIPC呼び出しをunwrapAccに統一"
```

---

### Task 4: `completeAccount` — update in place on reauth, distinguish log wording

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:800-807`

**Interfaces:**
- Consumes: `this.accounts: Account[]` (existing field)
- Produces: no signature change to `completeAccount(sessionId: string): Promise<void>`

- [ ] **Step 1: Replace the push-only-if-absent logic with an upsert, and vary the log message**

In `frontend/src/lib/store.svelte.ts:800-807`, change:

```ts
  async completeAccount(sessionId: string) {
    const account = await unwrap(commands.completeMiauth(sessionId));
    if (!this.accounts.some((a) => a.id === account.id)) {
      this.accounts = [...this.accounts, account];
    }
    this.#log("success", `アカウントを追加: @${account.username}@${account.host}`);
    await this.#syncServerMutes(account.id);
  }
```

to:

```ts
  async completeAccount(sessionId: string) {
    const account = await unwrap(commands.completeMiauth(sessionId));
    const existingIdx = this.accounts.findIndex((a) => a.id === account.id);
    if (existingIdx === -1) {
      this.accounts = [...this.accounts, account];
      this.#log("success", `アカウントを追加: @${account.username}@${account.host}`);
    } else {
      this.accounts = this.accounts.map((a, i) => (i === existingIdx ? account : a));
      this.#log("success", `再認証しました: @${account.username}@${account.host}`);
    }
    await this.#syncServerMutes(account.id);
  }
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: 再認証時にaccount情報を上書きしログ文言を分ける"
```

---

### Task 5: `AddAccount.svelte` reauth mode + `App.svelte` wiring

**Files:**
- Modify: `frontend/src/ui/AddAccount.svelte` (full rewrite of `<script>` and template)
- Modify: `frontend/src/App.svelte:1-40` (imports/state), `:114-118` (main branch)

**Interfaces:**
- Consumes: `app.addAccount(host: string): Promise<string>`, `app.completeAccount(sessionId: string): Promise<void>` (existing store methods, unchanged)
- Produces: `AddAccount` prop `reauthAccount?: { id: string; host: string; username: string }`; `App.svelte` local state `reauthAccount = $state<Account | null>(null)` and function `startReauth(account: Account): void` — consumed by Task 6 and Task 7.

- [ ] **Step 1: Rewrite `AddAccount.svelte`'s `<script>` block**

In `frontend/src/ui/AddAccount.svelte:1-40`, change:

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";

  // onclose があれば「戻る」導線を出す（ログイン済みで設定経由で開いた場合）。
  // 初回（アカウント0件）は onclose 未指定で戻る先が無いため非表示。
  let { onclose }: { onclose?: () => void } = $props();

  let host = $state("");
  let sessionId = $state<string | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);

  async function start() {
    err = null;
    busy = true;
    try {
      sessionId = await app.addAccount(host.trim());
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function complete() {
    if (!sessionId) return;
    err = null;
    busy = true;
    try {
      await app.completeAccount(sessionId);
      sessionId = null;
      host = "";
      onclose?.(); // 追加できたらカラム表示へ戻る
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>
```

to:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";

  // onclose があれば「戻る」導線を出す（ログイン済みで設定経由で開いた場合）。
  // 初回（アカウント0件）は onclose 未指定で戻る先が無いため非表示。
  // reauthAccount があれば host入力を省略し、そのアカウントの再認証フローになる。
  let {
    onclose,
    reauthAccount,
  }: {
    onclose?: () => void;
    reauthAccount?: { id: string; host: string; username: string };
  } = $props();

  let host = $state(reauthAccount?.host ?? "");
  let sessionId = $state<string | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);

  onMount(() => {
    if (reauthAccount) void start();
  });

  async function start() {
    err = null;
    busy = true;
    try {
      sessionId = await app.addAccount(host.trim());
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function complete() {
    if (!sessionId) return;
    err = null;
    busy = true;
    try {
      await app.completeAccount(sessionId);
      sessionId = null;
      host = "";
      onclose?.(); // 完了できたらカラム表示/設定へ戻る
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>
```

- [ ] **Step 2: Rewrite the template to branch on `reauthAccount`**

In `frontend/src/ui/AddAccount.svelte` (the markup block right after the `</script>` tag), change:

```svelte
<div class="add-account">
  <div class="head">
    <h2>アカウントを追加</h2>
    {#if onclose}
      <button class="close" onclick={onclose} title="戻る"><X size={16} /></button>
    {/if}
  </div>
  {#if !sessionId}
    <p class="hint">Misskeyインスタンスのホスト名を入力してください（例: misskey.example）</p>
    <div class="form">
      <input
        placeholder="misskey.example"
        bind:value={host}
        onkeydown={(e) => e.key === "Enter" && host.trim() && start()}
      />
      <button disabled={busy || !host.trim()} onclick={start}>
        {busy ? "…" : "認可ページを開く"}
      </button>
    </div>
  {:else}
    <p class="hint">
      ブラウザで認可を完了したら、下のボタンを押してください。
    </p>
    <button class="primary" disabled={busy} onclick={complete}>
      {busy ? "確認中…" : "認可を完了した"}
    </button>
    <button class="link" onclick={() => (sessionId = null)}>やり直す</button>
  {/if}
  {#if err}<p class="err">{err}</p>{/if}
</div>
```

to:

```svelte
<div class="add-account">
  <div class="head">
    <h2>
      {reauthAccount
        ? `再認証: @${reauthAccount.username}@${reauthAccount.host}`
        : "アカウントを追加"}
    </h2>
    {#if onclose}
      <button class="close" onclick={onclose} title="戻る"><X size={16} /></button>
    {/if}
  </div>
  {#if reauthAccount && !sessionId}
    <p class="hint">{busy ? "認可ページを開いています…" : "認可ページを開けませんでした。"}</p>
    {#if !busy}
      <button class="link" onclick={start}>もう一度試す</button>
    {/if}
  {:else if !sessionId}
    <p class="hint">Misskeyインスタンスのホスト名を入力してください（例: misskey.example）</p>
    <div class="form">
      <input
        placeholder="misskey.example"
        bind:value={host}
        onkeydown={(e) => e.key === "Enter" && host.trim() && start()}
      />
      <button disabled={busy || !host.trim()} onclick={start}>
        {busy ? "…" : "認可ページを開く"}
      </button>
    </div>
  {:else}
    <p class="hint">
      {reauthAccount
        ? "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。"
        : "ブラウザで認可を完了したら、下のボタンを押してください。"}
    </p>
    <button class="primary" disabled={busy} onclick={complete}>
      {busy ? "確認中…" : "認可を完了した"}
    </button>
    <button class="link" onclick={() => (reauthAccount ? start() : (sessionId = null))}>
      やり直す
    </button>
  {/if}
  {#if err}<p class="err">{err}</p>{/if}
</div>
```

- [ ] **Step 3: Add `reauthAccount` state and `startReauth` to `App.svelte`**

In `frontend/src/App.svelte:1-14`, change:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import type { TabView } from "./lib/store.svelte";
  import Column from "./ui/Column.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import AddColumnModal from "./ui/AddColumnModal.svelte";
  import ColumnSettings from "./ui/ColumnSettings.svelte";
  import ComposeBar from "./ui/ComposeBar.svelte";
  import Settings from "./ui/Settings.svelte";
  import Backstage from "./ui/Backstage.svelte";
  import { buildKeymap, eventToChord } from "./lib/keymap";
  import { Settings as SettingsIcon, Pencil } from "@lucide/svelte";
```

to:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import type { TabView } from "./lib/store.svelte";
  import type { Account } from "./bindings/tauri.gen";
  import Column from "./ui/Column.svelte";
  import AddAccount from "./ui/AddAccount.svelte";
  import AddColumnModal from "./ui/AddColumnModal.svelte";
  import ColumnSettings from "./ui/ColumnSettings.svelte";
  import ComposeBar from "./ui/ComposeBar.svelte";
  import Settings from "./ui/Settings.svelte";
  import Backstage from "./ui/Backstage.svelte";
  import { buildKeymap, eventToChord } from "./lib/keymap";
  import { Settings as SettingsIcon, Pencil } from "@lucide/svelte";
```

In `frontend/src/App.svelte:20-36`, change:

```svelte
  let showAdd = $state(false);
  let showAddColumn = $state(false);
  let editTab = $state<TabView | null>(null);
  type SettingsSection = "accounts" | "display" | "notify" | "mute" | "keys";
  let showSettings = $state(false);
  let settingsInitial = $state<SettingsSection>("notify");
  let addTabGroupId = $state<string | null>(null);
  let columnSettingsGroupId = $state<string | null>(null);

  function openSettings(section: SettingsSection) {
    settingsInitial = section;
    showSettings = true;
  }
  function addAccountFromSettings() {
    showSettings = false;
    showAdd = true;
  }
```

to:

```svelte
  let showAdd = $state(false);
  let showAddColumn = $state(false);
  let editTab = $state<TabView | null>(null);
  type SettingsSection = "accounts" | "display" | "notify" | "mute" | "keys";
  let showSettings = $state(false);
  let settingsInitial = $state<SettingsSection>("notify");
  let addTabGroupId = $state<string | null>(null);
  let columnSettingsGroupId = $state<string | null>(null);
  let reauthAccount = $state<Account | null>(null);

  function openSettings(section: SettingsSection) {
    settingsInitial = section;
    showSettings = true;
  }
  function addAccountFromSettings() {
    showSettings = false;
    showAdd = true;
  }
  function startReauth(account: Account) {
    showSettings = false;
    reauthAccount = account;
  }
```

- [ ] **Step 4: Show the reauth `AddAccount` in the main view**

In `frontend/src/App.svelte:114-118`, change:

```svelte
  <main class="main">
    {#if app.booting}
      <div class="center-msg">起動中…</div>
    {:else if showAdd || app.accounts.length === 0}
      <AddAccount onclose={app.accounts.length > 0 ? () => (showAdd = false) : undefined} />
```

to:

```svelte
  <main class="main">
    {#if app.booting}
      <div class="center-msg">起動中…</div>
    {:else if showAdd || reauthAccount || app.accounts.length === 0}
      <AddAccount
        reauthAccount={reauthAccount ?? undefined}
        onclose={
          app.accounts.length > 0
            ? () => {
                showAdd = false;
                reauthAccount = null;
              }
            : undefined
        }
      />
```

- [ ] **Step 5: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes. `startReauth` is defined but not yet called from any button — that's fine, it's a private function used starting in Task 6; TypeScript/svelte-check doesn't flag unused private functions in this project's config (verify: if it does, see note below).

If `pnpm check` reports `startReauth` as unused, that's expected until Task 6 wires it — this is acceptable because Task 6 lands immediately after in the same plan and no intermediate commit is pushed/released standalone. Proceed to commit.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/AddAccount.svelte frontend/src/App.svelte
git commit -m "feat: AddAccountに再認証モードを追加"
```

---

### Task 6: 「再認証」ボタン — `AccountsSection.svelte` + `Settings.svelte` + `App.svelte`

**Files:**
- Modify: `frontend/src/ui/settings/AccountsSection.svelte`
- Modify: `frontend/src/ui/Settings.svelte`
- Modify: `frontend/src/App.svelte` (Settings usage, :166-172)

**Interfaces:**
- Consumes: `startReauth(account: Account): void` (Task 5, `App.svelte`)
- Produces: `AccountsSection` prop `onReauth: (account: Account) => void`; `Settings` prop `onReauth: (account: Account) => void`

- [ ] **Step 1: Add the "再認証" button to `AccountsSection.svelte`**

In `frontend/src/ui/settings/AccountsSection.svelte:1-4`, change:

```svelte
<script lang="ts">
  import { app } from "../../lib/store.svelte";

  let { onAddAccount }: { onAddAccount: () => void } = $props();
```

to:

```svelte
<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import type { Account } from "../../bindings/tauri.gen";

  let {
    onAddAccount,
    onReauth,
  }: { onAddAccount: () => void; onReauth: (account: Account) => void } = $props();
```

In `frontend/src/ui/settings/AccountsSection.svelte:58-63`, change:

```svelte
        {:else}
          {#if a.id !== app.defaultAccountId()}
            <button class="ghost" onclick={() => makeDefault(a.id)}>既定に設定</button>
          {/if}
          <button class="ghost" onclick={() => (confirmId = a.id)}>削除</button>
        {/if}
```

to:

```svelte
        {:else}
          {#if a.id !== app.defaultAccountId()}
            <button class="ghost" onclick={() => makeDefault(a.id)}>既定に設定</button>
          {/if}
          <button class="ghost" onclick={() => onReauth(a)}>再認証</button>
          <button class="ghost" onclick={() => (confirmId = a.id)}>削除</button>
        {/if}
```

- [ ] **Step 2: Thread `onReauth` through `Settings.svelte`**

In `frontend/src/ui/Settings.svelte:14-18`, change:

```svelte
  let {
    onclose,
    onAddAccount,
    initial = "notify",
  }: { onclose: () => void; onAddAccount: () => void; initial?: Section } = $props();
```

to:

```svelte
  let {
    onclose,
    onAddAccount,
    onReauth,
    initial = "notify",
  }: {
    onclose: () => void;
    onAddAccount: () => void;
    onReauth: (account: Account) => void;
    initial?: Section;
  } = $props();
```

In `frontend/src/ui/Settings.svelte:1-10`, add the `Account` type import — change:

```svelte
<script lang="ts">
  import NotifySection from "./settings/NotifySection.svelte";
```

to:

```svelte
<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";
  import NotifySection from "./settings/NotifySection.svelte";
```

In `frontend/src/ui/Settings.svelte:53-54`, change:

```svelte
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} />
```

to:

```svelte
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} {onReauth} />
```

- [ ] **Step 3: Pass `onReauth={startReauth}` from `App.svelte`**

In `frontend/src/App.svelte:166-172`, change:

```svelte
  {#if showSettings}
    <Settings
      initial={settingsInitial}
      onAddAccount={addAccountFromSettings}
      onclose={() => (showSettings = false)}
    />
  {/if}
```

to:

```svelte
  {#if showSettings}
    <Settings
      initial={settingsInitial}
      onAddAccount={addAccountFromSettings}
      onReauth={startReauth}
      onclose={() => (showSettings = false)}
    />
  {/if}
```

- [ ] **Step 4: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes with no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AccountsSection.svelte frontend/src/ui/Settings.svelte frontend/src/App.svelte
git commit -m "feat: 設定→アカウントに再認証ボタンを追加"
```

---

### Task 7: 403検知アクション — `Backstage.svelte` + `App.svelte`

**Files:**
- Modify: `frontend/src/ui/Backstage.svelte`
- Modify: `frontend/src/App.svelte` (:132-134)

**Interfaces:**
- Consumes: `LogEntry.reauthAccountId?: string` (Task 2), `startReauth(account: Account): void` (Task 5), `app.accounts: Account[]`
- Produces: `Backstage` prop `onReauth: (accountId: string) => void`

- [ ] **Step 1: Add `onReauth` prop and render the action button in `Backstage.svelte`**

In `frontend/src/ui/Backstage.svelte:1-8`, change:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import type { LogLevel } from "../lib/store.svelte";
  import type { Component } from "svelte";
  import { Circle, Check, TriangleAlert, X, ChevronUp, ChevronDown, Database, Activity, Clock } from "@lucide/svelte";

  let open = $state(false);
```

to:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import type { LogLevel } from "../lib/store.svelte";
  import type { Component } from "svelte";
  import { Circle, Check, TriangleAlert, X, ChevronUp, ChevronDown, Database, Activity, Clock } from "@lucide/svelte";

  let { onReauth }: { onReauth: (accountId: string) => void } = $props();

  let open = $state(false);
```

In `frontend/src/ui/Backstage.svelte:49-56`, change:

```svelte
        {#each app.logs as l (l.id)}
          {@const Ic = icon[l.level]}
          <div class="log-row" data-level={l.level}>
            <span class="ic" data-level={l.level}><Ic size={12} /></span>
            <span class="ts">{hhmmss(l.at)}</span>
            <span class="msg">{l.text}</span>
          </div>
        {/each}
```

to:

```svelte
        {#each app.logs as l (l.id)}
          {@const Ic = icon[l.level]}
          <div class="log-row" data-level={l.level}>
            <span class="ic" data-level={l.level}><Ic size={12} /></span>
            <span class="ts">{hhmmss(l.at)}</span>
            <span class="msg">{l.text}</span>
            {#if l.reauthAccountId}
              <button class="reauth" onclick={() => onReauth(l.reauthAccountId!)}>再認証</button>
            {/if}
          </div>
        {/each}
```

Add a `.reauth` style next to the existing `.log-row`/`.msg` rules — in `frontend/src/ui/Backstage.svelte`, in the `<style>` block, change:

```css
  .msg {
    word-break: break-word;
  }
```

to:

```css
  .msg {
    word-break: break-word;
    flex: 1;
  }
  .reauth {
    flex: none;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--accent);
    border-radius: 4px;
    padding: 1px 8px;
    cursor: pointer;
    font-size: 0.72rem;
  }
  .reauth:hover {
    border-color: var(--accent);
  }
```

- [ ] **Step 2: Wire `onReauth` from `App.svelte`**

In `frontend/src/App.svelte:132-134`, change:

```svelte
  {#if app.accounts.length > 0 && !app.booting}
    <Backstage />
  {/if}
```

to:

```svelte
  {#if app.accounts.length > 0 && !app.booting}
    <Backstage
      onReauth={(accountId) => {
        const acc = app.accounts.find((a) => a.id === accountId);
        if (acc) startReauth(acc);
      }}
    />
  {/if}
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm check`
Expected: passes with no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/Backstage.svelte frontend/src/App.svelte
git commit -m "feat: Backstageの403ログに再認証アクションを追加"
```

---

### Task 8: Manual verification

**Files:** none (verification only)

- [ ] **Step 1: Full type-check**

Run: `cd frontend && pnpm check`
Expected: no errors.

- [ ] **Step 2: Launch the app**

Run: `cargo tauri dev` (from repo root — never `cargo run`/`./target/debug/tsumugi` directly, per root `CLAUDE.md`)

- [ ] **Step 3: Manual reauth via Settings**

With at least one account already added: open 設定→アカウント, click 「再認証」 on an account. Confirm:
- The main view swaps to `AddAccount` showing "再認証: @user@host" and the browser opens automatically (no host input shown).
- After completing authorization in the browser and clicking "認可を完了した", the view returns to the columns and Backstage logs "再認証しました: @user@host" (not "アカウントを追加").
- The account's columns/groups are unchanged (same tabs present before and after).

- [ ] **Step 4: Manual verification of 403 → Backstage action**

Temporarily revert or bypass a scope (e.g. test against an account whose token predates a recently-added scope, or manually clear a required permission on the Misskey instance side) to trigger a 403 from one of the wrapped calls (e.g. attempt to favorite a note). Confirm:
- A `warn`/`error` line appears in Backstage with a "再認証" button.
- Clicking it opens the same reauth `AddAccount` flow as Step 3, pre-filled with the correct account's host.

- [ ] **Step 5: Regression check on plain "アカウントを追加"**

From 設定→アカウント, click 「＋ アカウントを追加」. Confirm the host-input step still appears (heading "アカウントを追加", not "再認証: ..."), and adding a brand-new account still works end-to-end.

- [ ] **Step 6: Report results**

No commit for this task — it's verification only. If any step fails, return to the relevant task, fix, re-run `pnpm check`, and amend forward with a new commit (do not amend previous commits per repo workflow rules).
