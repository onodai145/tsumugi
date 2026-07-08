<script lang="ts" generics="T extends string">
  // 汎用の値+ラベル ドロップダウン（テーマ適用・portal で overflow 脱出）。
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

<button class="trigger" bind:this={trigger} onclick={toggle} type="button">
  <span class="label" class:placeholder={!current}>{current?.label ?? placeholder}</span>
  <span class="caret">▾</span>
</button>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="menu"
      style={`left:${pos.left}px;top:${pos.top}px;min-width:${pos.width}px`}
      onclick={(e) => e.stopPropagation()}
      role="listbox"
      tabindex="-1"
    >
      {#each options as o (o.value)}
        <button class="item" class:active={o.value === value} onclick={() => choose(o.value)} type="button">
          {o.label}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
  }
  .trigger:hover {
    border-color: var(--accent);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label.placeholder {
    color: var(--text-dim);
  }
  .caret {
    color: var(--text-dim);
    font-size: 0.7rem;
    flex: none;
  }

  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
  }
  .menu {
    position: fixed;
    max-height: 280px;
    overflow-y: auto;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
  }
  .item {
    display: block;
    width: 100%;
    padding: 7px 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 0.85rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item.active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
</style>
