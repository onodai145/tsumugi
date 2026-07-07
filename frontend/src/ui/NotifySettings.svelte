<script lang="ts">
  import { app } from "../lib/store.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let desktop = $state(app.notify.desktop);
  let sound = $state(app.notify.sound);
  let busy = $state(false);
  let err = $state<string | null>(null);

  const hasNotifColumn = $derived(
    app.groups.some((g) => g.tabs.some((t) => t.kind.type === "notifications")),
  );

  async function save() {
    err = null;
    busy = true;
    try {
      await app.setNotify({ desktop, sound });
      desktop = app.notify.desktop; // 権限拒否で false に戻る場合を反映
      onclose();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>通知設定</span>
      <button class="x" onclick={onclose}>✕</button>
    </header>

    <label class="row"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す</label>
    <label class="row"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす</label>

    <p class="hint">
      通知は<b>通知カラム</b>への新着で発火します。通知カラムが無いと動きません。
      {#if !hasNotifColumn}<br /><span class="warn">※ 現在、通知カラムがありません。「＋カラム」→ Notifications で追加してください。</span>{/if}
    </p>

    <div class="actions">
      <button class="save" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
    </div>
    {#if err}<p class="err">{err}</p>{/if}
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
    width: min(420px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 12px;
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
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.88rem;
    margin-bottom: 8px;
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 8px 0 12px;
  }
  .warn {
    color: #eab308;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
  .save {
    padding: 7px 18px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .save:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
