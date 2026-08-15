<script lang="ts">
  import type { CompletionItem } from "../lib/mfmCompletion";
  import { portal } from "../lib/portal";

  let {
    items,
    selectedIndex,
    left,
    top,
    onpick,
  }: {
    items: CompletionItem[];
    selectedIndex: number;
    left: number;
    top: number;
    onpick: (index: number) => void;
  } = $props();

  let itemEls: (HTMLButtonElement | undefined)[] = $state([]);

  $effect(() => {
    itemEls[selectedIndex]?.scrollIntoView({ block: "nearest" });
  });
</script>

<!-- Modal.svelte/ConfirmDialog.svelte(z-[1000])より前面に出す必要がある。
     AddColumnModal(唯一のTqlCompletionField呼び出し元)が共通Modalを使うようになったため。 -->
<div
  class="fixed z-[1010] flex max-h-[260px] min-w-[160px] max-w-[min(320px,90vw)] flex-col overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
  data-testid="completion-popover"
  use:portal
  style={`left:${left}px;top:${top}px`}
  role="listbox"
>
  {#each items as item, i (item.key)}
    <button
      type="button"
      class={i === selectedIndex
        ? "flex w-full items-center gap-1.5 rounded-md bg-muted px-2 py-[5px] text-left font-[inherit] text-sm text-primary"
        : "flex w-full items-center gap-1.5 rounded-md px-2 py-[5px] text-left font-[inherit] text-sm text-foreground"}
      role="option"
      aria-selected={i === selectedIndex}
      bind:this={itemEls[i]}
      onmousedown={(e) => {
        // click ではなく mousedown を使い、かつ preventDefault することで
        // textarea の blur を発生させずに確定できるようにする(blurが先に走ると
        // ポップアップが閉じてクリックが空振りする)。
        e.preventDefault();
        onpick(i);
      }}
    >
      {#if item.thumbnail?.type === "custom" || item.thumbnail?.type === "avatar"}
        <img class="h-[18px] w-[18px] flex-none object-contain" src={item.thumbnail.url} alt={item.label} />
      {:else if item.thumbnail?.type === "unicode"}
        <span class="inline-flex h-[18px] w-[18px] flex-none items-center justify-center text-base">{item.thumbnail.char}</span>
      {/if}
      <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{item.label}</span>
    </button>
  {/each}
</div>
