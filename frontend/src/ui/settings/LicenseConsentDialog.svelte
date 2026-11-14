<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { pushModalHistory } from "../../lib/modalHistory";
  import { formatBytes } from "../../lib/localModel";

  // PLaMo Community License への同意ダイアログ(Issue #603)。強制は Rust 側(start_local_model_download が版を検証する)で、
  // ここは同意を取る画面。ConfirmDialog と同じく portal で body 直下に置き、キャンセルに初期フォーカスする。
  let {
    sizeBytes,
    onAccept,
    onCancel,
    onOpenLicense,
  }: { sizeBytes: number; onAccept: () => void; onCancel: () => void; onOpenLicense: () => void } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  // 開いた時にキャンセルへフォーカスする(Enter 一発で同意にならない)。
  let cancelButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    cancelButton?.focus();
  });

  // モバイルUIでは戻るボタンで、このダイアログだけを閉じる(ConfirmDialog と同じ)。
  $effect(() => {
    if (!app.useMobileUi()) return;
    return pushModalHistory(() => onCancel());
  });
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
  style="z-index:1000"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(420px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="license-dialog-title"
    tabindex="-1"
    data-testid="license-dialog"
  >
    <header id="license-dialog-title" class="mb-2.5 font-semibold">PLaMo Community License への同意</header>
    <p class="mb-2 mt-0 text-sm text-foreground">
      翻訳モデル（PLaMo 3 Translate）は、Preferred Networks の PLaMo Community License で提供されています。ダウンロードするには、次の内容への同意が必要です。
    </p>
    <ul class="mb-2 mt-0 list-disc pl-5 text-sm text-foreground">
      <li>個人・非商用の利用は無料です。商用で利用する場合は、事前の登録と、年商 10 億円以下であることが条件です。</li>
      <li>モデルを第三者に提供する場合は、ライセンス全文の写しの提供、"Built with PLaMo" の表示、派生モデル名への "PLaMo" の含有が必要です。</li>
      <li>ライセンスは PFN が一方的に改定できます。版が変わったときは、次のダウンロードの前に再度同意を求めます。</li>
      <li>取得するファイルは約 {formatBytes(sizeBytes)} です。</li>
    </ul>
    <p class="mb-4 mt-0 text-sm">
      <button type="button" class="border-0 bg-transparent p-0 text-left text-sm text-primary hover:underline" onclick={onOpenLicense}>
        ライセンス全文（日本語）を開く
      </button>
    </p>
    <div class="flex justify-end gap-2">
      <Button bind:ref={cancelButton} variant="secondary" size="sm" onclick={onCancel} data-testid="license-cancel">キャンセル</Button>
      <Button size="sm" onclick={onAccept} data-testid="license-accept">同意してダウンロード</Button>
    </div>
  </div>
</div>
