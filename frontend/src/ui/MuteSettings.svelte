<script lang="ts">
  import { app } from "../lib/store.svelte";

  let { onclose }: { onclose: () => void } = $props();

  // 1行1エントリのテキストで編集
  let words = $state(app.mute.ngWords.join("\n"));
  let users = $state(app.mute.ngUsers.join("\n"));
  let instances = $state(app.mute.ngInstances.join("\n"));
  let busy = $state(false);
  let err = $state<string | null>(null);

  const lines = (s: string) =>
    s
      .split("\n")
      .map((x) => x.trim())
      .filter(Boolean);

  async function save() {
    err = null;
    busy = true;
    try {
      await app.setMute({
        ngWords: lines(words),
        ngUsers: lines(users),
        ngInstances: lines(instances),
      });
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
      <span>NG（ミュート）設定</span>
      <button class="x" onclick={onclose}>✕</button>
    </header>
    <p class="hint">1行につき1件。以降に受信するノートに適用され、表示中の該当ノートも消えます。</p>

    <label class="field">
      <span>NGワード（本文/CWに含むと非表示・部分一致）</span>
      <textarea rows="3" placeholder={"ネタバレ\nspoiler"} bind:value={words}></textarea>
    </label>
    <label class="field">
      <span>NGユーザ（@user@host。@は省略可）</span>
      <textarea rows="2" placeholder={"@spammer@example.com"} bind:value={users}></textarea>
    </label>
    <label class="field">
      <span>NGインスタンス（host）</span>
      <textarea rows="2" placeholder={"spam.example"} bind:value={instances}></textarea>
    </label>

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
    width: min(480px, 92vw);
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
    margin-bottom: 8px;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .hint {
    font-size: 0.78rem;
    color: var(--text-dim);
    margin: 0 0 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 0.82rem;
  }
  .field span {
    color: var(--text-dim);
  }
  textarea {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    resize: vertical;
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
