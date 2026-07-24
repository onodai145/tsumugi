<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";

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

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>カラム設定</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>

    {#if group}
      {#if rowSlot?.isLeaf}
        <div class="field">
          <span>幅</span>
          <label class="check-row">
            <input type="radio" name="width-mode" checked={!group.auto} onchange={() => setAuto(false)} /> 固定（ドラッグで調整）
          </label>
          <label class="check-row">
            <input type="radio" name="width-mode" checked={group.auto} onchange={() => setAuto(true)} /> 自動調整（ウィンドウ幅に合わせて均等割付）
          </label>
        </div>

        {#if !group.auto}
          <label class="field num-field">
            <span>幅（px、220〜720）</span>
            <input
              type="number"
              min="220"
              max="720"
              value={group.width}
              onchange={(e) => setWidth(Number((e.currentTarget as HTMLInputElement).value))}
            />
          </label>
        {/if}
      {:else if rowSlot}
        <div class="field">
          <span>分割ブロック全体の幅</span>
          <label class="check-row">
            <input type="radio" name="block-width-mode" checked={!rowSlot.auto} onchange={() => setBlockAuto(false)} /> 固定
          </label>
          <label class="check-row">
            <input type="radio" name="block-width-mode" checked={rowSlot.auto} onchange={() => setBlockAuto(true)} /> 自動調整（ウィンドウ幅に合わせて均等割付）
          </label>
        </div>

        {#if !rowSlot.auto}
          <label class="field num-field">
            <span>幅（px、220〜720）</span>
            <input
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
        <div class="field">
          <span>高さ</span>
          <label class="check-row">
            <input type="radio" name="height-mode" checked={!paneCtx.auto} onchange={() => setHeightAuto(false)} /> 固定
          </label>
          <label class="check-row">
            <input type="radio" name="height-mode" checked={paneCtx.auto} onchange={() => setHeightAuto(true)} /> 自動調整（残りを均等割り）
          </label>
        </div>

        {#if !paneCtx.auto}
          <label class="field num-field">
            <span>高さ（%、5〜95）</span>
            <input
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

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 50;
  }
  .modal {
    width: min(360px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 12px;
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  .field > span:first-child {
    color: var(--text-dim);
  }
  .check-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.85rem;
  }
  .num-field {
    margin-top: 10px;
  }
  .num-field input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    width: 100px;
  }
</style>
