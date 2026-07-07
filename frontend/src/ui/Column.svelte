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
    // 末尾付近まで来たら過去ページを取得
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      app.loadMore(column.id);
    }
  }
</script>

<section class="column">
  <header class="col-head">
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
</section>

<style>
  .column {
    display: flex;
    flex-direction: column;
    width: 360px;
    flex: none;
    height: 100%;
    border-right: 1px solid var(--border);
    background: var(--surface-1);
  }
  .col-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
  }
  .title {
    font-weight: 600;
    font-size: 0.9rem;
  }
  .col-state {
    margin-left: auto;
    font-size: 0.72rem;
    color: var(--text-dim);
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.9rem;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-dim);
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
    padding: 16px;
    text-align: center;
    color: var(--text-dim);
    font-size: 0.85rem;
  }
</style>
