<script lang="ts">
  import type { ReactionAcceptanceInput } from "../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(),
  }: { value: ReactionAcceptanceInput } = $props();

  const OPTIONS: { v: ReactionAcceptanceInput; label: string; desc: string }[] = [
    { v: "all", label: "すべて", desc: "誰でもリアクションできます" },
    { v: "likeOnly", label: "いいねのみ", desc: "いいね♡のみ受け付けます" },
    { v: "likeOnlyForRemote", label: "いいねのみ（リモート）", desc: "リモートユーザーはいいねのみ" },
    { v: "nonSensitiveOnly", label: "非センシティブ絵文字のみ", desc: "センシティブな絵文字リアクションを拒否します" },
    {
      v: "nonSensitiveOnlyForLocalLikeOnlyForRemote",
      label: "非センシティブ（ローカル）／いいねのみ（リモート）",
      desc: "ローカルは非センシティブ絵文字のみ、リモートはいいねのみ",
    },
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
            ? "active flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"
            : "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-[inherit] text-foreground hover:bg-muted"}
          onclick={() => choose(o.v)}
        >
          <span class="flex min-w-0 flex-col">
            <span class="text-sm font-semibold">{o.label}</span>
            <span class="text-xs text-muted-foreground">{o.desc}</span>
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
