<script lang="ts">
  import { app } from "../../lib/store.svelte";

  let desktop = $state(app.notify.desktop);
  let sound = $state(app.notify.sound);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const hasNotifColumn = $derived(
    app.groups.some((g) => g.tabs.some((t) => t.kind.type === "notifications")),
  );

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setNotify({ desktop, sound });
      desktop = app.notify.desktop; // 権限拒否で false に戻る場合を反映
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="title">通知</h3>

<label class="row"><input type="checkbox" bind:checked={desktop} /> デスクトップ通知を出す</label>
<label class="row"><input type="checkbox" bind:checked={sound} /> 通知音を鳴らす</label>

<p class="hint">
  通知は<b>通知カラム</b>への新着で発火します。通知カラムが無いと動きません。
  {#if !hasNotifColumn}<br /><span class="warn">※ 現在、通知カラムがありません。「＋カラム」→ Notifications で追加してください。</span>{/if}
</p>

<div class="actions">
  {#if saved}<span class="ok">保存しました</span>{/if}
  <button class="save" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 14px;
    font-size: 1rem;
    font-weight: 600;
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
    margin: 8px 0 16px;
  }
  .warn {
    color: #eab308;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
  }
  .ok {
    font-size: 0.8rem;
    color: #22c55e;
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
