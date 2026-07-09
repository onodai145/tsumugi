<script lang="ts">
  import type { GroupView, TabView } from "../lib/store.svelte";
  import { app, tabName } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";
  import { X } from "@lucide/svelte";

  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
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
  class="column"
  style={group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
  class:dragging={app.draggingGroupId === group.id}
  class:focused={app.focusedGroupId === group.id}
  ondragover={(e) => {
    e.preventDefault();
    app.dragOverGroup(group.id);
  }}
  role="group"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tabbar"
    ondragover={(e) => {
      if (app.draggingTabId) {
        e.preventDefault();
        app.dragOverTabBarEnd(group.id);
      }
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      class="grip"
      draggable="true"
      ondragstart={(e) => {
        e.dataTransfer?.setData("text/plain", group.id);
        app.startDragGroup(group.id);
      }}
      ondragend={() => app.endDragGroup()}
      ondblclick={() => onEditGroup(group.id)}
      title="ドラッグでカラムを並べ替え（ダブルクリックでカラム設定）"
    >⋮⋮</span>

    {#each group.tabs as t (t.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={t.id === group.activeTabId}
        class:tabdrag={app.draggingTabId === t.id}
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
          class="tab-btn"
          onclick={() => app.setActiveTab(group.id, t.id)}
          ondblclick={() => onEditTab(t)}
          title={`${tabName(t)}（ダブルクリックで編集）`}
        >
          <span class="tab-dot" data-state={t.state}></span>{tabName(t)}
        </button>
        <button class="tab-close" title="タブを閉じる" onclick={() => app.closeTab(t.id)}><X size={12} /></button>
      </div>
    {/each}

    <button class="tab-add" title="タブを追加" onclick={() => onAddTab(group.id)}>＋</button>
  </div>

  {#if activeTab}
    <div class="notes" onscroll={onScroll}>
      {#if isNotif}
        {#each activeTab.notifications as n (n.id)}
          <NotificationCard notification={n} accountId={activeTab.accountId} />
        {/each}
        {#if activeTab.notifications.length === 0 && !activeTab.loadingMore}
          <div class="empty">まだ通知がありません</div>
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
          <div class="empty">まだノートがありません</div>
        {/if}
      {/if}
      {#if activeTab.loadingMore}<div class="loading">読み込み中…</div>{/if}
    </div>
  {/if}

  {#if !group.auto}
    <div
      class="resize"
      onpointerdown={onResizeDown}
      onpointermove={onResizeMove}
      onpointerup={onResizeUp}
      role="separator"
      aria-label="幅を変更"
    ></div>
  {/if}
</section>

<style>
  .column {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: none;
    height: 100%;
    border-right: 1px solid var(--border);
    /* 背景画像設定時にカラムを透けさせるための不透明度。未設定なら 100%(不透明)のまま */
    background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);
  }
  .column.dragging {
    opacity: 0.55;
  }
  .tabbar {
    display: flex;
    align-items: stretch;
    gap: 1px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    border-top: 2px solid color-mix(in srgb, var(--accent) 45%, transparent);
    overflow-x: auto;
    min-height: 26px;
  }
  /* キーボードフォーカス中のカラムは上端をはっきり表示 */
  .column.focused .tabbar {
    border-top-color: var(--accent);
  }
  .grip {
    display: flex;
    align-items: center;
    padding: 0 4px;
    color: var(--text-dim);
    cursor: grab;
    user-select: none;
    font-size: 0.7rem;
    letter-spacing: -2px;
  }
  .grip:active {
    cursor: grabbing;
  }
  .tab {
    display: flex;
    align-items: center;
  }
  .tab {
    cursor: grab;
  }
  .tab:active {
    cursor: grabbing;
  }
  .tab.active {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .tab:not(.active) {
    opacity: 0.65;
  }
  .tab.tabdrag {
    opacity: 0.4;
  }
  .tab-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    font-size: 0.76rem;
    padding: 2px 6px;
    white-space: nowrap;
  }
  .tab-close {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0 4px 0 0;
  }
  .tab:not(.active) .tab-close {
    display: none;
  }
  .tab-add {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0 8px;
    font-size: 0.85rem;
  }
  .tab-add:hover {
    color: var(--accent);
  }
  .tab-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-dim);
    flex: none;
  }
  .tab-dot[data-state="connected"] {
    background: #22c55e;
  }
  .tab-dot[data-state="connecting"],
  .tab-dot[data-state="reconnecting"] {
    background: #eab308;
  }
  .tab-dot[data-state="error"] {
    background: #ef4444;
  }
  .notes {
    overflow-y: auto;
    flex: 1;
  }
  .loading,
  .empty {
    padding: 14px;
    text-align: center;
    color: var(--text-dim);
    font-size: 0.82rem;
  }
  .resize {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 5;
  }
  .resize:hover {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
  }
</style>
