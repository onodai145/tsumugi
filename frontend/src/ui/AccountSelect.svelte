<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

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

<Button
  type="button"
  variant="outline"
  size={large ? "lg" : "sm"}
  class={showLabel ? "w-full justify-start" : ""}
  onclick={toggle}
  {disabled}
  title={selected ? handle(selected) : "アカウントを選択"}
  bind:ref={trigger}
>
  {#if selected}
    {#if selected.avatarUrl}
      <img
        src={selected.avatarUrl}
        alt=""
        class={large
          ? "size-9 flex-none rounded-lg object-cover"
          : "size-[22px] flex-none rounded-[5px] object-cover"}
      />
    {:else}
      <span
        class={large
          ? "grid size-9 flex-none place-items-center rounded-lg bg-accent text-[1rem] font-bold text-muted-foreground"
          : "grid size-[22px] flex-none place-items-center rounded-[5px] bg-accent text-[0.7rem] font-bold text-muted-foreground"}
        >{(selected.displayName || selected.username).charAt(0)}</span
      >
    {/if}
    {#if showLabel}
      <span class="overflow-hidden text-ellipsis whitespace-nowrap">{handle(selected)}</span>
    {/if}
  {/if}
  {#if !large}
    <span
      class={showLabel
        ? "ml-auto flex-none text-[0.7rem] text-muted-foreground"
        : "flex-none text-[0.7rem] text-muted-foreground"}>▾</span
    >
  {/if}
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
      {#each accounts as a (a.id)}
        <button
          type="button"
          class={a.id === value
            ? "active flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"
            : "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"}
          onclick={() => choose(a.id)}
        >
          {#if a.avatarUrl}
            <img src={a.avatarUrl} alt="" class="size-7 flex-none rounded-[5px] object-cover" />
          {:else}
            <span
              class="grid size-7 flex-none place-items-center rounded-[5px] bg-accent text-[0.7rem] font-bold text-muted-foreground"
              >{(a.displayName || a.username).charAt(0)}</span
            >
          {/if}
          <span class="flex min-w-0 flex-col">
            <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.85rem] font-semibold"
              >{a.displayName || a.username}</span
            >
            <span
              class="overflow-hidden text-ellipsis whitespace-nowrap text-[0.74rem] text-muted-foreground"
              >@{a.username}@{a.host}</span
            >
          </span>
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
