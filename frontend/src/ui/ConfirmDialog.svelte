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
    // z-indexレイヤー規約: 基本はz-1000。CompletionPopover等、より前面に
    // 出す必要がある要素はz-1010を使う(CompletionPopover.svelte参照)。
    // ドロップダウンメニュー(z-1010)から呼ばれるConfirmDialogは、メニュー自身の
    // click-outside用backdropの下に隠れてクリックを奪われるため、z-1020を渡すこと
    // (NoteMenu.svelte参照)。この値はTailwindのz-[...]角括弧クラスが静的文字列
    // しか拾えないため、style属性で動的に適用する。
    z = 1000,
    onConfirm,
    onCancel,
  }: {
    title?: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    z?: number;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,env(safe-area-inset-top))]"
  style={`z-index:${z}`}
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
    <p class="mb-4 mt-0 whitespace-pre-wrap text-sm text-foreground">{message}</p>
    <div class="flex justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={onCancel}>{cancelLabel}</Button>
      <Button variant={danger ? "destructive" : "default"} size="sm" onclick={onConfirm}>{confirmLabel}</Button>
    </div>
  </div>
</div>
