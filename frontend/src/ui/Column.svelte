<script lang="ts">
  import type { GroupView, TabView } from "../lib/store.svelte";
  import { app, tabName } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";
  import { X, GripVertical } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

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
</script>

<section
  class="group relative flex flex-none flex-col h-full border-r border-border col-bg"
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
      ondblclick={() => onEditGroup(group.id)}
      title="ドラッグでカラムを並べ替え（ダブルクリックでカラム設定）"
    ><GripVertical size={14} /></span>

    {#each group.tabs as t (t.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class={[
          "flex cursor-grab items-center active:cursor-grabbing",
          {
            "shadow-[inset_0_-2px_0_var(--color-primary)]": t.id === group.activeTabId,
            "opacity-65": t.id !== group.activeTabId,
            "opacity-40": app.draggingTabId === t.id,
          },
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
          class="flex items-center gap-1 whitespace-nowrap border-none bg-transparent px-1.5 py-0.5 text-[0.76rem] text-foreground"
          onclick={() => app.setActiveTab(group.id, t.id)}
          ondblclick={() => onEditTab(t)}
          title={`${tabName(t)}（ダブルクリックで編集）`}
        >
          <span
            class={[
              "h-1.5 w-1.5 flex-none rounded-full bg-muted-foreground",
              {
                "bg-[var(--success)]": t.state === "connected",
                "bg-[var(--warning)]": t.state === "connecting" || t.state === "reconnecting",
                "bg-destructive": t.state === "error",
              },
            ]}
            data-state={t.state}
          ></span>{tabName(t)}
        </button>
        <button
          class="inline-flex border-none bg-transparent py-0 pr-0 pl-1 text-muted-foreground"
          class:hidden={t.id !== group.activeTabId}
          title="タブを閉じる"
          onclick={() => app.closeTab(t.id)}
        ><X size={12} /></button>
      </div>
    {/each}

    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="タブを追加" onclick={() => onAddTab(group.id)}>＋</Button>
    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="下に分割" onclick={() => onSplitDown(group.id)}>⬓</Button>
  </div>

  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {#if isNotif}
        {#each activeTab.notifications as n (n.id)}
          <NotificationCard notification={n} accountId={activeTab.accountId} />
        {/each}
        {#if activeTab.notifications.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">まだ通知がありません</div>
        {/if}
      {:else}
        {#each activeTab.notes as note (note.id)}
          <NoteCard
            {note}
            accountId={activeTab.accountId}
            tabId={activeTab.id}
            selected={note.id === activeTab.selectedNoteId}
          />
        {/each}
        {#if activeTab.notes.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">まだノートがありません</div>
        {/if}
      {/if}
      {#if activeTab.loadingMore}<div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">読み込み中…</div>{/if}
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
     color-mix()のユーティリティが無いため、この2つの背景だけはCSSに残す。 */
  .col-bg {
    background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);
  }
  .tabbar-bg {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    border-top-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  /* キーボードフォーカス中のカラムは上端をはっきり表示。.group/.focusedは同一コンポーネント
     テンプレート内の要素なのでSvelteのスコープ付きCSSがそのまま効く(:global()不要)。 */
  .group.focused .tabbar-bg {
    border-top-color: var(--accent);
  }
</style>
