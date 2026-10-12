# モバイルのモーダル全画面化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** モバイルUI(`app.useMobileUi()`が真)で、`Modal.svelte`を使うモーダルを全画面ページ(「←」+タイトル)として表示し、戻るジェスチャ/ボタンで閉じられるようにする(Issue #450)。

**Architecture:** `Modal.svelte`が`useMobileUi()`に応じて箱型/全画面を切り替える(呼び出し側は原則無改修)。全画面時だけ`lib/modalHistory.ts`で`history.pushState`を積み、`popstate`で最上段のモーダルを閉じる。`Settings`は上部の横スクロールタブ帯、モバイル投稿オーバーレイは`Modal`へ置き換え。エラー表示2箇所は`compact`で従来の箱型を維持する。

**Tech Stack:** Svelte 5 (runes), Tailwind v4, Vitest + @testing-library/svelte (jsdom), lucide-svelte

**Spec:** `docs/superpowers/specs/2026-10-12-mobile-fullscreen-modal-design.md`

## Global Constraints

- PC版の見た目・挙動は変えない。`Modal.test.ts`の既存テスト(PC側)は無変更で通ること。
- `data-testid="modal-close"`は全画面でも維持する(E2E `helpers/mobile.ts`・`helpers/appInspect.ts`等が依存)。
- `env(safe-area-inset-*)`を直接書かず`var(--safe-top)`/`var(--safe-bottom)`を使う(`docs/design/style-guide.md` §11)。
- 角丸・フォントサイズ・アイコンサイズは`docs/design/style-guide.md`のスケールに従い、`rounded-[Npx]`等の一回限りの値を作らない。アイコンは`size={16}`が既定、全画面ヘッダの戻る矢印のみ`size={20}`(§6の「大きめのアイコン」例外)。
- テストファイルのモック(`@tauri-apps/plugin-os`等)は既存の`ColumnSettings.test.ts`/`Settings.test.ts`の書き方を踏襲する。
- コミットメッセージは件名のみ(本文・箇条書き禁止)。Co-Authored-Byトレーラーは別途付与される。コミットが失敗/タイムアウトしたらリトライせず報告する。`--no-verify`禁止。
- 作業ブランチは`feat/issue-450-mobile-fullscreen-modal`(作成済み)。`main`に直接コミットしない。
- 実画面でUIを確認するときは必ずXvfb等の仮想ディスプレイ越しで行い、自分で起動した`cargo tauri dev`等はタスク完了前に自分でkillする(`pkill`/`killall`は禁止、PID指定のみ)。

## Review Focus

- モーダルを閉じた直後(同一tick)に別のモーダルを開く: `history.back()`の完了前に`pushState`すると、後から来る`back`が新しい履歴を消してしまう。→ Task 1のdeferred pushテストで固定する。
- 同一tickで2枚を連続して閉じる: 無視カウンタが2になり、どちらも二重に`onclose`されない。→ Task 1。
- 全画面モーダルの上でエラー表示(`compact`)を開閉する: `compact`は履歴に触れず、下の全画面モーダルの履歴を壊さない。→ Task 2。
- モーダルを開いたままUIモードをモバイル⇄PCに切り替える: 全画面でなくなった時点で積んだ履歴が解放される。→ Task 2。
- モバイルの`Settings`で、開発者オプション解除により動的にタブが増える: タブ帯にも反映される。→ Task 3。

---

### Task 1: `lib/modalHistory.ts`(履歴スタック管理)

**Files:**
- Create: `frontend/src/lib/modalHistory.ts`
- Test: `frontend/src/lib/modalHistory.test.ts`

**Interfaces:**
- Consumes: なし(`window.history` / `popstate`のみ)
- Produces: `pushModalHistory(onclose: () => void): () => void`。戻り値は「UI側で閉じたとき/アンマウント時に呼ぶrelease関数」。release済みの再呼び出しは何もしない。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/modalHistory.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// history.back()のpopstateは非同期なので、イベントループを少し回して待つ
const settle = () => new Promise<void>((r) => setTimeout(r, 20));

let mod: typeof import("./modalHistory");
const releases: Array<() => void> = [];

function open(onclose = vi.fn()) {
  const release = mod.pushModalHistory(onclose);
  releases.push(release);
  return { onclose, release };
}

beforeEach(async () => {
  // モジュール内のスタック/カウンタをテストごとに初期化する
  vi.resetModules();
  mod = await import("./modalHistory");
});

afterEach(async () => {
  releases.splice(0).forEach((r) => r());
  await settle();
  vi.restoreAllMocks();
});

describe("modalHistory", () => {
  it("開くと履歴を1つ積む", () => {
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).toHaveBeenCalledTimes(1);
    expect(push.mock.calls[0][0]).toEqual({ tsumugiModal: expect.any(Number) });
  });

  it("UIで閉じると履歴を1つ戻し、そのpopstateではoncloseを呼ばない", async () => {
    const back = vi.spyOn(history, "back");
    const { onclose, release } = open();
    release();
    expect(back).toHaveBeenCalledTimes(1);
    await settle();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("戻る操作で最上段のoncloseを呼び、その後のreleaseではhistory.backしない", async () => {
    const { onclose, release } = open();
    history.back();
    await settle();
    expect(onclose).toHaveBeenCalledTimes(1);
    const back = vi.spyOn(history, "back");
    release();
    expect(back).not.toHaveBeenCalled();
  });

  it("重なっていると戻る操作ごとに1枚ずつ閉じる", async () => {
    const a = open();
    const b = open();
    history.back();
    await settle();
    expect(b.onclose).toHaveBeenCalledTimes(1);
    expect(a.onclose).not.toHaveBeenCalled();
    b.release();
    history.back();
    await settle();
    expect(a.onclose).toHaveBeenCalledTimes(1);
  });

  it("同一tickに2枚閉じても二重にoncloseせず、その後のpushは即時に行われる", async () => {
    const back = vi.spyOn(history, "back");
    const a = open();
    const b = open();
    b.release();
    a.release();
    expect(back).toHaveBeenCalledTimes(2);
    await settle();
    expect(a.onclose).not.toHaveBeenCalled();
    expect(b.onclose).not.toHaveBeenCalled();
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).toHaveBeenCalledTimes(1);
  });

  it("閉じた直後に開いたモーダルのpushは、history.backの完了まで遅延される", async () => {
    const a = open();
    a.release();
    const push = vi.spyOn(history, "pushState");
    open();
    expect(push).not.toHaveBeenCalled();
    await settle();
    expect(push).toHaveBeenCalledTimes(1);
    expect(push.mock.calls[0][0]).toEqual({ tsumugiModal: expect.any(Number) });
  });

  it("releaseの二重呼び出しは何もしない", () => {
    const back = vi.spyOn(history, "back");
    const { release } = open();
    release();
    release();
    expect(back).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && npx vitest run src/lib/modalHistory.test.ts`
Expected: FAIL(`./modalHistory`が存在しない)

- [ ] **Step 3: 実装を書く**

`frontend/src/lib/modalHistory.ts`:

```ts
// モバイルの全画面モーダル用の履歴管理(Issue #450)。
//
// 全画面表示中のモーダルごとに history.pushState で履歴を1つ積み、戻るジェスチャ/ボタン
// (Android WebViewではgoBack→popstate)で最上段のモーダルだけを閉じる。
// UI側(←ボタン・Escape・保存後の自動クローズ)で閉じたときは、積んだ履歴を history.back() で
// 戻し、その popstate は無視カウンタで握りつぶして onclose を二重に呼ばない。

type Entry = { id: number; onclose: () => void; popped: boolean };

const stack: Entry[] = [];
// history.back() 完了待ちの間に積もうとしたエントリ。back は非同期なので、完了前に
// pushState すると後から来る back が新しい履歴を消してしまう。
const deferred: Entry[] = [];
let nextId = 0;
// UI側で閉じたことによる history.back() の popstate をあと何回無視するか
let ignoredPops = 0;

function doPush(entry: Entry) {
  history.pushState({ tsumugiModal: entry.id }, "");
}

function flushDeferred() {
  for (const entry of deferred.splice(0)) doPush(entry);
}

function onPopState() {
  if (ignoredPops > 0) {
    ignoredPops--;
    if (ignoredPops === 0) flushDeferred();
    return;
  }
  const top = stack[stack.length - 1];
  if (!top) return;
  // 戻る操作で履歴は既に1つ戻っているため、アンマウント時に back() しない
  top.popped = true;
  top.onclose();
}

if (typeof window !== "undefined") {
  window.addEventListener("popstate", onPopState);
}

/// 全画面モーダルの表示開始時に呼ぶ。戻り値のreleaseは、モーダルが閉じられたとき(アンマウント時)に呼ぶ。
export function pushModalHistory(onclose: () => void): () => void {
  const entry: Entry = { id: ++nextId, onclose, popped: false };
  stack.push(entry);
  if (ignoredPops > 0) deferred.push(entry);
  else doPush(entry);

  return () => {
    const i = stack.indexOf(entry);
    if (i < 0) return;
    stack.splice(i, 1);
    const d = deferred.indexOf(entry);
    if (d >= 0) {
      // まだ履歴を積んでいないので戻す必要も無い
      deferred.splice(d, 1);
      return;
    }
    if (entry.popped) return;
    ignoredPops++;
    history.back();
  };
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && npx vitest run src/lib/modalHistory.test.ts`
Expected: PASS(7 tests)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/modalHistory.ts frontend/src/lib/modalHistory.test.ts
git commit -m "feat: 全画面モーダル用の履歴スタック管理を追加する(#450)"
```

---

### Task 2: `Modal.svelte`のモバイル全画面化と`compact`

**Files:**
- Modify: `frontend/src/ui/Modal.svelte`(全体)
- Modify: `frontend/src/App.svelte:295`(エラー表示に`compact`)
- Modify: `frontend/src/ui/ComposeBar.svelte:1208`(エラー表示に`compact`)
- Test: `frontend/src/ui/Modal.test.ts`

**Interfaces:**
- Consumes: Task 1の`pushModalHistory(onclose: () => void): () => void`、`app.useMobileUi(): boolean`(`lib/store.svelte.ts:1582`)
- Produces: `Modal`の新prop `compact?: boolean`(既定false)。既存prop(`title`/`onclose`/`children`/`width`/`maxHeight`)は不変。

- [ ] **Step 1: Modal.test.tsにモックとモバイル用テストを追加(失敗するテスト)**

`frontend/src/ui/Modal.test.ts`の先頭(import群)を次のように書き換える。`Modal`を静的importから動的importに変え、ストア依存のtauriプラグインをモックする(既存のPC側テストは本体を変更しない):

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";

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

const { default: Modal } = await import("./Modal.svelte");
const { app } = await import("../lib/store.svelte");

const settle = () => new Promise<void>((r) => setTimeout(r, 20));

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(async () => {
  cleanup();
  app.ui = { ...app.ui, uiMode: "auto" };
  // 全画面モーダルのhistory.back()によるpopstateを次のテストに持ち越さない
  await settle();
  vi.restoreAllMocks();
});
```

(元の`import Modal from "./Modal.svelte";`・`import { createRawSnippet } from "svelte";`・元の`textSnippet`・元の`afterEach`は上記に置き換えて削除する。)

ファイル末尾に新しい`describe`を追加:

```ts
describe("Modal モバイル全画面(Issue #450)", () => {
  function mobile() {
    app.ui = { ...app.ui, uiMode: "mobile" };
  }

  it("全画面レイアウトになり、黒オーバーレイ・角丸の箱ではない", async () => {
    mobile();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const overlay = getByRole("presentation");
    expect(overlay.className).toMatch(/bg-background/);
    expect(overlay.className).not.toMatch(/bg-black/);
    const dialog = getByRole("dialog");
    expect(dialog.className).toMatch(/h-full/);
    expect(dialog.className).not.toMatch(/rounded-xl/);
  });

  it("width/maxHeightは無視される", async () => {
    mobile();
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), width: "640px", maxHeight: "78vh" },
    });
    await tick();
    const dialog = getByRole("dialog");
    expect(dialog.style.maxHeight).toBe("");
    expect(dialog.style.getPropertyValue("--modal-w")).toBe("");
  });

  it("背景クリックでは閉じず、戻るボタンとEscapeでは閉じる", async () => {
    mobile();
    const onclose = vi.fn();
    const { getByRole, getByTestId } = render(Modal, {
      props: { title: "設定", onclose, children: textSnippet("hi") },
    });
    await tick();
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).not.toHaveBeenCalled();
    await fireEvent.keyDown(getByRole("presentation"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
    await fireEvent.click(getByTestId("modal-close"));
    expect(onclose).toHaveBeenCalledTimes(2);
  });

  it("開くと履歴を1つ積み、戻る操作でoncloseが呼ばれる", async () => {
    mobile();
    const push = vi.spyOn(history, "pushState");
    const onclose = vi.fn();
    render(Modal, { props: { title: "設定", onclose, children: textSnippet("hi") } });
    await tick();
    expect(push).toHaveBeenCalledTimes(1);
    history.back();
    await settle();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("アンマウントで積んだ履歴を戻す", async () => {
    mobile();
    const back = vi.spyOn(history, "back");
    const onclose = vi.fn();
    const { unmount } = render(Modal, { props: { title: "設定", onclose, children: textSnippet("hi") } });
    await tick();
    unmount();
    expect(back).toHaveBeenCalledTimes(1);
    await settle();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("開いたままUIモードをPC版に切り替えると履歴を解放し箱型に戻る", async () => {
    mobile();
    const back = vi.spyOn(history, "back");
    const { getByRole } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    app.ui = { ...app.ui, uiMode: "desktop" };
    await tick();
    expect(back).toHaveBeenCalledTimes(1);
    expect(getByRole("dialog").className).toMatch(/rounded-xl/);
  });

  it("compactはモバイルでも箱型で、履歴に触れない", async () => {
    mobile();
    const push = vi.spyOn(history, "pushState");
    const back = vi.spyOn(history, "back");
    const onclose = vi.fn();
    const { getByRole, unmount } = render(Modal, {
      props: { title: "エラー", onclose, children: textSnippet("hi"), compact: true },
    });
    await tick();
    expect(getByRole("dialog").className).toMatch(/rounded-xl/);
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledTimes(1);
    unmount();
    expect(push).not.toHaveBeenCalled();
    expect(back).not.toHaveBeenCalled();
  });

  it("全画面モーダルの上でcompactを開閉しても下の履歴は壊れない", async () => {
    mobile();
    const back = vi.spyOn(history, "back");
    const base = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("base") },
    });
    await tick();
    const err = render(Modal, {
      props: { title: "エラー", onclose: () => {}, children: textSnippet("err"), compact: true },
    });
    await tick();
    err.unmount();
    expect(back).not.toHaveBeenCalled();
    base.unmount();
    expect(back).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && npx vitest run src/ui/Modal.test.ts`
Expected: 既存のPC側テストはPASS、新しい`describe`はFAIL(全画面クラス/`compact`/履歴が未実装)

- [ ] **Step 3: `Modal.svelte`を書き換える**

`frontend/src/ui/Modal.svelte`全体を次に置き換える:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { ArrowLeft, X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { app } from "../lib/store.svelte";
  import { pushModalHistory } from "../lib/modalHistory";

  let {
    title,
    onclose,
    children,
    width = "480px",
    maxHeight,
    compact = false,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
    maxHeight?: string;
    // true ならモバイルでも従来の箱型ダイアログのまま表示する(エラー表示など短い通知用)。
    compact?: boolean;
  } = $props();

  // モバイルUIでは全画面ページとして表示する(Issue #450)。
  const fullscreen = $derived(!compact && app.useMobileUi());

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

  // 全画面表示中だけ履歴を1つ積み、戻るジェスチャ/ボタンで閉じられるようにする。
  // アンマウント時・全画面でなくなった時にreleaseが呼ばれて履歴を戻す。
  $effect(() => {
    if (!fullscreen) return;
    return pushModalHistory(() => onclose());
  });
</script>

<div
  class={fullscreen
    ? "fixed inset-0 z-[1000] bg-background"
    : "fixed inset-0 z-[1000] grid items-start justify-items-center overflow-y-auto bg-black/45 pt-[max(8vh,var(--safe-top))] pb-[max(8vh,var(--safe-bottom))]"}
  use:portal
  onclick={fullscreen ? undefined : onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class={fullscreen
      ? "flex h-full flex-col outline-none"
      : `w-[min(var(--modal-w),92vw)] rounded-xl border border-border bg-background p-4 ${maxHeight ? "flex flex-col" : ""}`}
    style={fullscreen ? undefined : `--modal-w:${width};${maxHeight ? ` max-height:${maxHeight};` : ""}`}
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    {#if fullscreen}
      <header
        class="flex h-[calc(3rem+var(--safe-top))] flex-none items-center gap-1 border-b border-border px-2 pt-[var(--safe-top)] font-semibold"
      >
        <Button variant="ghost" size="icon" onclick={onclose} data-testid="modal-close" aria-label="戻る"
          ><ArrowLeft size={20} /></Button
        >
        <span class="min-w-0 truncate">{title}</span>
      </header>
      {#if maxHeight}
        <div class="flex min-h-0 flex-1 flex-col overflow-hidden p-4 pb-[max(1rem,var(--safe-bottom))]">
          <div class="flex min-h-0 flex-1 flex-col">
            {@render children()}
          </div>
        </div>
      {:else}
        <div class="min-h-0 flex-1 overflow-y-auto p-4 pb-[max(1rem,var(--safe-bottom))]">
          {@render children()}
        </div>
      {/if}
    {:else}
      <header class="mb-3 flex flex-none items-center justify-between font-semibold">
        <span>{title}</span>
        <Button variant="ghost" size="icon-xs" onclick={onclose} data-testid="modal-close"><X size={16} /></Button>
      </header>
      {#if maxHeight}
        <div class="flex flex-1 flex-col min-h-0">
          {@render children()}
        </div>
      {:else}
        {@render children()}
      {/if}
    {/if}
  </div>
</div>
```

- [ ] **Step 4: エラー表示2箇所に`compact`を付ける**

`frontend/src/App.svelte:295`:

```svelte
    <Modal title="エラー" compact onclose={() => (app.errorModal = null)}>
```

`frontend/src/ui/ComposeBar.svelte:1208`:

```svelte
  <Modal title="エラー" compact onclose={() => (err = null)}>
```

- [ ] **Step 5: テストが通ることを確認**

Run: `cd frontend && npx vitest run src/ui/Modal.test.ts`
Expected: PASS(既存5件 + 新規8件)

- [ ] **Step 6: Modalを使う他のテストが壊れていないことを確認**

Run: `cd frontend && pnpm test`
Expected: 全件PASS。`Modal.svelte`がストアをimportするようになったため、tauriプラグインを未モックのまま`Modal`を(間接的に)importしているテストがあればここで失敗する。その場合は`ColumnSettings.test.ts`冒頭と同じ`vi.mock(...)`群をそのテストに足す。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/Modal.svelte frontend/src/ui/Modal.test.ts frontend/src/App.svelte frontend/src/ui/ComposeBar.svelte
git commit -m "feat: モバイルでModalを全画面表示にし戻る操作で閉じられるようにする(#450)"
```

(`pnpm test`で他のテストにモックを足した場合は、そのファイルも`git add`に含める。)

---

### Task 3: `Settings`のモバイル用タブ帯

**Files:**
- Modify: `frontend/src/ui/Settings.svelte:14-129`
- Test: `frontend/src/ui/Settings.test.ts`

**Interfaces:**
- Consumes: Task 2の全画面`Modal`(本文が`overflow-y-auto p-4`の1枚スクロール)、`app.useMobileUi()`
- Produces: モバイル時のみ`data-testid="settings-tab-strip"`の横スクロールタブ帯。既存の`data-testid="settings-tab-{id}"`は両モードで維持。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/Settings.test.ts`の`afterEach`を次に変更(`uiMode`もリセット):

```ts
afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.ui = { ...app.ui, developerOptionsEnabled: false, uiMode: "auto" };
});
```

ファイル末尾に追加:

```ts
describe("Settings モバイルのタブ帯(Issue #450)", () => {
  it("PC版ではタブ帯(settings-tab-strip)を出さない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-strip")).toBeNull();
  });

  it("モバイル版では横スクロールのタブ帯を出し、タブ切り替えができる", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    const { getByTestId } = renderSettings();
    const strip = getByTestId("settings-tab-strip");
    expect(strip.className).toMatch(/overflow-x-auto/);
    const layout = getByTestId("settings-tab-layout");
    expect(strip.contains(layout)).toBe(true);
    expect(layout.getAttribute("aria-selected")).toBe("false");
    await fireEvent.click(layout);
    expect(layout.getAttribute("aria-selected")).toBe("true");
  });

  it("モバイル版でも開発者オプション解除でタブ帯にタブが動的に現れる", async () => {
    app.ui = { ...app.ui, uiMode: "mobile" };
    const { getByTestId, queryByTestId, findByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).toBeNull();
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const dev = await findByTestId("settings-tab-developer");
    expect(getByTestId("settings-tab-strip").contains(dev)).toBe(true);
  });
});
```

import行を`import { cleanup, fireEvent, render } from "@testing-library/svelte";`に変更する(`fireEvent`を追加)。

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && npx vitest run src/ui/Settings.test.ts`
Expected: 新規3件のうち2件がFAIL(`settings-tab-strip`が無い)。PC版のテストと既存テストはPASS

- [ ] **Step 3: `Settings.svelte`を実装する**

`<script>`内、`let active = ...`の直後に追加:

```ts
  // モバイルUIでは左タブ+右ペインをやめ、上部の横スクロールタブ帯+1カラム本文にする(Issue #450)。
  const mobileUi = $derived(app.useMobileUi());

  // 選択中のタブがタブ帯の外にあれば見える位置までスクロールする
  let stripEl = $state<HTMLElement | null>(null);
  $effect(() => {
    void active;
    stripEl?.querySelector('[aria-selected="true"]')?.scrollIntoView({ inline: "center", block: "nearest" });
  });
```

テンプレート(`<Modal title="設定" ...>`以降)は、現在の`{#if active === "accounts"}…{/if}`の分岐チェーンを`snippet`に切り出し、PC/モバイル両方で`{@render sections()}`する形に書き換える。分岐チェーン本体(`AccountsSection`〜`DeveloperSection`の`{#if}…{/if}`)は**1文字も変えずに**そのまま`{#snippet sections()}`の中へ移動すること。

```svelte
{#snippet sections()}
  {#if active === "accounts"}
    <AccountsSection {onAddAccount} {onReauth} />
  {:else if active === "layout"}
    <!-- …現在の分岐チェーンを、そのまま全て移動する… -->
  {/if}
{/snippet}

<Modal title="設定" {onclose} width="640px">
  {#if mobileUi}
    <!-- Modal(全画面)本文のp-4を-mx-4/-mt-4で打ち消し、タブ帯を左右いっぱい・上端に張り付ける。
         下端のp-4(+セーフエリア)は残す。 -->
    <div class="-mx-4 -mt-4">
      <div
        bind:this={stripEl}
        class="sticky top-0 z-10 flex gap-1 overflow-x-auto border-b border-border bg-background px-3 py-2"
        role="tablist"
        data-testid="settings-tab-strip"
      >
        {#each nav as item (item.id)}
          <button
            type="button"
            role="tab"
            aria-selected={active === item.id}
            class={active === item.id
              ? "flex-none rounded-md bg-primary px-3 py-2 text-sm whitespace-nowrap text-primary-foreground"
              : "flex-none rounded-md px-3 py-2 text-sm whitespace-nowrap text-foreground"}
            data-testid={`settings-tab-${item.id}`}
            onclick={() => (active = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </div>
      <section class="min-w-0 px-4 py-4">
        {@render sections()}
      </section>
    </div>
  {:else}
    <!-- rounded-b-[11px]はModal.svelteのrounded-xl(12px) - border(1px)を差し引いた値
         （このdivはModalの内側にネガティブマージンで縁までにじみ出すため、外側の角丸に沿わせる必要がある）。
         スタイルガイド(docs/design/style-guide.md §2)が認める「親要素のborder-width分を差し引く」例外。 -->
    <div class="-mx-4 -mb-4 flex max-h-[calc(84vh-3rem)] flex-col overflow-hidden rounded-b-[11px]">
      <div class="flex min-h-0 flex-1 border-t border-border">
        <nav class="flex w-40 flex-none flex-col gap-0.5 overflow-y-auto border-r border-border bg-muted px-2 py-2.5">
          {#each nav as item (item.id)}
            <button
              type="button"
              class={active === item.id
                ? "rounded-md bg-primary px-2.5 py-2 text-left text-sm text-primary-foreground"
                : "rounded-md px-2.5 py-2 text-left text-sm text-foreground hover:bg-background"}
              data-testid={`settings-tab-${item.id}`}
              onclick={() => (active = item.id)}
            >
              {item.label}
            </button>
          {/each}
        </nav>
        <section class="min-w-0 flex-1 overflow-y-auto px-5 py-[18px]">
          {@render sections()}
        </section>
      </div>
    </div>
  {/if}
</Modal>
```

PC側の`<nav>`は現状のまま(`aria-selected`/`role`は足さない)。モバイル側のみ`role="tab"`+`aria-selected`を持つ。

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && npx vitest run src/ui/Settings.test.ts`
Expected: PASS(既存4件 + 新規3件)

- [ ] **Step 5: 型チェック**

Run: `cd frontend && pnpm check`
Expected: エラー0件

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/Settings.svelte frontend/src/ui/Settings.test.ts
git commit -m "feat: モバイルの設定画面を上部の横スクロールタブ帯にする(#450)"
```

---

### Task 4: モバイル投稿オーバーレイを`Modal`へ置き換え

**Files:**
- Modify: `frontend/src/App.svelte:~234-256`(`{#if app.showComposeModal}`ブロック)

**Interfaces:**
- Consumes: Task 2の`Modal`(全画面)、`ComposeBar`の既存props(`onPosted`、`expanded`)
- Produces: なし

- [ ] **Step 1: 現状のブロックを確認**

Run: `cd frontend && grep -n "showComposeModal" src/App.svelte`
Expected: `{#if app.showComposeModal}`ブロック(独自`fixed inset-0 z-50`オーバーレイ)が見つかる。このブロック全体(`{#if app.showComposeModal}`から対応する`{/if}`まで)が置き換え対象。

- [ ] **Step 2: 置き換える**

該当ブロック全体を次に置き換える:

```svelte
  {#if app.showComposeModal}
    <Modal title="投稿" onclose={() => (app.showComposeModal = false)}>
      {#snippet children()}
        <ComposeBar onPosted={() => (app.showComposeModal = false)} expanded />
      {/snippet}
    </Modal>
  {/if}
```

- [ ] **Step 3: 型チェックと全テスト**

Run: `cd frontend && pnpm check && pnpm test`
Expected: すべてPASS(`App.svelte`のimportは変更不要: `Modal`と`ComposeBar`は既にimport済み)

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.svelte
git commit -m "feat: モバイルの投稿画面を共通Modalの全画面表示に置き換える(#450)"
```

---

### Task 5: ドキュメント更新と最終検証

**Files:**
- Modify: `docs/design/style-guide.md`(§13の前に新セクション、旧§13を§14へ)
- Modify: `docs/guide/user-guide.md:93,97`

**Interfaces:**
- Consumes: Task 1〜4の成果
- Produces: なし

- [ ] **Step 1: スタイルガイドにモーダル規約を追記**

`docs/design/style-guide.md`の`## 13. 今後`を`## 14. 今後`に改め、その直前に次のセクションを挿入する:

```markdown
## 13. モーダル(モバイル)

`ui/Modal.svelte`はモバイルUI(`app.useMobileUi()`が真)では全画面ページとして表示される(Issue #450)。上部バーは「← タイトル」、本文は`Modal`が1枚だけスクロールし、背景クリックでは閉じない。戻るジェスチャ/ボタンでも閉じられるよう、全画面表示中は`lib/modalHistory.ts`が`history.pushState`を積む。

- 新しい大きめの画面(設定・検索・フォーム等)は`Modal`をそのまま使う。モバイルでは`width`/`maxHeight`は無視される。
- エラー表示など短い通知は`<Modal compact ...>`にして、モバイルでも従来の箱型ダイアログのままにする(履歴にも参加しない)。確認ダイアログは`ConfirmDialog`を使う。
- 本文の左右余白は`Modal`の`p-4`。全幅で見せたい要素(`Settings`のタブ帯)は`-mx-4`で打ち消す。
```

- [ ] **Step 2: ユーザーガイドを更新**

`docs/guide/user-guide.md`の、「モバイル版でのタブ/カラム並び替え」の項目(93行目)の直後に追加:

```markdown
- **モバイル版での画面表示**: 設定・検索・プロフィール・カラム追加などは全画面で開き、左上の「←」または端末の戻る操作で閉じます。エラー表示や確認ダイアログは、従来どおり小さなダイアログで表示されます。
```

97行目の「…代わりに新規投稿ボタンからモーダルで投稿します）。」を「…代わりに新規投稿ボタンから全画面の投稿画面で投稿します）。」に修正する。

- [ ] **Step 3: 自動検証**

Run: `cd frontend && pnpm check && pnpm test`
Expected: すべてPASS

- [ ] **Step 4: 実画面確認(仮想ディスプレイ越し)**

`docs/design/style-guide.md`と`e2e/README.md`の手順に従い、Xvfb越し(ユーザーの実画面を使わない。Waylandのため`WAYLAND_DISPLAY`もunset、隔離したXDGパスとdbus-run-session)でアプリを起動し、設定の「表示」でUIモードを「モバイル版」に切り替えて次を目視確認する(画像は自分で見る):
- 設定・カラム追加・検索・プロフィール・フォロー一覧・ドライブ選択・予約一覧・カラム設定が全画面になり、「←」で閉じる。Escapeでも閉じる。
- 設定の上部タブ帯が横スクロールし、選択中タブが見える位置に来る。
- エラー表示は従来の箱型ダイアログのまま。
- PC版UIモードでは見た目が従来どおり。

E2E: `e2e/README.md`の手順で`wdio.mobile.conf.ts`(`specs-mobile/`)を実行し、モーダル関連のセレクタ(`modal-close`)が通ることを確認する。起動したプロセス・開発サーバは、確認後に自分でPID指定でkillする。

- [ ] **Step 5: Commit**

```bash
git add docs/design/style-guide.md docs/guide/user-guide.md
git commit -m "docs: モバイルのモーダル全画面化をスタイルガイドとユーザーガイドに反映する(#450)"
```

- [ ] **Step 6: Android実機確認の項目を引き継ぐ**

PR本文(`.github/pull_request_template.md`の構成に従う。`Fixes #450`を必ず入れる)の「未確認項目」に、仕様書「実機確認(Android)」の4項目を転記する:
1. `pushState`の履歴が戻るジェスチャで辿れるか(最大の未検証点。ダメならKotlin側でJSブリッジ経由の戻る処理に切り替える)
2. ソフトキーボード表示時の投稿画面で入力欄が隠れないか(隠れるなら`dvh`系の高さ指定)
3. 上下のセーフエリアが正しく確保されるか
4. `ProfileModal` → `FollowListModal`など重なったときの戻る挙動
