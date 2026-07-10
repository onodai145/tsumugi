<script lang="ts">
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
  class="overlay"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">{title}</header>
    <p class="msg">{message}</p>
    <div class="actions">
      <button class="cancel" onclick={onCancel}>{cancelLabel}</button>
      <button class="confirm" class:danger onclick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 1000;
  }
  .modal {
    width: min(360px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    font-weight: 600;
    margin-bottom: 10px;
  }
  .msg {
    font-size: 0.85rem;
    color: var(--text);
    margin: 0 0 16px;
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .cancel,
  .confirm {
    padding: 7px 16px;
    border: none;
    border-radius: 8px;
    font-family: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .cancel {
    background: var(--surface-2);
    color: var(--text);
  }
  .confirm {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .confirm.danger {
    background: #ef4444;
  }
</style>
