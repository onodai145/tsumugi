<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import ReactionPicker from "../../input/ReactionPicker.svelte";
  import UnicodeEmoji from "../../render/UnicodeEmoji.svelte";
  import { isCustomEmojiKey, customEmojiNameFromKey } from "../../lib/emojiKey";
  import { X, ArrowUp, ArrowDown, Plus } from "@lucide/svelte";

  const accountId = $derived(app.defaultAccountId());
  const pinned = $derived(app.ui.pinnedEmojis ?? []);
  let picking = $state(false);
  let err = $state<string | null>(null);

  $effect(() => {
    if (accountId) app.loadEmojis(accountId).catch(() => {});
  });

  function customEmojiByName(name: string) {
    return (app.emojis[accountId] ?? []).find((e) => e.name === name);
  }

  async function apply(next: string[]) {
    err = null;
    try {
      await app.setPinnedEmojis(next);
    } catch (e) {
      err = String(e);
    }
  }

  function remove(index: number) {
    void apply(pinned.filter((_, i) => i !== index));
  }

  function move(index: number, dir: -1 | 1) {
    const to = index + dir;
    if (to < 0 || to >= pinned.length) return;
    const next = [...pinned];
    [next[index], next[to]] = [next[to], next[index]];
    void apply(next);
  }

  function add(key: string) {
    picking = false;
    if (pinned.includes(key)) return;
    void apply([...pinned, key]);
  }
</script>

<h3 class="title">リアクション</h3>
<p class="hint">絵文字ピッカーの「ピン留め」タブに表示する絵文字を編集できます（本家Misskeyのピン留め絵文字に相当）。</p>

<div class="list">
  {#each pinned as key, i (key)}
    {@const custom = isCustomEmojiKey(key) ? customEmojiByName(customEmojiNameFromKey(key)) : null}
    <div class="chip">
      <span class="glyph">
        {#if isCustomEmojiKey(key)}
          {#if custom}
            <img src={custom.url} alt={key} />
          {:else}
            {key}
          {/if}
        {:else}
          <UnicodeEmoji char={key} />
        {/if}
      </span>
      <div class="chip-actions">
        <button class="icon-btn" disabled={i === 0} onclick={() => move(i, -1)} title="上へ"><ArrowUp size={12} /></button>
        <button class="icon-btn" disabled={i === pinned.length - 1} onclick={() => move(i, 1)} title="下へ"><ArrowDown size={12} /></button>
        <button class="icon-btn" onclick={() => remove(i)} title="削除"><X size={12} /></button>
      </div>
    </div>
  {/each}
  <button class="add-btn" onclick={() => (picking = !picking)} title="ピン留めを追加">
    <Plus size={16} />
  </button>
</div>
{#if pinned.length === 0}
  <p class="hint">ピン留めがありません。「＋」から追加できます。</p>
{/if}

{#if picking}
  <div class="picker-wrap">
    <ReactionPicker {accountId} showPinned={false} onpick={add} />
  </div>
{/if}
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 6px;
    font-size: 1rem;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 14px;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }
  .glyph {
    font-size: 1.2rem;
    line-height: 1;
    display: flex;
  }
  .glyph img {
    height: 1.3em;
    width: 1.3em;
    object-fit: contain;
  }
  .chip-actions {
    display: flex;
    gap: 2px;
  }
  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px;
    border-radius: 4px;
    display: flex;
  }
  .icon-btn:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .icon-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .add-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .picker-wrap {
    margin-top: 12px;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
