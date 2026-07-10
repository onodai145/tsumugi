<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";

  // showLabel: トリガーにハンドル文字列も出すか（既定はアイコンのみ）。
  // showHost: ハンドルを出す場合に @host まで含めるか。
  // large: トリガーのアイコン・文字サイズを2倍にする（投稿窓横など目立たせたい箇所向け）。
  let {
    value = $bindable(),
    accounts,
    showLabel = false,
    showHost = true,
    disabled = false,
    large = false,
  }: {
    value: string;
    accounts: Account[];
    showLabel?: boolean;
    showHost?: boolean;
    disabled?: boolean;
    large?: boolean;
  } = $props();

  const selected = $derived(accounts.find((a) => a.id === value) ?? accounts[0]);
  const handle = (a: Account) => (showHost ? `@${a.username}@${a.host}` : `@${a.username}`);

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; top: number; width: number } | null>(null);

  const MENU_MAX_H = 280;

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    const r = trigger?.getBoundingClientRect();
    if (r) {
      const spaceBelow = window.innerHeight - r.bottom;
      const top = spaceBelow >= MENU_MAX_H + 8 || spaceBelow > r.top ? r.bottom + 4 : r.top - 4;
      pos = { left: r.left, top, width: r.width };
    }
    open = true;
  }

  function choose(id: string) {
    value = id;
    open = false;
  }

  // ドロップダウンを body 直下へ portal（overflow/contain を脱出）
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<button
  class="trigger"
  class:full={showLabel}
  class:large
  bind:this={trigger}
  onclick={toggle}
  {disabled}
  title={selected ? handle(selected) : "アカウントを選択"}
  type="button"
>
  {#if selected}
    {#if selected.avatarUrl}
      <img class="avatar" src={selected.avatarUrl} alt="" />
    {:else}
      <span class="avatar ph">{(selected.displayName || selected.username).charAt(0)}</span>
    {/if}
    {#if showLabel}<span class="label">{handle(selected)}</span>{/if}
  {/if}
  {#if !large}<span class="caret">▾</span>{/if}
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
      {#each accounts as a (a.id)}
        <button class="item" class:active={a.id === value} onclick={() => choose(a.id)} type="button">
          {#if a.avatarUrl}
            <img class="avatar" src={a.avatarUrl} alt="" />
          {:else}
            <span class="avatar ph">{(a.displayName || a.username).charAt(0)}</span>
          {/if}
          <span class="meta">
            <span class="name">{a.displayName || a.username}</span>
            <span class="acct">@{a.username}@{a.host}</span>
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
    gap: 6px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
    max-width: 100%;
  }
  .trigger:hover {
    border-color: var(--accent);
  }
  .trigger:disabled {
    cursor: default;
    opacity: 0.7;
  }
  .trigger:disabled:hover {
    border-color: var(--border);
  }
  /* フォーム用（showLabel）は全幅にして他ドロップダウンと揃える */
  .trigger.full {
    width: 100%;
    justify-content: flex-start;
  }
  .trigger.full .caret {
    margin-left: auto;
  }
  .trigger.large {
    padding: 10px 14px;
    gap: 10px;
    font-size: 1.1rem;
  }
  .trigger.large .avatar {
    width: 44px;
    height: 44px;
    border-radius: 9px;
  }
  .trigger.large .avatar.ph {
    font-size: 1.2rem;
  }
  .avatar {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    object-fit: cover;
    flex: none;
  }
  .avatar.ph {
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--text-dim);
    font-weight: 700;
    font-size: 0.7rem;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
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
    max-height: 280px;
    overflow-y: auto;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
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
  .item .avatar {
    width: 28px;
    height: 28px;
  }
  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-size: 0.85rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct {
    font-size: 0.74rem;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
