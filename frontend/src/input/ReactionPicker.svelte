<script lang="ts">
  import { app } from "../lib/store.svelte";
  import type { EmojiDef } from "../bindings/tauri.gen";

  let { accountId, onpick }: { accountId: string; onpick: (reaction: string) => void } = $props();

  const COMMON = ["👍", "❤️", "😆", "🎉", "🤔", "😢", "😮", "🙏"];

  let query = $state("");
  let emojis = $state<EmojiDef[]>([]);

  $effect(() => {
    app.loadEmojis(accountId).then((list) => (emojis = list)).catch(() => {});
  });

  const filtered = $derived(
    (query
      ? emojis.filter(
          (e) =>
            e.name.includes(query) || e.aliases.some((a) => a.includes(query)),
        )
      : emojis
    ).slice(0, 48),
  );
</script>

<div class="picker">
  <div class="common">
    {#each COMMON as e}
      <button class="emoji-btn" onclick={() => onpick(e)}>{e}</button>
    {/each}
  </div>
  <input class="search" placeholder="カスタム絵文字を検索…" bind:value={query} />
  <div class="grid">
    {#each filtered as e (e.name)}
      <button
        class="emoji-btn"
        title={`:${e.name}:`}
        onclick={() => onpick(`:${e.name}:`)}
      >
        <img src={e.url} alt={`:${e.name}:`} loading="lazy" />
      </button>
    {/each}
    {#if filtered.length === 0}
      <span class="none">絵文字がありません</span>
    {/if}
  </div>
</div>

<style>
  .picker {
    width: 260px;
    padding: 8px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .common {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    margin-bottom: 6px;
  }
  .search {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    margin-bottom: 6px;
  }
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
  }
  .emoji-btn {
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
  .none {
    color: var(--text-dim);
    font-size: 0.8rem;
    padding: 8px;
  }
</style>
