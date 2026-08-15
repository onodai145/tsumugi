<script lang="ts">
  import type { ReactionAcceptanceInput } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(),
  }: { value: ReactionAcceptanceInput } = $props();

  // ラベルは Misskey 本家 (ja-JP.yml) の文言に揃える。
  const OPTIONS: { v: ReactionAcceptanceInput; label: string }[] = [
    { v: "all", label: "全て" },
    { v: "likeOnly", label: "いいねのみ" },
    { v: "likeOnlyForRemote", label: "全て (リモートはいいねのみ)" },
    { v: "nonSensitiveOnly", label: "非センシティブのみ" },
    { v: "nonSensitiveOnlyForLocalLikeOnlyForRemote", label: "非センシティブのみ (リモートはいいねのみ)" },
  ];

  const current = $derived(OPTIONS.find((o) => o.v === value) ?? OPTIONS[0]);

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; top: number } | null>(null);

  const MENU_H = 260;

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

  function choose(v: ReactionAcceptanceInput) {
    value = v;
    open = false;
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<Button
  type="button"
  variant="outline"
  size="sm"
  class={value !== "all" ? "border-primary text-primary" : ""}
  onclick={toggle}
  title={`リアクション受け入れ: ${current.label}`}
  bind:ref={trigger}
><span class="whitespace-nowrap">{current.label}</span><span class="flex-none text-[0.7rem] text-muted-foreground">▾</span></Button>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed min-w-[260px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${pos.left}px;top:${pos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="listbox"
      tabindex="-1"
    >
      {#each OPTIONS as o (o.v)}
        <button
          type="button"
          class={o.v === value
            ? "active block w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"
            : "block w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"}
          onclick={() => choose(o.v)}
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
