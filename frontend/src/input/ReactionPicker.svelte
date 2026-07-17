<script lang="ts">
  import { app } from "../lib/store.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import type { EmojiDef } from "../bindings/tauri.gen";
  import { UNICODE_EMOJIS, UNICODE_EMOJI_CATEGORIES, DEFAULT_PINNED_EMOJIS } from "../lib/unicodeEmojiList";
  import { Pin, PinOff, Pencil } from "@lucide/svelte";

  let { accountId, onpick }: { accountId: string; onpick: (reaction: string) => void } = $props();

  // "pinned" | "custom" | カテゴリ index
  type Tab = "pinned" | "custom" | number;

  let query = $state("");
  let customEmojis = $state<EmojiDef[]>([]);
  let tab = $state<Tab>("pinned");
  let editMode = $state(false);

  $effect(() => {
    app.loadEmojis(accountId).then((list) => (customEmojis = list)).catch(() => {});
  });

  // 空配列は「ユーザーが全部ピン解除した」正当な状態なので、既定8種へフォールバックしない
  // (フォールバックすると解除操作がすぐ復活して見え、次のクリックが誤って追加になる)。
  const pinned = $derived(app.ui.pinnedEmojis ?? DEFAULT_PINNED_EMOJIS);

  function isCustomKey(key: string): boolean {
    return key.startsWith(":") && key.endsWith(":");
  }

  function customEmojiByName(name: string): EmojiDef | undefined {
    return customEmojis.find((e) => e.name === name);
  }

  // ピン留めキー(Unicode文字 or ":name:")を描画用の {char} | {name,url} に解決する。
  // カスタム絵文字が未取得/削除済みなら表示のみスキップする。
  const pinnedEntries = $derived(
    pinned
      .map((key) => {
        if (isCustomKey(key)) {
          const def = customEmojiByName(key.slice(1, -1));
          return def ? { key, custom: def } : null;
        }
        return { key, custom: null as EmojiDef | null };
      })
      .filter((e): e is { key: string; custom: EmojiDef | null } => e !== null),
  );

  const queryLower = $derived(query.trim().toLowerCase());

  const unicodeMatches = $derived(
    queryLower
      ? UNICODE_EMOJIS.filter((e) => e.name.includes(queryLower)).slice(0, 200)
      : typeof tab === "number"
        ? UNICODE_EMOJIS.filter((e) => e.category === tab)
        : [],
  );

  const customMatches = $derived(
    queryLower
      ? customEmojis
          .filter((e) => e.name.toLowerCase().includes(queryLower) || e.aliases.some((a) => a.toLowerCase().includes(queryLower)))
          .slice(0, 100)
      : tab === "custom"
        ? customEmojis
        : [],
  );

  function handlePick(key: string) {
    if (editMode) {
      void app.togglePinnedEmoji(key);
      return;
    }
    onpick(key);
  }
</script>

<div class="picker">
  <div class="tabs">
    <button class="tab-btn" class:active={tab === "pinned"} onclick={() => (tab = "pinned")}>ピン留め</button>
    {#each UNICODE_EMOJI_CATEGORIES as c (c.index)}
      <button class="tab-btn" class:active={tab === c.index} onclick={() => (tab = c.index)}>{c.label}</button>
    {/each}
    <button class="tab-btn" class:active={tab === "custom"} onclick={() => (tab = "custom")}>カスタム</button>
    <button
      class="edit-btn"
      class:active={editMode}
      title={editMode ? "編集モードを終了" : "ピン留めを編集"}
      onclick={() => (editMode = !editMode)}
    >
      <Pencil size={14} />
    </button>
  </div>
  <input class="search" placeholder="絵文字を検索…" bind:value={query} />
  <div class="grid">
    {#if !queryLower && tab === "pinned"}
      {#each pinnedEntries as e (e.key)}
        <button class="emoji-btn" title={e.key} onclick={() => handlePick(e.key)}>
          {#if e.custom}
            <img src={e.custom.url} alt={e.key} loading="lazy" />
          {:else}
            <UnicodeEmoji char={e.key} />
          {/if}
          {#if editMode}<PinOff class="pin-badge" size={10} />{/if}
        </button>
      {/each}
      {#if pinnedEntries.length === 0}
        <span class="none">ピン留めした絵文字がありません</span>
      {/if}
    {:else}
      {#each unicodeMatches as e (e.char)}
        <button class="emoji-btn" title={`:${e.name}:`} onclick={() => handlePick(e.char)}>
          <UnicodeEmoji char={e.char} />
          {#if editMode}
            {#if pinned.includes(e.char)}<Pin class="pin-badge" size={10} />{/if}
          {/if}
        </button>
      {/each}
      {#each customMatches as e (e.name)}
        <button class="emoji-btn" title={`:${e.name}:`} onclick={() => handlePick(`:${e.name}:`)}>
          <img src={e.url} alt={`:${e.name}:`} loading="lazy" />
          {#if editMode}
            {#if pinned.includes(`:${e.name}:`)}<Pin class="pin-badge" size={10} />{/if}
          {/if}
        </button>
      {/each}
      {#if unicodeMatches.length === 0 && customMatches.length === 0}
        <span class="none">絵文字がありません</span>
      {/if}
    {/if}
  </div>
  {#if editMode}
    <p class="hint">絵文字をクリックでピン留めの追加/削除</p>
  {/if}
</div>

<style>
  .picker {
    width: 300px;
    padding: 8px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .tabs {
    display: flex;
    flex-wrap: nowrap;
    gap: 2px;
    overflow-x: auto;
    margin-bottom: 6px;
    align-items: center;
  }
  .tab-btn {
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 3px 7px;
    border-radius: 6px;
    font-size: 0.72rem;
    white-space: nowrap;
  }
  .tab-btn:hover {
    background: var(--surface-3);
  }
  .tab-btn.active {
    background: var(--surface-3);
    color: var(--text);
  }
  .edit-btn {
    flex: none;
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
    display: flex;
  }
  .edit-btn:hover {
    background: var(--surface-3);
  }
  .edit-btn.active {
    color: var(--accent);
    background: var(--surface-3);
  }
  .search {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    margin-bottom: 6px;
    box-sizing: border-box;
  }
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    max-height: 220px;
    overflow-y: auto;
  }
  .emoji-btn {
    position: relative;
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
    font-size: 1.1rem;
    line-height: 1;
  }
  .emoji-btn:hover {
    background: var(--surface-3);
  }
  .emoji-btn img {
    height: 1.4em;
    width: 1.4em;
    object-fit: contain;
    display: block;
  }
  .emoji-btn :global(.pin-badge) {
    position: absolute;
    top: 1px;
    right: 1px;
    color: var(--accent);
    background: var(--surface-1);
    border-radius: 50%;
  }
  .none {
    color: var(--text-dim);
    font-size: 0.8rem;
    padding: 8px;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 0.7rem;
    color: var(--text-dim);
  }
</style>
