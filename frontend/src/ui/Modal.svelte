<script lang="ts">
  import type { Snippet } from "svelte";
  import { ArrowLeft, X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { app } from "../lib/store.svelte";
  import { pushModalHistory } from "../lib/modalHistory";

  let {
    title,
    onclose,
    children,
    width = "480px",
    maxHeight,
    compact = false,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
    maxHeight?: string;
    // true ならモバイルでも従来の箱型ダイアログのまま表示する(エラー表示など短い通知用)。
    compact?: boolean;
  } = $props();

  // モバイルUIでは全画面ページとして表示する(Issue #450)。
  const fullscreen = $derived(!compact && app.useMobileUi());

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

  // 全画面表示中だけ履歴を1つ積み、戻るジェスチャ/ボタンで閉じられるようにする。
  // アンマウント時・全画面でなくなった時にreleaseが呼ばれて履歴を戻す。
  $effect(() => {
    if (!fullscreen) return;
    return pushModalHistory(() => onclose());
  });
</script>

<div
  class={fullscreen
    ? "fixed inset-0 z-[1000] bg-background"
    : "fixed inset-0 z-[1000] grid items-start justify-items-center overflow-y-auto bg-black/45 pt-[max(8vh,var(--safe-top))] pb-[max(8vh,var(--safe-bottom))]"}
  use:portal
  onclick={fullscreen ? undefined : onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class={fullscreen
      ? "flex h-full flex-col outline-none"
      : `w-[min(var(--modal-w),92vw)] rounded-xl border border-border bg-background p-4 ${maxHeight ? "flex flex-col" : ""}`}
    style={fullscreen ? undefined : `--modal-w:${width};${maxHeight ? ` max-height:${maxHeight};` : ""}`}
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    {#if fullscreen}
      <header
        class="flex h-[calc(3rem+var(--safe-top))] flex-none items-center gap-1 border-b border-border px-2 pt-[var(--safe-top)] font-semibold"
      >
        <Button variant="ghost" size="icon" onclick={onclose} data-testid="modal-close" aria-label="戻る"
          ><ArrowLeft size={20} /></Button
        >
        <span class="min-w-0 truncate">{title}</span>
      </header>
      {#if maxHeight}
        <div class="flex min-h-0 flex-1 flex-col overflow-hidden p-4 pb-[max(1rem,var(--safe-bottom))]">
          <div class="flex min-h-0 flex-1 flex-col">
            {@render children()}
          </div>
        </div>
      {:else}
        <div class="min-h-0 flex-1 overflow-y-auto p-4 pb-[max(1rem,var(--safe-bottom))]">
          {@render children()}
        </div>
      {/if}
    {:else}
      <header class="mb-3 flex flex-none items-center justify-between font-semibold">
        <span>{title}</span>
        <Button variant="ghost" size="icon-xs" onclick={onclose} data-testid="modal-close"><X size={16} /></Button>
      </header>
      {#if maxHeight}
        <div class="flex flex-1 flex-col min-h-0">
          {@render children()}
        </div>
      {:else}
        {@render children()}
      {/if}
    {/if}
  </div>
</div>
