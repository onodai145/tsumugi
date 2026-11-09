<script lang="ts">
  // ユーザー通報ダイアログ(Issue #546)。ConfirmDialog と同じ portal + z の作法。送信の成否の扱いは
  // 呼び出し側の onsubmit(成功=resolve で呼び出し側が閉じる、失敗=reject でここにエラーを出して開いたまま)。
  import { Button } from "$lib/components/ui/button";

  let {
    acct,
    onsubmit,
    oncancel,
    // ドロップダウンメニュー(z-1010)から開いても隠れないよう、ConfirmDialog と同じく既定は1020。
    z = 1020,
  }: {
    acct: string;
    onsubmit: (comment: string) => Promise<void>;
    oncancel: () => void;
    z?: number;
  } = $props();

  const MAX_LENGTH = 2048; // 上流 users/report-abuse の comment の maxLength

  let comment = $state("");
  let busy = $state(false);
  let err = $state<string | null>(null);
  const canSend = $derived(!busy && comment.trim().length > 0);

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  async function send() {
    if (!canSend) return;
    busy = true;
    err = null;
    try {
      await onsubmit(comment.trim());
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
  style={`z-index:${z}`}
  use:portal
  onclick={oncancel}
  onkeydown={(e) => e.key === "Escape" && oncancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(420px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-2.5 font-semibold">ユーザーを通報</header>
    <p class="mb-2.5 mt-0 text-sm text-foreground">{acct} を、サーバーの管理者に通報します。</p>
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">通報の内容</span>
      <textarea
        class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
        rows="4"
        maxlength={MAX_LENGTH}
        bind:value={comment}
      ></textarea>
    </label>
    {#if err}<p class="mb-2.5 mt-0 text-sm text-destructive">{err}</p>{/if}
    <div class="flex justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={oncancel}>キャンセル</Button>
      <Button variant="destructive" size="sm" disabled={!canSend} onclick={send}>通報する</Button>
    </div>
  </div>
</div>
