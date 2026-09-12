# DrivePicker/ColumnSettingsのモーダル実装をModal.svelteに統一する Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `DrivePicker.svelte`と`ColumnSettings.svelte`の独自モーダル実装(portal・フォーカス管理・Escape処理・z-index)を、共有`Modal.svelte`に統一する(Issue #196)。

**Architecture:** `Modal.svelte`に任意prop `maxHeight` を追加し、指定時のみボックスを`flex flex-col`+`max-height`にして`children`を`flex-1 min-h-0`ラッパーで包む(未指定時は現状の挙動を完全維持)。`ColumnSettings.svelte`はこの拡張なしでそのまま`Modal`の`children`にフォームを渡す。`DrivePicker.svelte`は`maxHeight="78vh"`を指定し、既存の「パンくず(flex-none)/グリッド(flex-1 min-h-0 overflow-y-auto)/フッター(flex-none)」構造をそのまま`children`に移す。

**Tech Stack:** Svelte 5(runes, Snippet), Tailwind CSS, Vitest + @testing-library/svelte。

## Global Constraints

- `Modal.svelte`の既存呼び出し箇所(Settings/AddColumnModal/SearchModal/ProfileModal/FollowListModal/ComposeBar/NoteCard/NotificationCard)の見た目・挙動を変えないこと(`maxHeight`未指定時は現状と完全に同一の出力になること)。
- `DrivePicker.svelte`/`ColumnSettings.svelte`の公開propsのシグネチャ(呼び出し元から見たインターフェース)は変更しないこと。
- コミットメッセージは件名のみ(本文・箇条書き禁止)。
- 設計ドキュメント: `docs/superpowers/specs/2026-09-12-drivepicker-columnsettings-modal-unify-design.md`

---

### Task 1: `Modal.svelte`に`maxHeight` propを追加する

**Files:**
- Modify: `frontend/src/ui/Modal.svelte`
- Test: `frontend/src/ui/Modal.test.ts` (新規)

**Interfaces:**
- Produces: `Modal`のprops型が `{ title: string; onclose: () => void; children: Snippet; width?: string; maxHeight?: string }` になる(`maxHeight`は新規の任意prop)。`maxHeight`未指定時はダイアログ要素(`role="dialog"`)に`max-height`インラインスタイルが付かず、`children`はラッパーdivなしで直接レンダリングされる。`maxHeight`指定時はダイアログ要素に`style="max-height:<値>"`相当が付き、`flex-col`クラスを持ち、`children`は`class="flex flex-1 flex-col min-h-0"`のdivでラップされる。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/Modal.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import Modal from "./Modal.svelte";

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(() => {
  cleanup();
});

describe("Modal", () => {
  it("タイトルを表示し、×ボタンでoncloseを呼ぶ", async () => {
    const onclose = vi.fn();
    const { getByText, getByRole } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    expect(getByText("設定")).toBeTruthy();
    expect(getByText("hi")).toBeTruthy();
    await fireEvent.click(getByRole("button"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("Escapeキーでoncloseを呼ぶ", async () => {
    const onclose = vi.fn();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await fireEvent.keyDown(getByRole("presentation"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("オーバーレイクリックでoncloseを呼ぶが、ダイアログ内クリックでは呼ばない", async () => {
    const onclose = vi.fn();
    const { getByRole, getByText } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await fireEvent.click(getByText("hi"));
    expect(onclose).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("maxHeight未指定時はダイアログにmax-heightもflex-colも付かない", () => {
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("");
    expect(dialog.className).not.toMatch(/flex-col/);
  });

  it("maxHeight指定時はダイアログがflex-colになり、max-heightが設定され、childrenがflex-1 min-h-0でラップされる", () => {
    const { getByRole, getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), maxHeight: "78vh" },
    });
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("78vh");
    expect(dialog.className).toMatch(/flex-col/);
    const wrapper = getByText("hi").closest("div");
    expect(wrapper?.className).toMatch(/flex-1/);
    expect(wrapper?.className).toMatch(/min-h-0/);
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/Modal.test.ts`
Expected: FAIL(`maxHeight`未実装のため、`maxHeight`指定時のテストが落ちる。他のテストは既存実装でも通る可能性があるが、まずファイル全体を作成した状態で流して現状を確認する)

- [ ] **Step 3: `Modal.svelte`に`maxHeight`を実装する**

`frontend/src/ui/Modal.svelte`を以下に置き換える:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    title,
    onclose,
    children,
    width = "480px",
    maxHeight,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
    maxHeight?: string;
  } = $props();

  // 深くネストされたコンポーネントから呼ばれても
  // content-visibility/containの包含ブロックを脱出できるよう portal で body 直下に置く。
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  let modalEl: HTMLDivElement | undefined;

  $effect(() => {
    modalEl?.focus();
  });
</script>

<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center overflow-y-auto bg-black/45 pt-[max(8vh,env(safe-area-inset-top))] pb-[max(8vh,env(safe-area-inset-bottom))]"
  use:portal
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class={`w-[min(var(--modal-w),92vw)] rounded-xl border border-border bg-background p-4 ${maxHeight ? "flex flex-col" : ""}`}
    style={`--modal-w:${width};${maxHeight ? ` max-height:${maxHeight};` : ""}`}
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-3 flex flex-none items-center justify-between font-semibold">
      <span>{title}</span>
      <Button variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>
    {#if maxHeight}
      <div class="flex flex-1 flex-col min-h-0">
        {@render children()}
      </div>
    {:else}
      {@render children()}
    {/if}
  </div>
</div>
```

- [ ] **Step 4: テストを実行し、すべて通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/Modal.test.ts`
Expected: PASS(全5件)

- [ ] **Step 5: コミット**

```bash
cd frontend && git add src/ui/Modal.svelte src/ui/Modal.test.ts
git commit -m "feat: Modal.svelteにmaxHeight propを追加"
```

---

### Task 2: `ColumnSettings.svelte`を`Modal`に統一する

**Files:**
- Modify: `frontend/src/ui/ColumnSettings.svelte`
- Test: `frontend/src/ui/ColumnSettings.test.ts` (新規)

**Interfaces:**
- Consumes: `Modal`の`{ title, onclose, children, width }`(Task 1で確定した型)。
- Produces: `ColumnSettings`の公開propsは変更なし(`{ groupId: string; onclose: () => void }`)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ColumnSettings.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { PaneNode } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ColumnSettings } = await import("./ColumnSettings.svelte");
const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.groups = [];
  app.paneRoot = { type: "split", id: "boot", direction: "row", children: [] };
});

function setupSingleLeafGroup() {
  app.groups = [{ id: "g1", width: 300, auto: false, tabs: [], activeTabId: "" }];
  app.paneRoot = { type: "leaf", id: "leaf1", groupId: "g1" } satisfies PaneNode;
}

describe("ColumnSettings", () => {
  it("タイトル「カラム設定」と幅設定フォームを表示する", () => {
    setupSingleLeafGroup();
    const { getByText } = render(ColumnSettings, { props: { groupId: "g1", onclose: () => {} } });
    expect(getByText("カラム設定")).toBeTruthy();
    expect(getByText("固定（ドラッグで調整）")).toBeTruthy();
  });

  it("×ボタンでoncloseを呼ぶ", async () => {
    setupSingleLeafGroup();
    const onclose = vi.fn();
    const { getByRole } = render(ColumnSettings, { props: { groupId: "g1", onclose } });
    await fireEvent.click(getByRole("button", { name: "" }));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("「自動調整」を選ぶとsetGroupAuto(true)を呼ぶ", async () => {
    setupSingleLeafGroup();
    const { getByText } = render(ColumnSettings, { props: { groupId: "g1", onclose: () => {} } });
    await fireEvent.click(getByText("自動調整（ウィンドウ幅に合わせて均等割付）"));
    expect(invokeMock).toHaveBeenCalledWith("set_group_auto", expect.objectContaining({ groupId: "g1", auto: true }));
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/ColumnSettings.test.ts`
Expected: 現行実装でも「×ボタン」テストの`getByRole("button", { name: "" })`は複数ボタン(×とラジオ関連は button ではない)がある場合に失敗する可能性がある。まず実行してどのテストが落ちるかを確認する(新規ファイルなので、少なくとも実装変更前の挙動として全件PASSしても構わない — その場合はStep3実装後も同じ結果になることを確認する回帰テストとして機能する)。

- [ ] **Step 3: `ColumnSettings.svelte`を`Modal`ベースに書き換える**

`frontend/src/ui/ColumnSettings.svelte`の`<script>`ブロックの末尾に`import Modal from "./Modal.svelte";`を追加し、`X`と`Button`のimportとテンプレート全体を以下に置き換える:

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import Modal from "./Modal.svelte";

  // カラム(視覚カラム)自体の設定。タブ設定とは別に、グリップのダブルクリックで開く。
  let { groupId, onclose }: { groupId: string; onclose: () => void } = $props();

  const group = $derived(app.groups.find((g) => g.id === groupId));

  function setAuto(auto: boolean) {
    if (groupId) app.setGroupAuto(groupId, auto);
  }

  function setWidth(w: number) {
    if (!groupId || !Number.isFinite(w)) return;
    const clamped = Math.min(720, Math.max(220, Math.round(w)));
    app.setGroupWidthLocal(groupId, clamped);
    app.persistGroupWidth(groupId, clamped);
  }

  const paneCtx = $derived(groupId ? app.paneColumnContext(groupId) : null);

  function setHeightPercent(p: number) {
    if (!paneCtx || !Number.isFinite(p)) return;
    const clamped = Math.min(95, Math.max(5, Math.round(p)));
    app.resizePane(paneCtx.nodeId, clamped);
  }

  function setHeightAuto(auto: boolean) {
    if (!paneCtx) return;
    app.setPaneAuto(paneCtx.nodeId, auto);
  }

  // 縦分割されたブロック全体の幅(Row内でこのグループを含むSplit自身の幅)。
  // 一度も分割していない普通のカラムなら isLeaf=true になり、既存のColumnGroup.width
  // ベースの幅UIをそのまま使う(こちらは触らない)。
  const rowSlot = $derived(groupId ? app.paneRowSlotContext(groupId) : null);

  function setBlockWidth(w: number) {
    if (!rowSlot || rowSlot.isLeaf || !Number.isFinite(w)) return;
    const clamped = Math.min(720, Math.max(220, Math.round(w)));
    app.resizePane(rowSlot.nodeId, clamped);
  }

  function setBlockAuto(auto: boolean) {
    if (!rowSlot || rowSlot.isLeaf) return;
    app.setPaneAuto(rowSlot.nodeId, auto);
  }
</script>

<Modal title="カラム設定" {onclose} width="360px">
  {#if group}
    {#if rowSlot?.isLeaf}
      <div class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">幅</span>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="width-mode"
            class="accent-primary"
            checked={!group.auto}
            onchange={() => setAuto(false)}
          /> 固定（ドラッグで調整）
        </label>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="width-mode"
            class="accent-primary"
            checked={group.auto}
            onchange={() => setAuto(true)}
          /> 自動調整（ウィンドウ幅に合わせて均等割付）
        </label>
      </div>

      {#if !group.auto}
        <label class="mt-2.5 flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">幅（px、220〜720）</span>
          <input
            class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            type="number"
            min="220"
            max="720"
            value={group.width}
            onchange={(e) => setWidth(Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
      {/if}
    {:else if rowSlot}
      <div class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">分割ブロック全体の幅</span>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="block-width-mode"
            class="accent-primary"
            checked={!rowSlot.auto}
            onchange={() => setBlockAuto(false)}
          /> 固定
        </label>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="block-width-mode"
            class="accent-primary"
            checked={rowSlot.auto}
            onchange={() => setBlockAuto(true)}
          /> 自動調整（ウィンドウ幅に合わせて均等割付）
        </label>
      </div>

      {#if !rowSlot.auto}
        <label class="mt-2.5 flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">幅（px、220〜720）</span>
          <input
            class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            type="number"
            min="220"
            max="720"
            value={Math.round(rowSlot.size)}
            onchange={(e) => setBlockWidth(Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
      {/if}
    {/if}

    {#if paneCtx}
      <div class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">高さ</span>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="height-mode"
            class="accent-primary"
            checked={!paneCtx.auto}
            onchange={() => setHeightAuto(false)}
          /> 固定
        </label>
        <label class="flex items-center gap-1.5 text-sm">
          <input
            type="radio"
            name="height-mode"
            class="accent-primary"
            checked={paneCtx.auto}
            onchange={() => setHeightAuto(true)}
          /> 自動調整（残りを均等割り）
        </label>
      </div>

      {#if !paneCtx.auto}
        <label class="mt-2.5 flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">高さ（%、5〜95）</span>
          <input
            class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            type="number"
            min="5"
            max="95"
            value={Math.round(paneCtx.size)}
            onchange={(e) => setHeightPercent(Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
      {/if}
    {/if}
  {/if}
</Modal>
```

- [ ] **Step 4: テストを実行し、すべて通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/ColumnSettings.test.ts`
Expected: PASS(全3件)。「×ボタン」テストの`getByRole("button", { name: "" })`が複数マッチしてエラーになった場合は、`getAllByRole("button")[0]`(ヘッダーの×ボタンがDOM順で最初に来る)に修正して再実行する。

- [ ] **Step 5: `pnpm check`を実行し、型エラーがないことを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: コミット**

```bash
cd frontend && git add src/ui/ColumnSettings.svelte src/ui/ColumnSettings.test.ts
git commit -m "refactor: ColumnSettings.svelteを共有Modal.svelteに統一"
```

---

### Task 3: `DrivePicker.svelte`を`Modal`(`maxHeight`指定)に統一する

**Files:**
- Modify: `frontend/src/ui/DrivePicker.svelte`
- Test: `frontend/src/ui/DrivePicker.test.ts` (新規)

**Interfaces:**
- Consumes: `Modal`の`{ title, onclose, children, width, maxHeight }`(Task 1)。
- Produces: `DrivePicker`の公開propsは変更なし(`{ accountId: string; onSelect: (files: DriveFile[]) => void; onclose: () => void }`)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/DrivePicker.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: DrivePicker } = await import("./DrivePicker.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
});

describe("DrivePicker", () => {
  it("タイトル「ドライブから選択」を表示し、フォルダ一覧を読み込む", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drive_folders") return Promise.resolve([{ id: "f1", name: "写真" }]);
      if (cmd === "list_drive_files") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect: vi.fn(), onclose: () => {} },
    });
    expect(getByText("ドライブから選択")).toBeTruthy();
    await waitFor(() => expect(getByText(/写真/)).toBeTruthy());
  });

  it("×ボタンでoncloseを呼ぶ", async () => {
    invokeMock.mockResolvedValue([]);
    const onclose = vi.fn();
    const { getAllByRole } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect: vi.fn(), onclose },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    await fireEvent.click(getAllByRole("button")[0]);
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("ファイルを選択して添付ボタンでonSelectを呼ぶ", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drive_folders") return Promise.resolve([]);
      if (cmd === "list_drive_files") {
        return Promise.resolve([
          { id: "file1", name: "a.png", mimeType: "image/png", url: "https://example.com/a.png", thumbnailUrl: null, isSensitive: false },
        ]);
      }
      return Promise.resolve(null);
    });
    const onSelect = vi.fn();
    const { getByText } = render(DrivePicker, {
      props: { accountId: "acc1", onSelect, onclose: () => {} },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_drive_files", expect.anything()));
    const fileButtons = document.querySelectorAll('img[alt="a.png"]');
    await fireEvent.click(fileButtons[0].closest("button")!);
    await fireEvent.click(getByText("添付"));
    expect(onSelect).toHaveBeenCalledWith([expect.objectContaining({ id: "file1" })]);
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/DrivePicker.test.ts`
Expected: `commands.listDriveFolders`/`commands.listDriveFiles`の実際のIPCコマンド名(snake_case変換後の値)がテストのmock実装と一致しない場合は失敗する。`frontend/src/bindings/tauri.gen.ts`で`listDriveFolders`/`listDriveFiles`が実際にinvokeする文字列を確認し、テストのmockキーをそれに合わせて修正する。

- [ ] **Step 3: `DrivePicker.svelte`を`Modal`ベースに書き換える**

`frontend/src/ui/DrivePicker.svelte`の`<script>`ブロック冒頭のimportに`import Modal from "./Modal.svelte";`を追加し、`X`のimportを削除、テンプレート全体を以下に置き換える(スクリプト内のロジック部分は変更なし):

```svelte
<Modal title="ドライブから選択" {onclose} width="520px" maxHeight="78vh">
  <nav class="mb-2.5 flex flex-none flex-wrap items-center gap-1 text-sm">
    <button
      type="button"
      class={path.length === 0
        ? "px-1 py-0.5 font-[inherit] font-semibold text-foreground"
        : "px-1 py-0.5 font-[inherit] text-muted-foreground"}
      onclick={() => goToBreadcrumb(-1)}>ドライブ</button
    >
    {#each path as p, i (p.id)}
      <span class="text-muted-foreground">/</span>
      <button
        type="button"
        class={i === path.length - 1
          ? "px-1 py-0.5 font-[inherit] font-semibold text-foreground"
          : "px-1 py-0.5 font-[inherit] text-muted-foreground"}
        onclick={() => goToBreadcrumb(i)}
      >
        {p.name || "(無題)"}
      </button>
    {/each}
  </nav>

  {#if loading}
    <p class="text-sm text-muted-foreground">読み込み中…</p>
  {:else}
    {#if folders.length === 0 && files.length === 0}
      <p class="text-sm text-muted-foreground">ファイルがありません</p>
    {/if}
    <div class="grid min-h-0 flex-1 auto-rows-[84px] grid-cols-[repeat(auto-fill,minmax(84px,1fr))] gap-2 overflow-y-auto">
      {#each folders as f (f.id)}
        <button
          type="button"
          class="relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-1.5 text-xs break-all text-foreground"
          onclick={() => enterFolder(f)}
        >
          📁 {f.name || "(無題)"}
        </button>
      {/each}
      {#each files as f (f.id)}
        <button
          type="button"
          class={selected.has(f.id)
            ? "relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-0 text-xs text-foreground outline outline-2 outline-offset-[-2px] outline-primary"
            : "relative flex h-full w-full items-center justify-center overflow-hidden rounded-lg border border-border bg-muted p-0 text-xs text-foreground"}
          onclick={() => onFileClick(f)}
        >
          {#if f.isSensitive && !revealed[f.id]}
            <span class="p-1 text-muted-foreground">閲覧注意（クリックで表示）</span>
          {:else if f.mimeType.startsWith("image/")}
            <img src={f.thumbnailUrl ?? f.url} alt={f.name} loading="lazy" class="h-full w-full object-cover" />
          {:else}
            <span class="text-muted-foreground">{f.mimeType.split("/")[0] || "file"}</span>
          {/if}
          {#if selected.has(f.id)}
            <span
              class="absolute top-1 right-1 flex size-[18px] items-center justify-center rounded-full bg-primary text-white"
              ><Check size={16} /></span
            >
          {/if}
        </button>
      {/each}
    </div>
    {#if !noMoreFiles && files.length > 0}
      <Button
        type="button"
        variant="outline"
        size="sm"
        class="mt-2 self-center"
        disabled={loadingMore}
        onclick={loadMore}
      >
        {loadingMore ? "読み込み中…" : "もっと見る"}
      </Button>
    {/if}
  {/if}

  {#if err}<p class="mt-2 flex-none text-sm break-words text-destructive">{err}</p>{/if}

  <div class="mt-3 flex flex-none items-center justify-between">
    <span class="text-sm text-muted-foreground">選択中 {selected.size}件</span>
    <Button type="button" variant="default" size="sm" disabled={selected.size === 0} onclick={confirm}
      >添付</Button
    >
  </div>
</Modal>
```

- [ ] **Step 4: テストを実行し、すべて通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/DrivePicker.test.ts`
Expected: PASS(全3件)

- [ ] **Step 5: `pnpm check`を実行し、型エラーがないことを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし(未使用importの`X`削除漏れがあれば解消する)

- [ ] **Step 6: フロントエンド全体のテストを実行する**

Run: `cd frontend && pnpm test`
Expected: 全件PASS(既存の`Modal.svelte`利用箇所に回帰がないこと)

- [ ] **Step 7: コミット**

```bash
cd frontend && git add src/ui/DrivePicker.svelte src/ui/DrivePicker.test.ts
git commit -m "refactor: DrivePicker.svelteを共有Modal.svelteに統一"
```

---

### Task 4: 実機での目視確認(issue確認事項)

**Files:** なし(手動確認のみ)

- [ ] **Step 1: `cargo tauri dev`を起動する**

Run(リポジトリルートから): `cargo tauri dev`

- [ ] **Step 2: DrivePickerを確認する**

添付メニュー→「ドライブから選択」を開き、以下を確認する:
- パンくず・グリッドが表示され、グリッドをスクロールしても外側のオーバーレイ全体はスクロールしない(グリッド内部だけがスクロールする)。
- Escapeキーで閉じる。オーバーレイクリックで閉じる。×ボタンで閉じる。
- 他のモーダル(Settings等)を同時に開いても重なり順が破綻しない。

- [ ] **Step 3: ColumnSettingsを確認する**

カラムのグリップをダブルクリックして開き、以下を確認する:
- 幅/高さのラジオ・数値入力が正しく反映される。
- Escape・オーバーレイクリック・×ボタンで閉じる。

- [ ] **Step 4: 起動した`cargo tauri dev`を終了する**

確認が終わったら、自分で起動した`cargo tauri dev`プロセスを終了する。

- [ ] **Step 5: 確認結果をコミットログ等に残さず、そのままPR作成に進む**

(このタスクはコード変更を伴わないため、コミットなし)
