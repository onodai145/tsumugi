<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import type { LogLevel } from "../lib/store.svelte";
  import type { Component } from "svelte";
  import { Circle, Check, TriangleAlert, X, ChevronUp, ChevronDown, Database, Activity, Clock } from "@lucide/svelte";

  let open = $state(false);

  const latest = $derived(app.logs[0] ?? null);

  const icon: Record<LogLevel, Component> = {
    info: Circle,
    success: Check,
    warn: TriangleAlert,
    error: X,
  };

  function hhmmss(ms: number): string {
    const d = new Date(ms);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }

  const errorCount = $derived(app.logs.filter((l) => l.level === "error").length);

  // 起動からの経過時間（右下ステータス用）。1秒ごとに再計算するだけのローカル時計。
  let now = $state(Date.now());
  onMount(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const elapsed = $derived.by(() => {
    const sec = Math.max(0, Math.floor((now - app.bootedAt) / 1000));
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    const s = sec % 60;
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(h)}:${p(m)}:${p(s)}`;
  });
</script>

<div class="backstage" class:open>
  {#if open}
    <div class="log-panel">
      {#if app.logs.length === 0}
        <div class="empty">ログはまだありません</div>
      {:else}
        {#each app.logs as l (l.id)}
          {@const Ic = icon[l.level]}
          <div class="log-row" data-level={l.level}>
            <span class="ic" data-level={l.level}><Ic size={12} /></span>
            <span class="ts">{hhmmss(l.at)}</span>
            <span class="msg">{l.text}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <div class="bar">
    <button class="toggle" onclick={() => (open = !open)} title="操作ログ (Backstage)">
      {#if open}<ChevronDown size={13} />{:else}<ChevronUp size={13} />{/if} ログ
      {#if errorCount > 0}<span class="badge">{errorCount}</span>{/if}
    </button>
    <div class="tail" data-level={latest?.level ?? "info"}>
      {#if latest}
        {@const LatestIc = icon[latest.level]}
        <span class="ic" data-level={latest.level}><LatestIc size={12} /></span>
        <span class="tail-ts">{hhmmss(latest.at)}</span>
        <span class="tail-msg">{latest.text}</span>
      {:else}
        <span class="tail-msg dim">操作すると、ここに履歴が表示されます</span>
      {/if}
    </div>
    {#if open && app.logs.length > 0}
      <button class="clear" onclick={() => app.clearLogs()}>クリア</button>
    {/if}
    <div class="stats" title="DB件数 / 流速(件・分) / 起動からの経過時間">
      <span class="stat"><Database size={12} />{app.noteCount.toLocaleString()}件</span>
      <span class="stat"><Activity size={12} />{app.noteRatePerMin}件/分</span>
      <span class="stat"><Clock size={12} />{elapsed}</span>
    </div>
  </div>
</div>

<style>
  .backstage {
    flex: none;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
    display: flex;
    flex-direction: column;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    min-height: 24px;
    font-size: 0.76rem;
  }
  .toggle {
    flex: none;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 4px;
    padding: 2px 8px;
    cursor: pointer;
    font-size: 0.74rem;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .toggle:hover {
    border-color: var(--accent);
  }
  .badge {
    background: #ef4444;
    color: #fff;
    border-radius: 8px;
    padding: 0 5px;
    font-size: 0.68rem;
    line-height: 1.4;
  }
  .tail {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    white-space: nowrap;
  }
  .tail-ts {
    color: var(--text-dim);
    flex: none;
  }
  .tail-msg {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tail-msg.dim {
    color: var(--text-dim);
  }
  .clear {
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.74rem;
  }
  .clear:hover {
    color: var(--accent);
  }
  .stats {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .stat {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .log-panel {
    height: min(38vh, 320px);
    overflow-y: auto;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
    font-family: ui-monospace, monospace;
    font-size: 0.76rem;
  }
  .log-row {
    display: flex;
    gap: 8px;
    padding: 2px 10px;
    align-items: baseline;
  }
  .log-row:hover {
    background: var(--surface-2);
  }
  .ts {
    color: var(--text-dim);
    flex: none;
  }
  .msg {
    word-break: break-word;
  }
  .ic {
    display: inline-flex;
    flex: none;
  }
  .ic[data-level="success"] {
    color: #22c55e;
  }
  .ic[data-level="warn"] {
    color: #eab308;
  }
  .ic[data-level="error"] {
    color: #ef4444;
  }
  .ic[data-level="info"] {
    color: var(--text-dim);
  }
  .empty {
    padding: 14px;
    text-align: center;
    color: var(--text-dim);
  }
</style>
