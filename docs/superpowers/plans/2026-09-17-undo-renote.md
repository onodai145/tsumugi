# Renoteの取り消し Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 自分が行った純粋なRenote(引用なし)を、そのRenoteノートのメニューから取り消せるようにする。

**Architecture:** バックエンドは無変更。`NoteCard.svelte` が純粋Renoteノート自身を新propとして `NoteMenu.svelte` に渡し、`NoteMenu.svelte` がその投稿者が自分かどうかを判定して「Renote取り消し」項目を出し、確認後に既存の `app.deleteNote(accountId, noteId)` をRenoteノート自身のIDで呼ぶ。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest + @testing-library/svelte。

## Global Constraints

- 対象は純粋なRenote(`!note.text && !!note.renote`)のみ。引用付きRenoteは対象外。
- 新規のRust/Tauriコマンドは追加しない。既存の `app.deleteNote(accountId, noteId)` をRenoteノート自身のIDで呼ぶだけで実現する。
- 「自分がこの元ノートをRenoteしたか」をサーバー問い合わせやセッション追跡で判定する仕組みは作らない。自分のRenoteノート自身を表示しているときのみ操作可能、という制約を受け入れる(公式Misskeyクライアントと同じ挙動)。
- 確認ダイアログの文言は「このRenoteを取り消しますか？」とし、「取り消す/取り消せない」のように同じ語を二重に使う言い回しは避ける。
- 参照spec: `docs/superpowers/specs/2026-09-17-undo-renote-design.md`

---

### Task 1: NoteMenuに「Renote取り消し」を追加し、NoteCardから配線する

**Files:**
- Modify: `frontend/src/ui/NoteMenu.svelte`
- Modify: `frontend/src/ui/NoteCard.svelte:580`
- Test: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `app.deleteNote(accountId: string, noteId: string): Promise<void>`(既存、`frontend/src/lib/store.svelte.ts` で定義済み。変更しない)。`app.accounts: Account[]`(各要素に `id`, `userId` を持つ、既存)。
- Produces: `NoteMenu` の新しい optional prop `pureRenoteOf?: Note`(渡された場合、その `note.user.id` が現在の `accountId` に対応する `userId` と一致するときだけ「Renote取り消し」メニュー項目を表示する)。`NoteCard.svelte` は `isPureRenote` のときだけ `pureRenoteOf={note}` を渡す(所有者判定はNoteMenu側で行うので、NoteCard側では判定しない)。

- [ ] **Step 1: 失敗するテストを書く(4ケースをまとめて1コミット分として書く)**

`frontend/src/ui/NoteCard.test.ts` の末尾(既存の `describe("投稿削除メニュー", ...)` ブロックの後、ファイル末尾の `});` の直前)に以下を追加する:

```ts
describe("Renote取り消しメニュー", () => {
  let undoSpy: ReturnType<typeof vi.spyOn> | null = null;
  afterEach(async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.length = 0;
    undoSpy?.mockRestore();
    undoSpy = null;
  });

  function makePureRenote(overrides: { renoterId: string; renoteId?: string }): Note {
    return makeNote({
      id: overrides.renoteId ?? "n-renote-1",
      text: null,
      user: makeUser({ id: overrides.renoterId }),
      renote: makeNote({ id: "n0", user: makeUser({ id: "other-original-author" }) }),
    });
  }

  it("自分のRenoteノートでは取り消し項目を表示する", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makePureRenote({ renoterId: "u1" });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(getByText("Renote取り消し")).toBeTruthy();
  });

  it("他人のRenoteノートでは取り消し項目を表示しない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const note = makePureRenote({ renoterId: "someone-else" });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("Renote取り消し")).toBeNull();
  });

  it("純粋Renoteでない自分の投稿では取り消し項目を表示しない", async () => {
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
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("Renote取り消し")).toBeNull();
  });

  it("取り消しボタン→確認ダイアログで確定するとdeleteNoteがRenoteノート自身のIDで呼ばれる", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    undoSpy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makePureRenote({ renoterId: "u1", renoteId: "n-renote-2" });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("Renote取り消し").click();
    await getByText("取り消す").click();

    expect(undoSpy).toHaveBeenCalledWith("acc1", "n-renote-2");
  });

  it("取り消しボタン→確認ダイアログをキャンセルするとdeleteNoteが呼ばれない", async () => {
    const { app } = await import("../lib/store.svelte");
    app.accounts.push({
      id: "acc1",
      host: "misskey.example",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
    });
    const spy = vi.spyOn(app, "deleteNote").mockResolvedValue(undefined);
    const note = makePureRenote({ renoterId: "u1", renoteId: "n-renote-3" });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("Renote取り消し").click();
    await getByText("キャンセル").click();

    expect(spy).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm exec vitest run src/ui/NoteCard.test.ts`
Expected: 新しい5件のテストが FAIL する(「Renote取り消し」というテキストがDOMに存在しない、または `pureRenoteOf` が未知のpropとして無視される)。既存のテスト(削除メニュー等)はこの時点でもPASSしたままであること。

- [ ] **Step 3: `NoteMenu.svelte` に `pureRenoteOf` prop とRenote取り消しUIを実装する**

`frontend/src/ui/NoteMenu.svelte` を以下のように変更する。

1行目の import に `Repeat2` を追加:

```svelte
  import { Star, Paperclip, ChevronRight, Trash2, Copy, Repeat2 } from "@lucide/svelte";
```

props定義を変更:

```svelte
  let {
    accountId,
    note,
    pureRenoteOf,
    onclose,
  }: { accountId: string; note: Note; pureRenoteOf?: Note; onclose: () => void } = $props();

  const isOwnNote = $derived(app.accounts.find((a) => a.id === accountId)?.userId === note.user.id);
  const canUndoRenote = $derived(
    pureRenoteOf != null && app.accounts.find((a) => a.id === accountId)?.userId === pureRenoteOf.user.id,
  );
  let confirmDeleteOpen = $state(false);
  let confirmUndoRenoteOpen = $state(false);
```

`confirmDelete` 関数の下に、取り消し用の関数を追加:

```svelte
  function requestUndoRenote() {
    confirmUndoRenoteOpen = true;
  }

  async function confirmUndoRenote() {
    confirmUndoRenoteOpen = false;
    try {
      await app.deleteNote(accountId, pureRenoteOf!.id);
    } finally {
      onclose();
    }
  }
```

テンプレートの `{#if isOwnNote}` ブロック(削除ボタン)の直後、`</div>`(メニュー本体を閉じるタグ)の手前に追加:

```svelte
  {#if canUndoRenote}
    <button
      type="button"
      class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted"
      onclick={requestUndoRenote}
    >
      <Repeat2 size={16} />
      Renote取り消し
    </button>
  {/if}
```

既存の `{#if confirmDeleteOpen}` ブロック(`<ConfirmDialog ... />`)の直後に、取り消し用の確認ダイアログを追加:

```svelte
{#if confirmUndoRenoteOpen}
  <ConfirmDialog
    title="Renoteの取り消し"
    message="このRenoteを取り消しますか？"
    confirmLabel="取り消す"
    danger
    z={1020}
    onConfirm={confirmUndoRenote}
    onCancel={() => (confirmUndoRenoteOpen = false)}
  />
{/if}
```

- [ ] **Step 4: `NoteCard.svelte` から `pureRenoteOf` を配線する**

`frontend/src/ui/NoteCard.svelte:580` の

```svelte
                  <NoteMenu {accountId} note={inner} onclose={() => (noteMenuOpen = false)} />
```

を以下に変更する:

```svelte
                  <NoteMenu {accountId} note={inner} pureRenoteOf={isPureRenote ? note : undefined} onclose={() => (noteMenuOpen = false)} />
```

- [ ] **Step 5: テストを実行し、すべてPASSすることを確認する**

Run: `cd frontend && pnpm exec vitest run src/ui/NoteCard.test.ts`
Expected: 全テスト(既存分含む)がPASS。

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 7: コミット**

```bash
git add frontend/src/ui/NoteMenu.svelte frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: 純粋Renoteの取り消しメニューを追加"
```

---

### Task 2: 実機での動作確認

**Files:** なし(コード変更なし、手動確認のみ)

**Interfaces:**
- Consumes: Task 1で実装した「Renote取り消し」メニュー項目。
- Produces: なし(検証タスク)。

- [ ] **Step 1: Xvfb越しにdevサーバーを起動する**

リポジトリルートから、実際のディスプレイに影響を与えないよう `xvfb-run` 経由で起動する:

```bash
xvfb-run -a cargo tauri dev
```

- [ ] **Step 2: 任意のノートをRenoteする**

タイムライン上の任意のノートのRenoteボタン(Repeat2アイコン)をクリックし、純粋Renoteを作成する。

- [ ] **Step 3: 自分のタイムラインで、作成したRenoteノートのメニューを開く**

自分のホーム/ローカルタイムライン等に流れてきた、自分がRenoteした旨のバナー(「(自分の表示名)さんがRenote」)付きのノートカードで「その他」ボタン(`⋯`)をクリックし、「Renote取り消し」が表示されることを確認する。

- [ ] **Step 4: 取り消しを実行する**

「Renote取り消し」→確認ダイアログで「取り消す」をクリックし、そのRenoteノートがタイムラインから消えることを確認する。

- [ ] **Step 5: 元ノートが削除されていないことを確認する**

Renote先の元ノート(Renoteされた側の投稿)がタイムライン上に別途表示されている場合、それが削除されずそのまま残っていることを確認する。

- [ ] **Step 6: devサーバーを終了する**

検証に使った `cargo tauri dev` プロセスを終了する(起動した自分でkillする。名前パターンでの `pkill`/`killall` は使わず、正確なPIDを指定して `kill` する)。
