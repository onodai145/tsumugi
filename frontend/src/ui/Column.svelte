<script lang="ts">
  import type { GroupView, TabView } from "../lib/store.svelte";
  import { app, tabName } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";
  import { X, GripVertical, MoreHorizontal, Plus, SquareSplitVertical, Settings } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";

  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    stretch = false,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    stretch?: boolean;
  } = $props();

  const activeTab = $derived(
    group.tabs.find((t) => t.id === group.activeTabId) ?? group.tabs[0],
  );
  const isNotif = $derived(activeTab?.kind.type === "notifications");

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300 && activeTab) {
      app.loadMore(activeTab.id);
    }
  }

  // 幅リサイズ
  let resizing = false;
  let startX = 0;
  let startW = 0;
  function onResizeDown(e: PointerEvent) {
    resizing = true;
    startX = e.clientX;
    startW = group.width;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onResizeMove(e: PointerEvent) {
    if (!resizing) return;
    const w = Math.min(720, Math.max(220, startW + (e.clientX - startX)));
    app.setGroupWidthLocal(group.id, w);
  }
  function onResizeUp() {
    if (!resizing) return;
    resizing = false;
    app.persistGroupWidth(group.id, group.width);
  }

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
    const MENU_WIDTH = 160;
    const MENU_MARGIN = 8;
    if (r)
      menuPos = {
        left: Math.max(0, Math.min(r.left, window.innerWidth - MENU_WIDTH - MENU_MARGIN)),
        top: r.bottom + 4,
      };
    menuOpen = true;
  }

  function pickMenuItem(action: () => void) {
    menuOpen = false;
    action();
  }
</script>

<section
  class="column-root relative flex flex-none flex-col h-full border-r border-border col-bg"
  style={stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
  class:opacity-55={app.draggingGroupId === group.id}
  class:focused={app.focusedGroupId === group.id}
  ondragover={(e) => {
    e.preventDefault();
    app.dragOverGroup(group.id);
  }}
  role="group"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tabbar-bg flex min-h-[26px] items-stretch gap-px overflow-x-auto border-b border-border border-t-2"
    ondragover={(e) => {
      if (app.draggingTabId) {
        e.preventDefault();
        app.dragOverTabBarEnd(group.id);
      }
    }}
  >
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

    {#each group.tabs as t (t.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class={[
          "flex cursor-grab items-center active:cursor-grabbing",
          {
            "shadow-[inset_0_-2px_0_var(--color-primary)]": t.id === group.activeTabId,
          },
          app.draggingTabId === t.id ? "opacity-40" : t.id !== group.activeTabId ? "opacity-65" : "",
        ]}
        draggable="true"
        ondragstart={(e) => {
          e.dataTransfer?.setData("text/plain", t.id);
          e.stopPropagation();
          app.startDragTab(t.id);
        }}
        ondragend={() => app.endDragTab()}
        ondragover={(e) => {
          if (app.draggingTabId) {
            e.preventDefault();
            e.stopPropagation();
            app.dragOverTab(group.id, t.id);
          }
        }}
      >
        <button
          class="flex items-center gap-1 whitespace-nowrap border-none bg-transparent px-1.5 py-0.5 text-xs text-foreground"
          onclick={() => app.setActiveTab(group.id, t.id)}
          ondblclick={() => onEditTab(t)}
          title={`${tabName(t)}（ダブルクリックで編集）`}
        >
          <span
            class="h-1.5 w-1.5 flex-none rounded-full bg-muted-foreground data-[state=connected]:bg-[var(--success)] data-[state=connecting]:bg-[var(--warning)] data-[state=reconnecting]:bg-[var(--warning)] data-[state=error]:bg-destructive"
            data-state={t.state}
          ></span>{tabName(t)}
        </button>
        <button
          class={[
            t.id === group.activeTabId ? "inline-flex" : "hidden",
            "border-none bg-transparent py-0 pr-1 text-muted-foreground",
          ]}
          title="タブを閉じる"
          onclick={() => app.closeTab(t.id)}
        ><X size={12} /></button>
      </div>
    {/each}

    <Button
      variant="ghost"
      size="icon-xs"
      class="text-muted-foreground"
      title="メニュー"
      onclick={toggleMenu}
      bind:ref={menuTrigger}
    ><MoreHorizontal size={16} /></Button>
  </div>

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

  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {#if isNotif}
        {#each activeTab.notifications as n (n.id)}
          <NotificationCard notification={n} accountId={activeTab.accountId} />
        {/each}
        {#if activeTab.notifications.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-sm text-muted-foreground">まだ通知がありません</div>
        {/if}
      {:else}
        {#each activeTab.notes as note (note.id)}
          <NoteCard
            {note}
            accountId={activeTab.accountId}
            tabId={activeTab.id}
            selected={note.id === activeTab.selectedNoteId}
          />
          {#if activeTab.gapMarker && note.id === activeTab.gapMarker.boundaryId}
            <div class="flex items-center gap-2 border-y border-border bg-muted/40 px-3.5 py-2 text-sm text-muted-foreground">
              <span class="flex-1">この間の投稿は省略されています</span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={activeTab.fillingGap}
                onclick={() => app.fillRemainingGap(activeTab.id)}
              >
                {activeTab.fillingGap ? "取得中…" : "省略された投稿を表示"}
              </Button>
            </div>
          {/if}
        {/each}
        {#if activeTab.notes.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-sm text-muted-foreground">まだノートがありません</div>
        {/if}
      {/if}
      {#if activeTab.loadingMore}<div class="p-3.5 text-center text-sm text-muted-foreground">読み込み中…</div>{/if}
    </div>
  {/if}

  {#if !stretch && !group.auto}
    <div
      class="absolute right-[-3px] top-0 h-full w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
      style="z-index:5"
      onpointerdown={onResizeDown}
      onpointermove={onResizeMove}
      onpointerup={onResizeUp}
      role="separator"
      aria-label="幅を変更"
    ></div>
  {/if}
</section>

<style>
  /* 背景画像設定時にカラムを透けさせるための不透明度(--column-opacity)。Tailwindに
     color-mix()のユーティリティが無いことに加えて、Svelteのコンポーネントスコープ
     CSSはunlayeredで注入される(Tailwindのutilitiesレイヤーより優先度が高い)ため、
     この2つの背景は<style>に残す必要がある。逆に言うと、.col-bg/.tabbar-bgが付いた
     要素に後からbg-*系のTailwindクラスを足しても無効化される点に注意。 */
  .col-bg {
    background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);
  }
  .tabbar-bg {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    border-top-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  /* キーボードフォーカス中のカラムは上端をはっきり表示。.column-root/.focusedは同一
     コンポーネントテンプレート内の要素なのでSvelteのスコープ付きCSSがそのまま効く
     (:global()不要)。*/
  .column-root.focused .tabbar-bg {
    border-top-color: var(--accent);
  }
</style>
