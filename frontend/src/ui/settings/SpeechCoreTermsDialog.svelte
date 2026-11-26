<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { pushModalHistory } from "../../lib/modalHistory";
  import { escapeOnLostFocus } from "../../lib/escapeOnLostFocus";
  import { formatBytes } from "../../lib/localModel";
  import type { SpeechCoreTerms } from "../../lib/ipc";

  // VOICEVOX の利用規約(音声モデル・ONNX Runtime)への同意ダイアログ(Issue #625 段階2)。強制は Rust 側
  // (start_speech_core_download が版を検証する)で、ここは同意を取る画面。
  let {
    terms,
    sizeBytes,
    busy = false,
    onAccept,
    onCancel,
  }: {
    terms: SpeechCoreTerms;
    sizeBytes: number;
    busy?: boolean;
    onAccept: () => void;
    onCancel: () => void;
  } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  // 開いた時に取りやめへフォーカスする(Enter 一発で同意にならない)。
  let cancelButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    cancelButton?.focus();
  });

  // モバイルUIでは戻るボタンで、このダイアログだけを閉じる(LicenseConsentDialog と同じ)。
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
    class="w-[min(560px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    use:escapeOnLostFocus={() => onCancel()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="speech-terms-title"
    tabindex="-1"
    data-testid="speech-terms-dialog"
  >
    <header id="speech-terms-title" class="mb-2.5 font-semibold">VOICEVOX の利用規約への同意</header>
    <p class="mb-2 mt-0 text-sm text-foreground">
      内蔵の読み上げエンジンは、VOICEVOX の音声モデルと ONNX Runtime を使います。取得するには、次の規約への同意が必要です。
      声を使った音声を公開する場合は、「VOICEVOX:キャラクター名」のクレジット表記と、各キャラクターの利用規約の遵守が必要です。
    </p>
    <p class="mb-2 mt-0 text-sm text-foreground">取得するファイルは約 {formatBytes(sizeBytes)} です。規約が更新されたときは、次の取得の前に、再度同意を求めます。</p>
    <div class="mb-3 max-h-[40vh] overflow-auto rounded-lg border border-border bg-muted p-2.5">
      <pre class="m-0 whitespace-pre-wrap break-words font-[inherit] text-xs text-foreground" data-testid="speech-terms-text">{terms.voiceModels}

{terms.onnxruntime}</pre>
    </div>
    <div class="flex justify-end gap-2">
      <Button bind:ref={cancelButton} variant="secondary" size="sm" onclick={onCancel} data-testid="speech-terms-cancel">キャンセル</Button>
      <Button size="sm" onclick={onAccept} disabled={busy} data-testid="speech-terms-accept">同意して取得</Button>
    </div>
  </div>
</div>
