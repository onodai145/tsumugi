<script lang="ts">
  import { app } from "../lib/store.svelte";
  import type { LogLevel } from "../lib/store.svelte";

  let open = $state(false);

  const latest = $derived(app.logs[0] ?? null);

  const icon: Record<LogLevel, string> = {
    info: "•",
    success: "✓",
    warn: "!",
    error: "✕",
  };

  function hhmmss(ms: number): string {
    const d = new Date(ms);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }

  const errorCount = $derived(app.logs.filter((l) => l.level === "error").length);
</script>

<div class="backstage" class:open>
  {#if open}
    <div class="log-panel">
      {#if app.logs.length === 0}
        <div class="empty">ログはまだありません</div>
      {:else}
        {#each app.logs as l (l.id)}
          <div class="log-row" data-level={l.level}>
            <span class="ic" data-level={l.level}>{icon[l.level]}</span>
            <span class="ts">{hhmmss(l.at)}</span>
            <span class="msg">{l.text}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <div class="bar">
    <button class="toggle" onclick={() => (open = !open)} title="操作ログ (Backstage)">
      {open ? "▼" : "▲"} ログ
      {#if errorCount > 0}<span class="badge">{errorCount}</span>{/if}
    </button>
    <div class="tail" data-level={latest?.level ?? "info"}>
      {#if latest}
        <span class="ic" data-level={latest.level}>{icon[latest.level]}</span>
        <span class="tail-ts">{hhmmss(latest.at)}</span>
        <span class="tail-msg">{latest.text}</span>
      {:else}
        <span class="tail-msg dim">操作すると、ここに履歴が表示されます</span>
      {/if}
    </div>
    {#if open && app.logs.length > 0}
      <button class="clear" onclick={() => app.clearLogs()}>クリア</button>
    {/if}
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
    flex: none;
    width: 1em;
    text-align: center;
    font-weight: 700;
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
