<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import type { LogLevel } from "../lib/store.svelte";
  import type { Component } from "svelte";
  import { Circle, Check, TriangleAlert, X, ChevronUp, ChevronDown, Database, Activity, Clock } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let { onReauth }: { onReauth: (accountId: string) => void } = $props();

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

<div class="flex flex-col flex-none border-t border-border bg-card">
  {#if open}
    <div class="h-[min(38vh,320px)] overflow-y-auto border-b border-border bg-background font-mono text-xs">
      {#if app.logs.length === 0}
        <div class="p-3.5 text-center text-muted-foreground">ログはまだありません</div>
      {:else}
        {#each app.logs as l (l.id)}
          {@const Ic = icon[l.level]}
          <div class="flex items-baseline gap-2 px-2.5 py-0.5 hover:bg-card" data-level={l.level}>
            <span
              class={[
                "inline-flex flex-none",
                {
                  "text-[var(--success)]": l.level === "success",
                  "text-[var(--warning)]": l.level === "warn",
                  "text-destructive": l.level === "error",
                  "text-muted-foreground": l.level === "info",
                },
              ]}
            ><Ic size={12} /></span>
            <span class="flex-none text-muted-foreground">{hhmmss(l.at)}</span>
            <span class="flex-1 break-words">{l.text}</span>
            {#if l.reauthAccountId}
              <Button variant="outline" size="xs" onclick={() => onReauth(l.reauthAccountId!)}>再認証</Button>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <div
    class="flex min-h-6 items-center gap-2 pt-[3px] pr-[max(8px,env(safe-area-inset-right))] pb-[max(3px,env(safe-area-inset-bottom))] pl-[max(8px,env(safe-area-inset-left))] text-xs"
  >
    <Button variant="outline" size="xs" onclick={() => (open = !open)} title="操作ログ (Backstage)">
      {#if open}<ChevronDown size={12} />{:else}<ChevronUp size={12} />{/if} ログ
      <!-- text-[0.68rem]はスタイルガイド(docs/design/style-guide.md §5)の対象外。極小バッジのため例外的に即値を維持。 -->
      {#if errorCount > 0}<span class="rounded-lg bg-destructive px-[5px] text-[0.68rem] leading-[1.4] text-white">{errorCount}</span>{/if}
    </Button>
    <div class="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden whitespace-nowrap" data-level={latest?.level ?? "info"}>
      {#if latest}
        {@const LatestIc = icon[latest.level]}
        <span
          class={[
            "inline-flex flex-none",
            {
              "text-[var(--success)]": latest.level === "success",
              "text-[var(--warning)]": latest.level === "warn",
              "text-destructive": latest.level === "error",
              "text-muted-foreground": latest.level === "info",
            },
          ]}
        ><LatestIc size={12} /></span>
        <span class="flex-none text-muted-foreground">{hhmmss(latest.at)}</span>
        <span class="overflow-hidden text-ellipsis">{latest.text}</span>
      {:else}
        <span class="overflow-hidden text-ellipsis text-muted-foreground">操作すると、ここに履歴が表示されます</span>
      {/if}
    </div>
    {#if open && app.logs.length > 0}
      <Button variant="ghost" size="xs" class="flex-none text-muted-foreground" onclick={() => app.clearLogs()}>クリア</Button>
    {/if}
    <div
      class="flex flex-none items-center gap-2.5 whitespace-nowrap text-muted-foreground [font-variant-numeric:tabular-nums]"
      title="DB件数 / 流速(件・分) / 起動からの経過時間"
    >
      <span class="inline-flex items-center gap-0.5"><Database size={12} />{app.noteCount.toLocaleString()}件</span>
      <span class="inline-flex items-center gap-0.5"><Activity size={12} />{app.noteRatePerMin}件/分</span>
      <span class="inline-flex items-center gap-0.5"><Clock size={12} />{elapsed}</span>
    </div>
  </div>
</div>
