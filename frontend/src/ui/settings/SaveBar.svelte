<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";

  // 設定セクション共通の保存行(Issue #537)。長いセクションでもスクロールせずに押せるよう、
  // スクロール領域の下端に張り付かせる。エラー文も同じ領域に入れ、バーの下に隠れないようにする。
  let {
    busy,
    disabled = false,
    saved,
    err,
    onsave,
    testid,
  }: {
    busy: boolean;
    disabled?: boolean;
    saved: boolean;
    err: string | null;
    onsave: () => void;
    testid?: string;
  } = $props();

  // sticky の基準はスクロール領域のパディング内側(style-guide.md)。余白ぶんを打ち消して下端いっぱいに寄せる。
  // PC: スクロール領域は Settings の <section>(px-5 py-[18px])。
  // モバイル: スクロール領域は Modal 本体(p-4 + max(1rem, セーフエリア, キーボード+0.5rem) の下余白)で、
  //   <section> の py-4 も加わる。bottom-0 だとバーが下余白の分だけ浮き、その隙間に本文が透けて見える。
  //   バーの下端をキーボード上端(キーボードなしなら画面下端)へ寄せるため、bottom で下余白との差を打ち消す
  //   (式: キーボード - 下余白。負値)。<section> の下パディング(py-4)の中へは、-mb-4 でバーの margin box を
  //   縮めて食い込ませる(sticky は margin box が包含ブロックの内側に収まるよう制約されるため)。
  //   セーフエリアはバー自身の pb で確保する(キーボード表示中はキーボードが覆うので不要)。
  const mobileUi = $derived(app.useMobileUi());
</script>

<div
  class={mobileUi
    ? "sticky bottom-[calc(var(--keyboard-inset)_-_max(1rem,var(--safe-bottom),calc(var(--keyboard-inset)_+_0.5rem)))] z-10 -mx-4 -mb-4 mt-4 border-t border-border bg-background px-4 pt-2 pb-[max(0.5rem,calc(var(--safe-bottom)_-_var(--keyboard-inset)))]"
    : "sticky -bottom-[18px] z-10 -mx-5 -mb-[18px] mt-4 border-t border-border bg-background px-5 py-3"}
  data-testid="settings-save-bar"
>
  <div class="flex items-center justify-end gap-3">
    {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
    <Button type="button" disabled={busy || disabled} onclick={onsave} data-testid={testid}>
      {busy ? "保存中…" : "保存"}
    </Button>
  </div>
  {#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
</div>
