<script lang="ts">
  import { app } from "../lib/store.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import type { EmojiDef } from "../bindings/tauri.gen";
  import { UNICODE_EMOJIS, UNICODE_EMOJI_CATEGORIES, DEFAULT_PINNED_EMOJIS } from "../lib/unicodeEmojiList";
  import { isCustomEmojiKey, customEmojiKey, parseCustomEmojiPinKey } from "../lib/emojiKey";

  // showPinned=false は「ピン留め絵文字を選ぶための絵文字選択」用途(設定画面の追加ボタンから)。
  // 本家 Misskey の pickEmoji({ showPinned: false }) 相当。
  let {
    accountId,
    onpick,
    showPinned = true,
  }: { accountId: string; onpick: (reaction: string) => void; showPinned?: boolean } = $props();

  let query = $state("");
  let customEmojis = $state<EmojiDef[]>([]);

  $effect(() => {
    app.loadEmojis(accountId).then((list) => (customEmojis = list)).catch(() => {});
  });

  const pinned = $derived(app.ui.pinnedEmojis ?? DEFAULT_PINNED_EMOJIS);
  const accountHost = $derived(app.accounts.find((a) => a.id === accountId)?.host);

  // ピン留めキー(Unicode文字 or ":name@host:")を描画用の {char} | {name,url} に解決する。
  // カスタム絵文字はピン留め元インスタンス(host)が今開いているアカウントと一致する場合のみ解決する
  // (複数インスタンスのアカウントを使っている場合、同名だが別絵文字を誤って出すのを防ぐ)。
  // 未解決(host不一致・削除済み等)は表示から除外する。
  const pinnedEntries = $derived(
    pinned
      .map((key) => {
        if (isCustomEmojiKey(key)) {
          const { name, host } = parseCustomEmojiPinKey(key);
          if (host !== null && host !== accountHost) return null;
          const def = customEmojis.find((e) => e.name === name);
          return def ? { key, custom: def } : null;
        }
        return { key, custom: null as EmojiDef | null };
      })
      .filter((e): e is { key: string; custom: EmojiDef | null } => e !== null),
  );

  // カスタム絵文字のカテゴリ一覧(サーバー管理者が自由記述するため件数不定。未分類は「その他」)。
  const customByCategory = $derived(
    [...new Set(customEmojis.map((e) => e.category?.trim() || null))]
      .sort((a, b) => (a ?? "￿").localeCompare(b ?? "￿"))
      .map((cat) => ({
        category: cat,
        emojis: customEmojis.filter((e) => (e.category?.trim() || null) === cat),
      })),
  );

  const queryLower = $derived(query.trim().toLowerCase());

  const unicodeMatches = $derived(
    queryLower ? UNICODE_EMOJIS.filter((e) => e.name.includes(queryLower)).slice(0, 200) : [],
  );

  const customMatches = $derived(
    queryLower
      ? customEmojis
          .filter((e) => e.name.toLowerCase().includes(queryLower) || e.aliases.some((a) => a.toLowerCase().includes(queryLower)))
          .slice(0, 100)
      : [],
  );

  function reactionKeyOf(e: { key: string; custom: EmojiDef | null }): string {
    // リアクション送信は host@own-instance を含まない :name: 形式で行う必要がある
    // (pinned保存は衝突防止のため :name@host: だが、送信キーは通常の絵文字と揃える)。
    return e.custom ? customEmojiKey(e.custom.name) : e.key;
  }
</script>

<div class="picker">
  <input class="search" placeholder="絵文字を検索…" bind:value={query} />
  <div class="scroll">
    {#if queryLower}
      <div class="flat-grid">
        {#each customMatches as e (e.name)}
          <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
            <img src={e.url} alt={`:${e.name}:`} loading="lazy" />
          </button>
        {/each}
        {#each unicodeMatches as e (e.char)}
          <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
            <UnicodeEmoji char={e.char} />
          </button>
        {/each}
        {#if unicodeMatches.length === 0 && customMatches.length === 0}
          <span class="none">絵文字がありません</span>
        {/if}
      </div>
    {:else}
      {#if showPinned}
        <section class="section">
          <h4 class="section-title">ピン留め</h4>
          <div class="flat-grid">
            {#each pinnedEntries as e (e.key)}
              <button class="emoji-btn" title={e.key} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
            {#if pinnedEntries.length === 0}
              <span class="none">ピン留めした絵文字がありません（設定→リアクションで追加できます）</span>
            {/if}
          </div>
        </section>
      {/if}

      <section class="section">
        <h4 class="section-title">カスタム絵文字</h4>
        {#each customByCategory as group (group.category ?? "")}
          <details class="category" open={customByCategory.length <= 1}>
            <summary>{group.category ?? "その他"}（{group.emojis.length}）</summary>
            <div class="flat-grid">
              {#each group.emojis as e (e.name)}
                <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
                  <img src={e.url} alt={`:${e.name}:`} loading="lazy" />
                </button>
              {/each}
            </div>
          </details>
        {/each}
        {#if customByCategory.length === 0}
          <span class="none">カスタム絵文字がありません</span>
        {/if}
      </section>

      <section class="section">
        <h4 class="section-title">絵文字</h4>
        {#each UNICODE_EMOJI_CATEGORIES as c (c.index)}
          <details class="category">
            <summary>{c.label}</summary>
            <div class="flat-grid">
              {#each UNICODE_EMOJIS.filter((e) => e.category === c.index) as e (e.char)}
                <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
                  <UnicodeEmoji char={e.char} />
                </button>
              {/each}
            </div>
          </details>
        {/each}
      </section>
    {/if}
  </div>
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
  .scroll {
    max-height: 320px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .section {
    margin-bottom: 4px;
  }
  .section-title {
    margin: 6px 0 4px;
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--text-dim);
  }
  .category summary {
    cursor: pointer;
    font-size: 0.72rem;
    color: var(--text-dim);
    padding: 4px 2px;
  }
  .flat-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
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
