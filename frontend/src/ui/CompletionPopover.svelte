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

<div class="completion-popover" use:portal style={`left:${left}px;top:${top}px`} role="listbox">
  {#each items as item, i (item.key)}
    <button
      type="button"
      class="completion-item"
      class:selected={i === selectedIndex}
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
      {#if item.thumbnail?.type === "custom"}
        <img class="completion-thumb" src={item.thumbnail.url} alt={item.label} />
      {:else if item.thumbnail?.type === "unicode"}
        <span class="completion-thumb completion-thumb-unicode">{item.thumbnail.char}</span>
      {/if}
      <span class="completion-label">{item.label}</span>
    </button>
  {/each}
</div>

<style>
  .completion-popover {
    position: fixed;
    z-index: 60;
    display: flex;
    flex-direction: column;
    max-height: 260px;
    overflow-y: auto;
    min-width: 160px;
    max-width: min(320px, 90vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
  }
  .completion-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 0.82rem;
  }
  .completion-item.selected {
    background: var(--surface-2);
    color: var(--accent);
  }
  .completion-thumb {
    flex: none;
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .completion-thumb-unicode {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 1rem;
  }
  .completion-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
