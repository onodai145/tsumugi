<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  // カラム(視覚カラム)自体の設定。タブ設定とは別に、グリップのダブルクリックで開く。
  let { groupId, onclose }: { groupId: string; onclose: () => void } = $props();

  const group = $derived(app.groups.find((g) => g.id === groupId));

  function setAuto(auto: boolean) {
    if (groupId) app.setGroupAuto(groupId, auto);
  }

  function setWidth(w: number) {
    if (!groupId || !Number.isFinite(w)) return;
    const clamped = Math.min(720, Math.max(220, Math.round(w)));
    app.setGroupWidthLocal(groupId, clamped);
    app.persistGroupWidth(groupId, clamped);
  }

  const paneCtx = $derived(groupId ? app.paneColumnContext(groupId) : null);

  function setHeightPercent(p: number) {
    if (!paneCtx || !Number.isFinite(p)) return;
    const clamped = Math.min(95, Math.max(5, Math.round(p)));
    app.resizePane(paneCtx.nodeId, clamped);
  }

  function setHeightAuto(auto: boolean) {
    if (!paneCtx) return;
    app.setPaneAuto(paneCtx.nodeId, auto);
  }

  // 縦分割されたブロック全体の幅(Row内でこのグループを含むSplit自身の幅)。
  // 一度も分割していない普通のカラムなら isLeaf=true になり、既存のColumnGroup.width
  // ベースの幅UIをそのまま使う(こちらは触らない)。
  const rowSlot = $derived(groupId ? app.paneRowSlotContext(groupId) : null);

  function setBlockWidth(w: number) {
    if (!rowSlot || rowSlot.isLeaf || !Number.isFinite(w)) return;
    const clamped = Math.min(720, Math.max(220, Math.round(w)));
    app.resizePane(rowSlot.nodeId, clamped);
  }

  function setBlockAuto(auto: boolean) {
    if (!rowSlot || rowSlot.isLeaf) return;
    app.setPaneAuto(rowSlot.nodeId, auto);
  }
</script>

<div
  class="fixed inset-0 z-50 grid items-start justify-items-center bg-black/45 pt-[8vh]"
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(360px,92vw)] rounded-[14px] border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header class="mb-3 flex items-center justify-between font-semibold">
      <span>カラム設定</span>
      <Button type="button" variant="ghost" size="icon-xs" onclick={onclose}><X size={16} /></Button>
    </header>

    {#if group}
      {#if rowSlot?.isLeaf}
        <div class="flex flex-col gap-1 text-[0.85rem]">
          <span class="text-muted-foreground">幅</span>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="width-mode"
              class="accent-primary"
              checked={!group.auto}
              onchange={() => setAuto(false)}
            /> 固定（ドラッグで調整）
          </label>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="width-mode"
              class="accent-primary"
              checked={group.auto}
              onchange={() => setAuto(true)}
            /> 自動調整（ウィンドウ幅に合わせて均等割付）
          </label>
        </div>

        {#if !group.auto}
          <label class="mt-2.5 flex flex-col gap-1 text-[0.85rem]">
            <span class="text-muted-foreground">幅（px、220〜720）</span>
            <input
              class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
              type="number"
              min="220"
              max="720"
              value={group.width}
              onchange={(e) => setWidth(Number((e.currentTarget as HTMLInputElement).value))}
            />
          </label>
        {/if}
      {:else if rowSlot}
        <div class="flex flex-col gap-1 text-[0.85rem]">
          <span class="text-muted-foreground">分割ブロック全体の幅</span>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="block-width-mode"
              class="accent-primary"
              checked={!rowSlot.auto}
              onchange={() => setBlockAuto(false)}
            /> 固定
          </label>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="block-width-mode"
              class="accent-primary"
              checked={rowSlot.auto}
              onchange={() => setBlockAuto(true)}
            /> 自動調整（ウィンドウ幅に合わせて均等割付）
          </label>
        </div>

        {#if !rowSlot.auto}
          <label class="mt-2.5 flex flex-col gap-1 text-[0.85rem]">
            <span class="text-muted-foreground">幅（px、220〜720）</span>
            <input
              class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
              type="number"
              min="220"
              max="720"
              value={Math.round(rowSlot.size)}
              onchange={(e) => setBlockWidth(Number((e.currentTarget as HTMLInputElement).value))}
            />
          </label>
        {/if}
      {/if}

      {#if paneCtx}
        <div class="flex flex-col gap-1 text-[0.85rem]">
          <span class="text-muted-foreground">高さ</span>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="height-mode"
              class="accent-primary"
              checked={!paneCtx.auto}
              onchange={() => setHeightAuto(false)}
            /> 固定
          </label>
          <label class="flex items-center gap-1.5 text-[0.85rem]">
            <input
              type="radio"
              name="height-mode"
              class="accent-primary"
              checked={paneCtx.auto}
              onchange={() => setHeightAuto(true)}
            /> 自動調整（残りを均等割り）
          </label>
        </div>

        {#if !paneCtx.auto}
          <label class="mt-2.5 flex flex-col gap-1 text-[0.85rem]">
            <span class="text-muted-foreground">高さ（%、5〜95）</span>
            <input
              class="w-[100px] rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
              type="number"
              min="5"
              max="95"
              value={Math.round(paneCtx.size)}
              onchange={(e) => setHeightPercent(Number((e.currentTarget as HTMLInputElement).value))}
            />
          </label>
        {/if}
      {/if}
    {/if}
  </div>
</div>
