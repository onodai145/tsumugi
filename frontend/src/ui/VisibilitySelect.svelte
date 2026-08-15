<script lang="ts">
  import type { VisibilityInput } from "../bindings/tauri.gen";
  import type { Component } from "svelte";
  import { Globe, House, Lock, Mail } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(),
    disabled = false,
  }: { value: VisibilityInput; disabled?: boolean } = $props();

  const OPTIONS: { v: VisibilityInput; label: string; icon: Component; desc: string }[] = [
    { v: "public", label: "公開", icon: Globe, desc: "誰でも見られます" },
    { v: "home", label: "ホーム", icon: House, desc: "ホーム TL とプロフィールのみ" },
    { v: "followers", label: "フォロワー", icon: Lock, desc: "フォロワーのみ" },
    { v: "specified", label: "ダイレクト", icon: Mail, desc: "指定した相手のみ" },
  ];

  // disabled中(チャンネル投稿選択中など)はサーバー側で強制される "公開" を表示する。
  // value 自体は上書きしない(disabled解除で元の選択に戻すため)。
  const current = $derived(OPTIONS.find((o) => o.v === (disabled ? "public" : value)) ?? OPTIONS[0]);

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; top: number } | null>(null);

  const MENU_H = 200;

  function toggle() {
    if (disabled) return;
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

<Button
  type="button"
  variant="outline"
  size="sm"
  onclick={toggle}
  title={`公開範囲: ${current.label}`}
  bind:ref={trigger}
  disabled={disabled}
><span class="inline-flex flex-none"><current.icon size={14} /></span><span class="whitespace-nowrap">{current.label}</span><span class="flex-none text-[0.7rem] text-muted-foreground">▾</span></Button>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed min-w-[200px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
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
          <span class="inline-flex flex-none text-muted-foreground"><o.icon size={16} /></span>
          <span class="flex min-w-0 flex-col">
            <span class="text-[0.85rem] font-semibold">{o.label}</span>
            <span class="text-[0.72rem] text-muted-foreground">{o.desc}</span>
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
