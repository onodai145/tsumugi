<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { X } from "@lucide/svelte";

  // カラム(視覚カラム)自体の設定。タブ設定とは別に、グリップのダブルクリックで開く。
  let { groupId, onclose }: { groupId: string; onclose: () => void } = $props();

  const group = $derived(app.groups.find((g) => g.id === groupId));

  function setAuto(auto: boolean) {
    if (groupId) app.setGroupAuto(groupId, auto);
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
      <div class="field">
        <span>幅</span>
        <label class="check-row">
          <input type="radio" name="width-mode" checked={!group.auto} onchange={() => setAuto(false)} /> 固定（ドラッグで調整）
        </label>
        <label class="check-row">
          <input type="radio" name="width-mode" checked={group.auto} onchange={() => setAuto(true)} /> 自動調整（ウィンドウ幅に合わせて均等割付）
        </label>
      </div>
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
</style>
