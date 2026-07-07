<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import {
    ACTIONS,
    effectiveChord,
    prettyChord,
    eventToChord,
    isModifierOnly,
    type KeyAction,
  } from "../../lib/keymap";

  let capturing = $state<KeyAction | null>(null);
  let err = $state<string | null>(null);
  let busy = $state(false);

  const overrides = $derived(app.ui.keymap ?? {});
  const isCustom = (action: KeyAction) => overrides[action] !== undefined;

  // 追加で固定の操作（キーマップ外・変更不可）
  const fixed: { combo: string; desc: string }[] = [
    { combo: "Ctrl / ⌘ + Enter", desc: "投稿する（投稿バー・投稿フォーム）" },
    { combo: "Esc", desc: "モーダル／リアクションピッカーを閉じる" },
  ];

  function startCapture(action: KeyAction) {
    err = null;
    capturing = action;
  }
  function cancel() {
    capturing = null;
  }

  // キャプチャ中は capture フェーズでキーを横取りして chord を確定する
  $effect(() => {
    if (!capturing) return;
    const action = capturing;
    const handler = (e: KeyboardEvent) => {
      if (isModifierOnly(e)) return; // 修飾キー単体は待つ
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        cancel();
        return;
      }
      void assign(action, eventToChord(e));
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  });

  async function assign(action: KeyAction, chord: string) {
    // Esc は予約（モーダル/ピッカーを閉じる）
    if (chord === "escape") {
      err = "Esc は予約済みのため割り当てできません";
      return;
    }
    // 重複チェック（他アクションの実効キーと衝突しないか）
    const conflict = ACTIONS.find((a) => a.action !== action && effectiveChord(a.action, overrides) === chord);
    if (conflict) {
      err = `そのキーは「${conflict.label}」に割り当て済みです`;
      return;
    }
    const next = { ...overrides };
    const def = ACTIONS.find((a) => a.action === action)!.default;
    if (chord === def) delete next[action];
    else next[action] = chord;
    await save(next);
    capturing = null;
  }

  async function resetOne(action: KeyAction) {
    const next = { ...overrides };
    delete next[action];
    await save(next);
  }
  async function resetAll() {
    await save({});
  }

  async function save(next: Record<string, string>) {
    err = null;
    busy = true;
    try {
      await app.setKeymap(next);
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="head">
  <h3 class="title">キー操作</h3>
  <button class="reset-all" disabled={busy || Object.keys(overrides).length === 0} onclick={resetAll}>
    すべて既定に戻す
  </button>
</div>
<p class="hint">
  「変更」を押して割り当てたいキーを押してください（Esc でキャンセル）。タイムライン上でフォーカス中カラムの選択ノートを操作します。
</p>

<table>
  <tbody>
    {#each ACTIONS as a (a.action)}
      <tr>
        <td class="kbd">
          {#if capturing === a.action}
            <span class="capturing">キー入力待ち…</span>
          {:else}
            <kbd>{prettyChord(effectiveChord(a.action, overrides))}</kbd>
            {#if isCustom(a.action)}<span class="tag">変更済</span>{/if}
          {/if}
        </td>
        <td class="desc">{a.label}</td>
        <td class="ops">
          {#if capturing === a.action}
            <button class="ghost" onclick={cancel}>キャンセル</button>
          {:else}
            <button class="ghost" disabled={busy} onclick={() => startCapture(a.action)}>変更</button>
            {#if isCustom(a.action)}
              <button class="ghost" disabled={busy} onclick={() => resetOne(a.action)}>既定</button>
            {/if}
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

{#if err}<p class="err">{err}</p>{/if}

<div class="fixed">
  <div class="fixed-label">固定（変更不可）</div>
  <table>
    <tbody>
      {#each fixed as f (f.combo)}
        <tr>
          <td class="kbd"><kbd>{f.combo}</kbd></td>
          <td class="desc">{f.desc}</td>
          <td class="ops"></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }
  .reset-all {
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 0.76rem;
    cursor: pointer;
  }
  .reset-all:disabled {
    opacity: 0.4;
    cursor: default;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.84rem;
    margin: 6px 0;
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .kbd {
    width: 34%;
    white-space: nowrap;
  }
  .ops {
    width: 22%;
    text-align: right;
    white-space: nowrap;
  }
  kbd {
    display: inline-block;
    padding: 2px 7px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface-2);
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
  }
  .tag {
    margin-left: 6px;
    font-size: 0.68rem;
    color: var(--accent);
  }
  .capturing {
    color: var(--accent);
    font-size: 0.8rem;
  }
  .desc {
    color: var(--text);
  }
  .ghost {
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 6px;
    padding: 3px 9px;
    font-size: 0.76rem;
    cursor: pointer;
    margin-left: 4px;
  }
  .ghost:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 6px 0 10px;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 6px 0;
  }
  .fixed {
    margin-top: 14px;
  }
  .fixed-label {
    font-size: 0.74rem;
    color: var(--text-dim);
    margin-bottom: 2px;
  }
</style>
