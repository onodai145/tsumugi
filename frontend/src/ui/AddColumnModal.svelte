<script lang="ts">
  import { app } from "../lib/store.svelte";
  import type { ColumnKind, FilterQuery } from "../bindings/tauri.gen";

  let { onclose }: { onclose: () => void } = $props();

  const sources: { kind: ColumnKind; label: string }[] = [
    { kind: { type: "home" }, label: "Home（ホーム）" },
    { kind: { type: "local" }, label: "Local（ローカル）" },
    { kind: { type: "hybrid" }, label: "Hybrid（ソーシャル）" },
    { kind: { type: "global" }, label: "Global（グローバル）" },
  ];

  let accountId = $state(app.accounts[0]?.id ?? "");
  let sourceIdx = $state(0);
  let filterText = $state("");
  let filterErr = $state<string | null>(null);
  let busy = $state(false);
  let submitErr = $state<string | null>(null);

  function buildFilter(): FilterQuery {
    return filterText.trim()
      ? { kind: "tql", value: filterText.trim() }
      : { kind: "keywords", value: [] };
  }

  // TQL の入力を都度検証（空は常にOK）
  async function onFilterInput() {
    submitErr = null;
    if (!filterText.trim()) {
      filterErr = null;
      return;
    }
    filterErr = await app.validateFilter(buildFilter());
  }

  async function submit() {
    submitErr = null;
    if (!accountId) {
      submitErr = "アカウントを選択してください";
      return;
    }
    if (filterErr) return;
    busy = true;
    try {
      await app.addColumn(accountId, sources[sourceIdx].kind, buildFilter());
      onclose();
    } catch (e) {
      submitErr = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>カラムを追加</span>
      <button class="x" onclick={onclose}>✕</button>
    </header>

    <label class="field">
      <span>アカウント</span>
      <select bind:value={accountId}>
        {#each app.accounts as acc (acc.id)}
          <option value={acc.id}>@{acc.username}@{acc.host}</option>
        {/each}
      </select>
    </label>

    <label class="field">
      <span>ソース</span>
      <select bind:value={sourceIdx}>
        {#each sources as s, i}<option value={i}>{s.label}</option>{/each}
      </select>
    </label>

    <label class="field">
      <span>フィルタ（TQL・空欄で全件）</span>
      <input
        placeholder={"例: has_files && !cw && reactions >= 5"}
        bind:value={filterText}
        oninput={onFilterInput}
        class:invalid={!!filterErr}
      />
    </label>
    <p class="hint">
      例: <code>has_files</code> / <code>!bot && local</code> /
      <code>reactions &gt;= 10</code> / <code>text -&gt; "rust"</code>
    </p>
    {#if filterErr}<p class="err">TQLエラー: {filterErr}</p>{/if}

    <div class="actions">
      <button class="submit" disabled={busy || !!filterErr} onclick={submit}>
        {busy ? "作成中…" : "追加"}
      </button>
    </div>
    {#if submitErr}<p class="err">{submitErr}</p>{/if}
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
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .field span {
    color: var(--text-dim);
  }
  select,
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
  input.invalid {
    border-color: #ef4444;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--text-dim);
    margin: 0 0 8px;
  }
  .hint code {
    background: var(--surface-3);
    padding: 0 4px;
    border-radius: 4px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
  }
  .submit {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .submit:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
    word-break: break-word;
  }
</style>
