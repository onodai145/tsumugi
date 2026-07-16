<script lang="ts">
  import { app } from "../../lib/store.svelte";

  let noteCacheLimit = $state(app.ui.noteCacheLimit ?? 10000);
  let noteCacheMaxAgeDays = $state(app.ui.noteCacheMaxAgeDays ?? 0);
  let noteCacheMaxSizeMb = $state(app.ui.noteCacheMaxSizeMb ?? 0);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const cacheLimit = Math.min(100000, Math.max(0, Math.round(noteCacheLimit) || 0));
      noteCacheLimit = cacheLimit;
      const cacheMaxAge = Math.min(3650, Math.max(0, Math.round(noteCacheMaxAgeDays) || 0));
      noteCacheMaxAgeDays = cacheMaxAge;
      const cacheMaxSize = Math.min(10000, Math.max(0, Math.round(noteCacheMaxSizeMb) || 0));
      noteCacheMaxSizeMb = cacheMaxSize;
      // このセクションが編集しないフィールド(テーマ等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        noteCacheLimit: cacheLimit,
        noteCacheMaxAgeDays: cacheMaxAge,
        noteCacheMaxSizeMb: cacheMaxSize,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<label class="field">
  <span>ノートキャッシュの保持件数上限（件, 0〜100000。0で無制限）</span>
  <input type="number" min="0" max="100000" step="500" bind:value={noteCacheLimit} />
</label>
<label class="field">
  <span>ノートキャッシュの保持日数上限（日, 0〜3650。0で無制限）</span>
  <input type="number" min="0" max="3650" step="1" bind:value={noteCacheMaxAgeDays} />
</label>
<label class="field">
  <span>ノートキャッシュのサイズ上限（MB, 0〜10000。0で無制限）</span>
  <input type="number" min="0" max="10000" step="50" bind:value={noteCacheMaxSizeMb} />
</label>
<p class="hint">
  ローカルDBに保持するノートの上限です。件数・投稿からの経過日数・DBファイルサイズのいずれかを
  超えた分は古い順に自動で削除されます。すべて0にすると無制限に溜め続けます
  （ディスク容量を圧迫する可能性があります）。
</p>

<div class="actions">
  {#if saved}<span class="ok">保存しました</span>{/if}
  <button class="save" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="err">{err}</p>{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .field > span {
    color: var(--text-dim);
  }
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--text-dim);
    margin: 0 0 16px;
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
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
