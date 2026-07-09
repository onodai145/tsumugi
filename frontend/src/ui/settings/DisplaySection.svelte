<script lang="ts">
  import { app } from "../../lib/store.svelte";

  let theme = $state(app.ui.theme);
  let width = $state(app.ui.defaultColumnWidth);
  let fontFamily = $state(app.ui.fontFamily ?? "");
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const themes: { id: string; label: string }[] = [
    { id: "auto", label: "OSに合わせる" },
    { id: "light", label: "ライト" },
    { id: "dark", label: "ダーク" },
  ];

  const fontPresets: { label: string; value: string }[] = [
    { label: "既定", value: "" },
    { label: "游ゴシック", value: '"Yu Gothic", "Hiragino Kaku Gothic ProN", sans-serif' },
    { label: "メイリオ", value: "Meiryo, sans-serif" },
    { label: "等幅", value: 'ui-monospace, "Cascadia Code", "SF Mono", monospace' },
    { label: "明朝", value: '"Yu Mincho", "Hiragino Mincho ProN", serif' },
  ];

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const w = Math.min(720, Math.max(220, Math.round(width) || 300));
      width = w;
      await app.setUiPrefs({ theme, defaultColumnWidth: w, keymap: app.ui.keymap, fontFamily });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="title">表示</h3>

<div class="field">
  <span>テーマ</span>
  <div class="seg">
    {#each themes as t (t.id)}
      <button class="seg-btn" class:active={theme === t.id} onclick={() => (theme = t.id)}>{t.label}</button>
    {/each}
  </div>
</div>

<label class="field">
  <span>新規カラムの既定幅（px, 220〜720）</span>
  <input type="number" min="220" max="720" step="10" bind:value={width} />
</label>
<p class="hint">既定幅は次に追加するカラムから適用されます。既存カラムはカラム端のドラッグで個別調整できます。</p>

<div class="field">
  <span>フォント</span>
  <div class="seg">
    {#each fontPresets as p (p.value)}
      <button
        class="seg-btn"
        class:active={fontFamily === p.value}
        onclick={() => (fontFamily = p.value)}
      >
        {p.label}
      </button>
    {/each}
  </div>
  <input
    type="text"
    class="font-input"
    placeholder='CSS の font-family 値（例: "Noto Sans JP", sans-serif）'
    bind:value={fontFamily}
  />
</div>
<p class="hint" style={fontFamily ? `font-family: ${fontFamily}` : undefined}>
  プレビュー: あいうえお ABCDEFG 123
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
    font-size: 0.82rem;
  }
  .field > span {
    color: var(--text-dim);
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    width: fit-content;
  }
  .seg-btn {
    padding: 6px 14px;
    border: none;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.82rem;
    border-right: 1px solid var(--border);
  }
  .seg-btn:last-child {
    border-right: none;
  }
  .seg-btn.active {
    background: var(--accent);
    color: #fff;
  }
  input[type="number"] {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    width: 140px;
  }
  .font-input {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    width: 100%;
    margin-top: 6px;
  }
  .hint {
    font-size: 0.76rem;
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
