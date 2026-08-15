<script lang="ts">
  // Issue #96: 「＋カラム」「設定」ボタンを投稿欄の隣（旧header）から、画面下部バーの
  // 左端のハンバーガーメニューへ移設。位置計算・portal・外側クリックで閉じる挙動は
  // Dropdown.svelte と同じパターン。画面最下部のバーなので常に上方向に開く。
  import { Menu, Plus, Settings } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let { onAddColumn, onOpenSettings }: { onAddColumn: () => void; onOpenSettings: () => void } = $props();

  let open = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let pos = $state<{ left: number; bottom: number } | null>(null);

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    const r = trigger?.getBoundingClientRect();
    if (r) pos = { left: r.left, bottom: window.innerHeight - r.top + 4 };
    open = true;
  }

  function pick(action: () => void) {
    open = false;
    action();
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<!-- Backstage.svelte(ログバー)と隣接して並ぶため、そのバーの外枠(border-t/bg-card)と
     見た目を揃える。Backstage.svelte自体はログ専用の責務を保つため変更しない。 -->
<div class="flex flex-none items-center border-t border-border bg-card py-[3px] pr-1 pl-[max(8px,env(safe-area-inset-left))]">
  <Button type="button" variant="ghost" size="icon-sm" onclick={toggle} bind:ref={trigger} title="メニュー">
    <Menu size={16} />
  </Button>
</div>

{#if open && pos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed w-[160px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${pos.left}px;bottom:${pos.bottom}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        type="button"
        role="menuitem"
        class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
        onclick={() => pick(onAddColumn)}
      >
        <Plus size={16} /> カラム追加
      </button>
      <button
        type="button"
        role="menuitem"
        class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
        onclick={() => pick(onOpenSettings)}
      >
        <Settings size={16} /> 設定
      </button>
    </div>
  </div>
{/if}
