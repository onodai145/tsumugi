<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../lib/store.svelte";
  import { pushModalHistory } from "../lib/modalHistory";
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

  // 開いた時にキャンセルボタンへフォーカスする(Issue #570)。呼び出し元のメニュー等が消えるとフォーカスは
  // body に落ち、そのままでは背景の Escape が届かず、App のグローバルショートカットも効いてしまう。
  // 破壊的な確認でも Enter 一発で確定しないよう、確定側ではなくキャンセル側に置く。
  let cancelButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    cancelButton?.focus();
  });

  // モバイルUIでは戻るボタンで、親のモーダルごとではなくこのダイアログだけを閉じる(Issue #573)。
  // Modal と同じく全画面(モバイルUI)のときだけ履歴を積む。PCは戻るボタンが無いので積まない。
  // 積んだ順に上から閉じるため、親の Modal より後に開いたこのダイアログから先に閉じる。
  $effect(() => {
    if (!app.useMobileUi()) return;
    return pushModalHistory(() => onCancel());
  });
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
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
      <Button bind:ref={cancelButton} variant="secondary" size="sm" onclick={onCancel}>{cancelLabel}</Button>
      <Button variant={danger ? "destructive" : "default"} size="sm" onclick={onConfirm}>{confirmLabel}</Button>
    </div>
  </div>
</div>
