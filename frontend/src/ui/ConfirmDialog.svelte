<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  // 汎用の確認モーダル。深くネストされたコンポーネント(NoteCard等)から呼ばれても
  // content-visibility/containの包含ブロックを脱出できるよう portal で body 直下に置く。
  let {
    title = "確認",
    message,
    confirmLabel = "OK",
    cancelLabel = "キャンセル",
    danger = false,
    onConfirm,
    onCancel,
  }: {
    title?: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[8vh]"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(360px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-2.5 font-semibold">{title}</header>
    <p class="mb-4 mt-0 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
    <div class="flex justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={onCancel}>{cancelLabel}</Button>
      <Button variant={danger ? "destructive" : "default"} size="sm" onclick={onConfirm}>{confirmLabel}</Button>
    </div>
  </div>
</div>
