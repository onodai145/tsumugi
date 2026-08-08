# モーダル基盤コンポーネントのTailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第2バッチとして、`Modal.svelte`/`ConfirmDialog.svelte`の手書きCSSをTailwindユーティリティクラスへ置き換え、既存のshadcn Buttonプリミティブ(#176で導入済み)を使う。

**Architecture:** 両コンポーネントの`<style>`ブロックを完全に削除し、Tailwindユーティリティクラスに置き換える。portal実装・Escapeキー・外側クリックでの閉じる処理は一切変更しない。shadcn Dialogプリミティブへの置き換えは行わない(スコープ外、理由はspec参照)。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ、shadcn-svelte Buttonプリミティブ(`$lib/components/ui/button`、新規追加なし)

## Global Constraints

- portal(`document.body.appendChild`)、Escapeキー検知、外側クリックでの閉じる処理、`role="dialog"`/`aria-modal="true"`等のアクセシビリティ属性、`$effect`によるフォーカス処理は一切変更しない
- shadcn Dialogプリミティブへの置き換えは行わない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--border`→`border-border`、`--text`→`text-foreground`
- `border-radius: 14px`は`rounded-[14px]`のアービトラリ値、`z-index: 1000`は`z-[1000]`のアービトラリ値を使う
- Rust側・`theme.ts`・`@theme`ブリッジ(`frontend/src/app.css`)は変更しない
- Buttonプリミティブは既存のものを`import { Button } from "$lib/components/ui/button";`でそのまま使う(shadcn-svelte CLIの再実行は不要)

---

### Task 1: `Modal.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/Modal.svelte`

**Interfaces:**
- Consumes: 既存の`Button`(`$lib/components/ui/button`)
- Produces: 見た目・挙動は現状維持。`ProfileModal.svelte`/`FollowListModal.svelte`/`ComposeBar.svelte`がこのコンポーネントを`<Modal>`として使い続けられる(Props: `title: string`, `onclose: () => void`, `children: Snippet` は変更しない)

現在の`Modal.svelte`の内容(全文):

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";

  let { title, onclose, children }: { title: string; onclose: () => void; children: Snippet } =
    $props();

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
  class="overlay"
  use:portal
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal"
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="head">
      <span>{title}</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>
    {@render children()}
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
    z-index: 1000;
  }
  .modal {
    width: min(480px, 92vw);
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
    margin-bottom: 12px;
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
</style>
```

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`<script lang="ts">`ブロック冒頭のimport群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: マークアップを置き換え、`<style>`ブロックを削除**

`<div class="overlay" ...>`以降を以下に置き換える(`<script>`のロジックは変更しない、Step 1のimport追加のみ):

```svelte
<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  use:portal
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(480px,92vw)] rounded-[14px] border border-border bg-background p-4"
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-3 flex items-center justify-between font-semibold">
      <span>{title}</span>
      <Button variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>
    {@render children()}
  </div>
</div>
```

`<style>`ブロック全体を削除する。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、`Modal.svelte`を使う画面(プロフィール表示、フォロー一覧、投稿欄の一部機能)を開いて以下を確認する:
- モーダルが画面上部寄り中央に表示される、半透明の背景オーバーレイ
- 閉じる(×)ボタンの見た目・クリック動作
- Escapeキー・背景クリックでの閉じる動作
- 見た目の崩れが無いこと

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/Modal.svelte
git commit -m "style: Modal.svelteをTailwindクラス+Buttonプリミティブに移行"
```

---

### Task 2: `ConfirmDialog.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/ConfirmDialog.svelte`

**Interfaces:**
- Consumes: 既存の`Button`(`$lib/components/ui/button`)。Task 1と独立して実施可能
- Produces: 見た目・挙動は現状維持。Props(`title?`, `message`, `confirmLabel?`, `cancelLabel?`, `danger?`, `onConfirm`, `onCancel`)は変更しない

現在の`ConfirmDialog.svelte`の内容(全文):

```svelte
<script lang="ts">
  // 汎用の確認モーダル。深くネストされたコンポーネント(NoteCard等)から呼ばれても
  // content-visibility/containの包含ブロックを脱出できるよう portal で body 直下に置く。
  let {
    title = "確認",
    message,
    confirmLabel = "OK",
    cancelLabel = "キャンセル",
    danger = false,
    onConfirm,
    onCancel,
  }: {
    title?: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<div
  class="overlay"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">{title}</header>
    <p class="msg">{message}</p>
    <div class="actions">
      <button class="cancel" onclick={onCancel}>{cancelLabel}</button>
      <button class="confirm" class:danger onclick={onConfirm}>{confirmLabel}</button>
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
    z-index: 1000;
  }
  .modal {
    width: min(360px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    font-weight: 600;
    margin-bottom: 10px;
  }
  .msg {
    font-size: 0.85rem;
    color: var(--text);
    margin: 0 0 16px;
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .cancel,
  .confirm {
    padding: 7px 16px;
    border: none;
    border-radius: 8px;
    font-family: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .cancel {
    background: var(--surface-2);
    color: var(--text);
  }
  .confirm {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .confirm.danger {
    background: var(--danger);
  }
</style>
```

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`<script lang="ts">`ブロック冒頭に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

(`let { title = "確認", ... }: {...} = $props();` と `function portal(...)` はそのまま変更しない)

- [ ] **Step 2: マークアップを置き換え、`<style>`ブロックを削除**

`<div class="overlay" ...>`以降を以下に置き換える:

```svelte
<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(360px,92vw)] rounded-[14px] border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-2.5 font-semibold">{title}</header>
    <p class="mb-4 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
    <div class="flex justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={onCancel}>{cancelLabel}</Button>
      <Button variant={danger ? "destructive" : "default"} size="sm" onclick={onConfirm}>{confirmLabel}</Button>
    </div>
  </div>
</div>
```

`<style>`ブロック全体を削除する。

補足: 元の確定ボタンは`color: #fff`(常に白文字)のハードコードだったが、Buttonの`default`/`destructive`バリアントが持つテーマ追従の文字色(`primary-foreground`/`destructive-foreground`)にそのまま任せる。キャンセルボタンの背景も元の`var(--surface-2)`から、Buttonの`secondary`バリアント(`--color-secondary` = `var(--surface-3)`)に変わるが、これはshadcn標準の配色に合わせた意図的な差分であり、修正しない。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 5: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、`ConfirmDialog`が表示される操作(例: 設定画面のリセット系ボタン、アカウント削除など、`danger`が使われる操作を含む)を行い、以下を確認する:
- ダイアログの表示位置、半透明の背景オーバーレイ
- キャンセルボタンの見た目・クリック動作
- 確定ボタンの見た目(通常時とdanger時で色が異なること)・クリック動作
- Escapeキー・背景クリックでのキャンセル動作

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/ConfirmDialog.svelte
git commit -m "style: ConfirmDialog.svelteをTailwindクラス+Buttonプリミティブに移行"
```
