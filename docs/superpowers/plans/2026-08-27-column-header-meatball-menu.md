# カラムヘッダーのミートボールメニュー化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `frontend/src/ui/Column.svelte` のタブバー末尾にある「＋」（タブ追加）「⬓」（下に分割）の
2ボタンと、グリップのダブルクリックで開くカラム設定を、1つの「⋯」メニューボタンに統合する。

**Architecture:** `AppMenu.svelte`（Issue #96で導入済み）と同じ「トリガーボタン＋
`getBoundingClientRect()`で位置計算＋`portal`でbody直下にメニューを描画＋backdropクリックで閉じる」
パターンを`Column.svelte`にインラインで実装する。項目は3つ（タブを追加／下に分割／カラム設定）で、
既存の`onAddTab`/`onSplitDown`/`onEditGroup` propsをそのまま呼び出す。新規コンポーネント切り出しは行わない。

**Tech Stack:** Svelte 5 (runes: `$state`, `$props`, `$derived`)、`@lucide/svelte`（`MoreHorizontal`アイコン）、
既存の`frontend/src/lib/portal.ts`のportalアクション、`$lib/components/ui/button`の`Button`コンポーネント。

## Global Constraints

- メニュートリガーのアイコンは `MoreHorizontal`（横向き三点、`@lucide/svelte`）。
- メニュー項目の順序は「タブを追加 → 下に分割 → カラム設定」。
- グリップ（⠿）のダブルクリックによるカラム設定オープンは廃止し、`title`からもその文言を除去する。
  グリップはドラッグでの並べ替え専用に戻す。
- 既存の`Column.svelte`のprops（`onAddTab` / `onEditTab` / `onEditGroup` / `onSplitDown` / `stretch`）の
  シグネチャは変更しない。呼び出し元コンポーネントの変更は不要。
- メニューの見た目・開閉挙動は`AppMenu.svelte`（`frontend/src/ui/AppMenu.svelte`）のパターンを踏襲する
  （backdrop + `role="menu"`/`role="menuitem"`のマークアップ、`rounded-lg border border-border
  bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]`のクラス）。ただし開く向きは
  トリガー下端起点（`top`基準・下方向）で、`AppMenu.svelte`の`bottom`基準（上方向）とは異なる。

---

## Task 1: カラムヘッダーメニューの実装

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: 既存props `onAddTab: (groupId: string) => void`、`onSplitDown: (groupId: string) => void`、
  `onEditGroup: (groupId: string) => void`（すべて`Column.svelte`にすでに存在、変更なし）。
  `frontend/src/lib/portal.ts` がexportする `portal` アクション（`export function portal(node: HTMLElement)`、
  `Action`互換で`use:portal`として使う）。
- Produces: このタスクの外から参照される新しいexportや型はない（`Column.svelte`内で完結する見た目の変更）。

このコンポーネントには既存の自動テスト（Vitest）は無く、`AppMenu.svelte`など類似の
メニュー系コンポーネントも同様にテストファイルを持たない。本タスクの検証は
`pnpm check`（型チェック）と手動でのアプリ起動確認で行う。

- [ ] **Step 1: importを追加する**

`frontend/src/ui/Column.svelte` の1〜7行目のimportを以下のように変更する。
`X` / `GripVertical` は既存のまま残し、`MoreHorizontal` を追加。`portal` を新規import。

```svelte
<script lang="ts">
  import type { GroupView, TabView } from "../lib/store.svelte";
  import { app, tabName } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";
  import { X, GripVertical, MoreHorizontal, Plus, SquareSplitVertical, Settings } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";
```

（`Plus`はタブ追加、`SquareSplitVertical`は下に分割、`Settings`はカラム設定の各メニュー項目アイコンに使う。
`SquareSplitVertical`は`@lucide/svelte`に存在するアイコン名 — 存在確認はStep 2のビルド/型チェックで行う。）

- [ ] **Step 2: メニュー開閉用のstateと関数を追加する**

既存の「幅リサイズ」ブロック（`onResizeUp`関数の直後、57行目の`</script>`の手前）に以下を追加する。

```svelte
  // カラムヘッダーメニュー（タブ追加／下に分割／カラム設定を1つの「⋯」に集約）
  let menuOpen = $state(false);
  let menuTrigger = $state<HTMLElement | null>(null);
  let menuPos = $state<{ left: number; top: number } | null>(null);

  function toggleMenu() {
    if (menuOpen) {
      menuOpen = false;
      return;
    }
    const r = menuTrigger?.getBoundingClientRect();
    if (r) menuPos = { left: r.left, top: r.bottom + 4 };
    menuOpen = true;
  }

  function pickMenuItem(action: () => void) {
    menuOpen = false;
    action();
  }
```

- [ ] **Step 3: グリップのダブルクリックとtitleを変更する**

現在の80〜91行目:

```svelte
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      class="flex w-[26px] flex-none cursor-grab select-none items-center justify-center text-muted-foreground active:cursor-grabbing"
      draggable="true"
      ondragstart={(e) => {
        e.dataTransfer?.setData("text/plain", group.id);
        app.startDragGroup(group.id);
      }}
      ondragend={() => app.endDragGroup()}
      ondblclick={() => onEditGroup(group.id)}
      title="ドラッグでカラムを並べ替え（ダブルクリックでカラム設定）"
    ><GripVertical size={16} /></span>
```

を以下に置き換える（`ondblclick`を削除、`title`からダブルクリック文言を除去）:

```svelte
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      class="flex w-[26px] flex-none cursor-grab select-none items-center justify-center text-muted-foreground active:cursor-grabbing"
      draggable="true"
      ondragstart={(e) => {
        e.dataTransfer?.setData("text/plain", group.id);
        app.startDragGroup(group.id);
      }}
      ondragend={() => app.endDragGroup()}
      title="ドラッグでカラムを並べ替え"
    ><GripVertical size={16} /></span>
```

- [ ] **Step 4: 末尾の2ボタンをメニュートリガー1つに置き換える**

現在の140〜141行目:

```svelte
    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="タブを追加" onclick={() => onAddTab(group.id)}>＋</Button>
    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="下に分割" onclick={() => onSplitDown(group.id)}>⬓</Button>
```

を以下に置き換える:

```svelte
    <Button
      variant="ghost"
      size="icon-xs"
      class="text-muted-foreground"
      title="メニュー"
      onclick={toggleMenu}
      bind:ref={menuTrigger}
    ><MoreHorizontal size={16} /></Button>
```

- [ ] **Step 5: メニュー本体をポータルで描画する**

タブバーの`<div>`（72〜142行目、上記の変更後は末尾がトリガーボタンになっている）を閉じる直後、
かつセクション内の`{#if activeTab}`ブロック（144行目）の手前に、以下を追加する。

```svelte
  {#if menuOpen && menuPos}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (menuOpen = false)} role="presentation">
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="fixed w-[160px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
        style={`left:${menuPos.left}px;top:${menuPos.top}px`}
        onclick={(e) => e.stopPropagation()}
        role="menu"
        tabindex="-1"
      >
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onAddTab(group.id))}
        >
          <Plus size={16} /> タブを追加
        </button>
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onSplitDown(group.id))}
        >
          <SquareSplitVertical size={16} /> 下に分割
        </button>
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onEditGroup(group.id))}
        >
          <Settings size={16} /> カラム設定
        </button>
      </div>
    </div>
  {/if}
```

- [ ] **Step 6: 型チェックを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし（`MoreHorizontal`/`Plus`/`SquareSplitVertical`/`Settings`が`@lucide/svelte`に
存在しない場合はここで型エラーになるので、その場合は存在するアイコン名に差し替える。
候補: `Plus`は既存で他画面でも使用実績あり(`AppMenu.svelte`)、`Settings`も同様(`AppMenu.svelte`)、
`SquareSplitVertical`のみ本タスクで新規使用のため要確認）。

- [ ] **Step 7: 手動確認（`cargo tauri dev`）**

リポジトリルートから起動する（`src-tauri`の中からではない — CLAUDE.md参照）。

Run: `cargo tauri dev`

確認項目:
1. カラムのタブバー右端に「⋯」ボタンのみが表示され、以前の「＋」「⬓」ボタンは表示されないこと。
2. 「⋯」クリックでメニューが開き、上から「タブを追加」「下に分割」「カラム設定」の順で
   3項目が表示されること。
3. 「タブを追加」クリックで既存のタブ追加ダイアログが開くこと（メニューは閉じる）。
4. 「下に分割」クリックで既存の分割動作が発火すること（メニューは閉じる）。
5. 「カラム設定」クリックで既存のカラム設定モーダルが開くこと（メニューは閉じる）。
6. メニュー表示中にメニュー外をクリックすると、何も実行されずメニューが閉じること。
7. グリップ（⠿）をダブルクリックしてもカラム設定が開かなくなったこと。
8. グリップをドラッグしたカラムの並べ替えは従来通り動作すること。

すべて確認できたら、起動した`cargo tauri dev`プロセスを終了する。

- [ ] **Step 8: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/Column.svelte
git commit -m "feat: カラムヘッダーのボタンをメニューに集約"
```
