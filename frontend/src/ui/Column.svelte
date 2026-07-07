<script lang="ts">
  import type { ColumnView } from "../lib/store.svelte";
  import { app } from "../lib/store.svelte";
  import NoteCard from "./NoteCard.svelte";
  import NotificationCard from "./NotificationCard.svelte";

  let { column }: { column: ColumnView } = $props();
  const isNotif = $derived(column.kind.type === "notifications");

  const stateLabel: Record<string, string> = {
    connecting: "接続中…",
    connected: "接続済み",
    reconnecting: "再接続中…",
    error: "エラー",
  };

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      app.loadMore(column.id);
    }
  }

  // 幅リサイズ（右端ハンドルをドラッグ）
  let resizing = false;
  let startX = 0;
  let startW = 0;
  function onResizeDown(e: PointerEvent) {
    resizing = true;
    startX = e.clientX;
    startW = column.width;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onResizeMove(e: PointerEvent) {
    if (!resizing) return;
    const w = Math.min(720, Math.max(220, startW + (e.clientX - startX)));
    app.setColumnWidthLocal(column.id, w);
  }
  function onResizeUp() {
    if (!resizing) return;
    resizing = false;
    app.persistColumnWidth(column.id, column.width);
  }
</script>

<section
  class="column"
  style={`width:${column.width}px`}
  class:dragging={app.draggingColumnId === column.id}
  ondragover={(e) => {
    e.preventDefault();
    app.dragOverColumn(column.id);
  }}
  role="group"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="col-head"
    draggable="true"
    ondragstart={(e) => {
      e.dataTransfer?.setData("text/plain", column.id);
      app.startDragColumn(column.id);
    }}
    ondragend={() => app.endDragColumn()}
    title="ドラッグで並べ替え"
  >
    <span class="dot" data-state={column.state}></span>
    <span class="title">{column.title}</span>
    <span class="col-state">{stateLabel[column.state] ?? column.state}</span>
    <button class="close" title="カラムを閉じる" onclick={() => app.closeColumn(column.id)}>✕</button>
  </header>

  <div class="notes" onscroll={onScroll}>
    {#if isNotif}
      {#each column.notifications as n (n.id)}
        <NotificationCard notification={n} />
      {/each}
      {#if column.notifications.length === 0 && !column.loadingMore}
        <div class="empty">まだ通知がありません</div>
      {/if}
    {:else}
      {#each column.notes as note (note.id)}
        <NoteCard {note} accountId={column.accountId} />
      {/each}
      {#if column.notes.length === 0 && !column.loadingMore}
        <div class="empty">まだノートがありません</div>
      {/if}
    {/if}
    {#if column.loadingMore}
      <div class="loading">読み込み中…</div>
    {/if}
  </div>

  <div
    class="resize"
    onpointerdown={onResizeDown}
    onpointermove={onResizeMove}
    onpointerup={onResizeUp}
    role="separator"
    aria-label="幅を変更"
  ></div>
</section>

<style>
  .column {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: none;
    height: 100%;
    border-right: 1px solid var(--border);
    background: var(--surface-1);
  }
  .column.dragging {
    opacity: 0.55;
  }
  .col-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    background: var(--surface-2);
    cursor: grab;
    user-select: none;
  }
  .col-head:active {
    cursor: grabbing;
  }
  .title {
    font-weight: 600;
    font-size: 0.82rem;
  }
  .col-state {
    margin-left: auto;
    font-size: 0.68rem;
    color: var(--text-dim);
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.85rem;
    padding: 0 2px;
  }
  .close:hover {
    color: var(--text);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-dim);
    flex: none;
  }
  .dot[data-state="connected"] {
    background: #22c55e;
  }
  .dot[data-state="connecting"],
  .dot[data-state="reconnecting"] {
    background: #eab308;
  }
  .dot[data-state="error"] {
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
