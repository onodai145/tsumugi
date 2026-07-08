<script lang="ts">
  import type { VisibilityInput } from "../bindings/tauri.gen";

  let { value = $bindable() }: { value: VisibilityInput } = $props();

  const OPTIONS: { v: VisibilityInput; label: string; icon: string; desc: string }[] = [
    { v: "public", label: "公開", icon: "🌐", desc: "誰でも見られます" },
    { v: "home", label: "ホーム", icon: "🏠", desc: "ホーム TL とプロフィールのみ" },
    { v: "followers", label: "フォロワー", icon: "🔒", desc: "フォロワーのみ" },
    { v: "specified", label: "ダイレクト", icon: "✉️", desc: "指定した相手のみ" },
  ];

  const current = $derived(OPTIONS.find((o) => o.v === value) ?? OPTIONS[0]);

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; top: number } | null>(null);

  const MENU_H = 200;

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    const r = trigger?.getBoundingClientRect();
    if (r) {
      const spaceBelow = window.innerHeight - r.bottom;
      const top = spaceBelow >= MENU_H + 8 || spaceBelow > r.top ? r.bottom + 4 : r.top - 4;
      pos = { left: r.left, top };
    }
    open = true;
  }

  function choose(v: VisibilityInput) {
    value = v;
    open = false;
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<button
  class="trigger"
  bind:this={trigger}
  onclick={toggle}
  title={`公開範囲: ${current.label}`}
  type="button"
>
  <span class="ico">{current.icon}</span>
  <span class="label">{current.label}</span>
  <span class="caret">▾</span>
</button>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="menu" style={`left:${pos.left}px;top:${pos.top}px`} onclick={(e) => e.stopPropagation()} role="listbox" tabindex="-1">
      {#each OPTIONS as o (o.v)}
        <button class="item" class:active={o.v === value} onclick={() => choose(o.v)} type="button">
          <span class="ico">{o.icon}</span>
          <span class="meta">
            <span class="name">{o.label}</span>
            <span class="desc">{o.desc}</span>
          </span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font: inherit;
    font-size: 0.82rem;
    flex: none;
  }
  .trigger:hover {
    border-color: var(--accent);
  }
  .ico {
    flex: none;
  }
  .label {
    white-space: nowrap;
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
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
    min-width: 200px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item.active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .item .ico {
    font-size: 1rem;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 0.85rem;
    font-weight: 600;
  }
  .desc {
    font-size: 0.72rem;
    color: var(--text-dim);
  }
</style>
