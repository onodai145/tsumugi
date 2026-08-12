<script lang="ts" generics="T extends string">
  // 汎用の値+ラベル ドロップダウン（テーマ適用・portal で overflow 脱出）。
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(),
    options,
    placeholder = "選択…",
  }: {
    value: T;
    options: { value: T; label: string }[];
    placeholder?: string;
  } = $props();

  const current = $derived(options.find((o) => o.value === value));

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; top: number; width: number } | null>(null);

  const MENU_H = 280;

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    const r = trigger?.getBoundingClientRect();
    if (r) {
      const spaceBelow = window.innerHeight - r.bottom;
      const top = spaceBelow >= MENU_H + 8 || spaceBelow > r.top ? r.bottom + 4 : r.top - 4;
      pos = { left: r.left, top, width: r.width };
    }
    open = true;
  }

  function choose(v: T) {
    value = v;
    open = false;
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<Button type="button" variant="outline" size="xs" class="w-full justify-between" onclick={toggle} bind:ref={trigger}>
  <span class={current ? "overflow-hidden text-ellipsis whitespace-nowrap" : "overflow-hidden text-ellipsis whitespace-nowrap text-muted-foreground"}>{current?.label ?? placeholder}</span>
  <span class="flex-none text-[0.7rem] text-muted-foreground">▾</span>
</Button>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed max-h-[280px] overflow-y-auto rounded-[10px] border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${pos.left}px;top:${pos.top}px;min-width:${pos.width}px`}
      onclick={(e) => e.stopPropagation()}
      role="listbox"
      tabindex="-1"
    >
      {#each options as o (o.value)}
        <button
          type="button"
          class={o.value === value
            ? "active block w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.85rem] text-foreground hover:bg-muted"
            : "block w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.85rem] text-foreground hover:bg-muted"}
          onclick={() => choose(o.value)}
        >
          {o.label}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
</style>
