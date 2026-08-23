# Issue #234 投稿削除機能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** NoteCardの「その他」メニュー（`NoteMenu.svelte`）に、自分の投稿にのみ表示される「削除」項目を追加し、確認ダイアログを経てMisskeyから投稿を削除できるようにする。

**Architecture:** バックエンド（`delete_note_cmd`）とstore層（`AppState.deleteNote()`）は実装済み・未使用のまま存在する。本実装はUI層（`NoteMenu.svelte`）のみを変更し、既存の `ConfirmDialog` コンポーネントで確認を挟んでから `app.deleteNote()` を呼び出す配線を追加する。

**Tech Stack:** Svelte 5（runes: `$props`, `$derived`, `$state`）, Vitest + @testing-library/svelte, `@lucide/svelte`（アイコン）, Tailwind（`text-destructive` トークン）。

## Global Constraints

- 削除ボタンは自分の投稿（`note.user.id === accountId に対応する Account.userId`）にのみ表示する。他人の投稿では表示しない。
- 削除実行前に既存の `ConfirmDialog`（`danger` フラグ付き）で確認を取る。確認なしの即削除はしない。
- バックエンド（`src-tauri/`）・store層（`store.svelte.ts` の `AppState.deleteNote()`）は変更しない。UI配線のみ。
- 参照設計doc: `docs/superpowers/specs/2026-08-23-issue-234-delete-note-design.md`

---

### Task 1: NoteMenuに削除項目を追加する

**Files:**
- Modify: `frontend/src/ui/NoteMenu.svelte`
- Test: `frontend/src/ui/NoteCard.test.ts`（既存ファイルに追記。`NoteMenu` は `NoteCard` 経由でメニューを開いてレンダリングされる想定）

**Interfaces:**
- Consumes:
  - `app.accounts: Account[]`（`frontend/src/lib/store.svelte.ts:124`）。`Account` 型は `frontend/src/bindings/tauri.gen.ts:261` の `{ id: string; host: string; username: string; userId: string; displayName: string; avatarUrl: string | null }`。
  - `app.deleteNote(accountId: string, noteId: string): Promise<void>`（`frontend/src/lib/store.svelte.ts:1575`、失敗時は内部でログを残した上で例外を re-throw する）。
  - `ConfirmDialog` の props（`frontend/src/ui/ConfirmDialog.svelte:5-21`）: `title?`, `message`, `confirmLabel?`, `cancelLabel?`, `danger?`, `onConfirm: () => void`, `onCancel: () => void`。
  - `NoteMenu` 自身の既存 props（`accountId: string`, `note: Note`, `onclose: () => void`）。
- Produces: なし（末端のUIコンポーネント）。

- [ ] **Step 1: 失敗するテストを書く（自分の投稿で削除項目が出る）**

`frontend/src/ui/NoteCard.test.ts` の末尾に以下を追記する（`makeUser` / `makeNote` は既存ヘルパーを再利用）。`app` シングルトンをimportし、テスト内で `app.accounts` に自分のアカウントを登録してから、`NoteCard` の「その他」ボタンをクリックしてメニューを開く。

```ts
describe("投稿削除メニュー", () => {
  it("自分の投稿では削除項目を表示する", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makeNote({ user: makeUser({ id: "u1" }) });
    const { container, getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(getByText("削除")).toBeTruthy();
    expect(container.querySelector("svg.lucide-trash-2")).toBeTruthy();

    app.accounts.length = 0;
  });

  it("他人の投稿では削除項目を表示しない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makeNote({ user: makeUser({ id: "other-user" }) });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("削除")).toBeNull();

    app.accounts.length = 0;
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "投稿削除メニュー"`
Expected: FAIL（「削除」テキストが見つからない）

- [ ] **Step 3: `NoteMenu.svelte` に削除項目を実装する**

`frontend/src/ui/NoteMenu.svelte` の `<script>` 部分を編集する。

`import` 文を編集:
```ts
import { Star, Paperclip, ChevronRight, Trash2 } from "@lucide/svelte";
```

`let { accountId, note, onclose }: ... = $props();` の直後に追加:
```ts
const isOwnNote = $derived(app.accounts.find((a) => a.id === accountId)?.userId === note.user.id);
let confirmDeleteOpen = $state(false);

function requestDelete() {
  confirmDeleteOpen = true;
}

async function confirmDelete() {
  confirmDeleteOpen = false;
  try {
    await app.deleteNote(accountId, note.id);
  } finally {
    onclose();
  }
}
```

マークアップの末尾（クリップ用の `<div class="relative" ...>...</div>` の直後、コンテナ `</div>` の直前）に追加:
```svelte
  {#if isOwnNote}
    <button
      type="button"
      class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted"
      onclick={requestDelete}
    >
      <Trash2 size={16} />
      削除
    </button>
  {/if}
```

コンポーネントのトップレベル（`<div class="w-[200px] ...">` の外、末尾）に追加:
```svelte
{#if confirmDeleteOpen}
  <ConfirmDialog
    title="投稿の削除"
    message="この投稿を削除します。取り消せません。よろしいですか？"
    confirmLabel="削除する"
    danger
    onConfirm={confirmDelete}
    onCancel={() => (confirmDeleteOpen = false)}
  />
{/if}
```

`import` に `ConfirmDialog` を追加:
```ts
import ConfirmDialog from "./ConfirmDialog.svelte";
```

- [ ] **Step 4: テストを実行して通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "投稿削除メニュー"`
Expected: PASS（両方のテスト）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/NoteMenu.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: 自分の投稿をその他メニューから削除できるようにする"
```

---

### Task 2: 削除確認ダイアログのフローをテストする

**Files:**
- Modify: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: Task 1で実装した `NoteMenu.svelte` の削除ボタン・`ConfirmDialog` 連携、および `app.deleteNote`（`frontend/src/lib/store.svelte.ts:1575`）。
- Produces: なし。

- [ ] **Step 1: 失敗するテストを書く（確認ダイアログ経由でdeleteNoteが呼ばれる／キャンセルで呼ばれない）**

`frontend/src/ui/NoteCard.test.ts` の `describe("投稿削除メニュー", ...)` 内に追記する。`app.deleteNote` をスパイして呼び出しを検証する（実APIは `@tauri-apps/api/core` の `invoke` モック経由になるため、直接 `vi.spyOn` する）。

```ts
  it("削除ボタン→確認ダイアログで確定するとdeleteNoteが呼ばれる", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const deleteSpy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makeNote({ id: "n-delete-1", user: makeUser({ id: "u1" }) });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("削除").click();
    await getByText("削除する").click();

    expect(deleteSpy).toHaveBeenCalledWith("acc1", "n-delete-1");

    deleteSpy.mockRestore();
    app.accounts.length = 0;
  });

  it("削除ボタン→確認ダイアログをキャンセルするとdeleteNoteが呼ばれない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const deleteSpy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makeNote({ id: "n-delete-2", user: makeUser({ id: "u1" }) });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("削除").click();
    await getByText("キャンセル").click();

    expect(deleteSpy).not.toHaveBeenCalled();

    deleteSpy.mockRestore();
    app.accounts.length = 0;
  });
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "削除ボタン"`
Expected: Task 1完了済みなら実際にはPASSする可能性が高い。もしFAILする場合は `ConfirmDialog` のボタンテキスト（`確定`/`キャンセル`）やイベント配線の不備を示す実質的な失敗として扱い、Step 3で修正する。

- [ ] **Step 3: 必要なら `NoteMenu.svelte` を修正する**

Task 1のStep 3で実装済みのコードで通るはずだが、テストが失敗した場合は `ConfirmDialog` の `onConfirm`/`onCancel` 配線、または `confirmDeleteOpen` の状態管理を見直して修正する。

- [ ] **Step 4: テストを実行して通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS（ファイル内の全テスト）

- [ ] **Step 5: 型チェックとコミット**

```bash
cd frontend && pnpm check
```
Expected: エラーなし

```bash
git add frontend/src/ui/NoteCard.test.ts
git commit -m "test: 投稿削除の確認ダイアログフローをテストする"
```

---

### Task 3: 手動確認とPR作成

**Files:** なし（動作確認とPR作成のみ）

**Interfaces:**
- Consumes: Task 1・Task 2で完成した削除機能一式。
- Produces: なし。

- [ ] **Step 1: 全フロントエンドテストを実行する**

Run: `cd frontend && pnpm test`
Expected: PASS（既存テストも含め全て）

- [ ] **Step 2: `cargo tauri dev` で実機確認する**

リポジトリルートから `cargo tauri dev` を起動し、実際のMisskeyアカウントで以下を確認する:
- 自分の投稿の「その他」メニューに「削除」が表示される。
- 他人の投稿・Renoteされた他人の投稿には「削除」が表示されない。
- 「削除」→確認ダイアログ→「削除する」で投稿がタイムラインから消え、Misskey側でも削除される。
- 確認ダイアログで「キャンセル」を押すと投稿が残る。

確認後、確認のために起動した `cargo tauri dev` は自分で終了させる。

- [ ] **Step 3: PRを作成する**

```bash
git push -u origin feature/issue-234-delete-note
gh pr create --title "feat: 投稿をその他メニューから削除できるようにする" --body "$(cat <<'EOF'
Fixes #234

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

## 自己レビュー結果

- **spec網羅性:** design docの3変更点（NoteMenuへの削除項目追加、既存削除後処理の確認、スコープ外の明記）はTask 1・Task 2で実装され、Task 3で動作確認とストリーミング冪等性の実地検証を行う。バックエンド・store層は変更しない方針もGlobal Constraintsに明記済み。
- **プレースホルダー:** なし。全ステップに実コード・実コマンドを記載。
- **型整合性:** `Account.userId`・`app.deleteNote(accountId, noteId)`・`ConfirmDialog` props は全タスクで同一の型・シグネチャを使用。
