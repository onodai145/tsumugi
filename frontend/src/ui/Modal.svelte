<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    title,
    onclose,
    children,
    width = "480px",
  }: { title: string; onclose: () => void; children: Snippet; width?: string } = $props();

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
</script>

<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  use:portal
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(var(--modal-w),92vw)] rounded-[14px] border border-border bg-background p-4"
    style={`--modal-w:${width}`}
    bind:this={modalEl}
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-3 flex items-center justify-between font-semibold">
      <span>{title}</span>
      <Button variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>
    {@render children()}
  </div>
</div>
