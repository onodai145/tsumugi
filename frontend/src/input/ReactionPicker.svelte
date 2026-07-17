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

  // "pinned" | "custom" | カテゴリ index
  type Tab = "pinned" | "custom" | number;

  let query = $state("");
  let customEmojis = $state<EmojiDef[]>([]);
  // showPinned はピッカーを開いた時点の呼び出し方（通常のリアクション用か、設定画面での
  // ピン留め追加用か）で固定され、生存期間中に変わらない。初期タブ選択にのみ使う。
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>(showPinned ? "pinned" : 0);

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
  const customCategories = $derived(
    [...new Set(customEmojis.map((e) => e.category?.trim() || null))].sort((a, b) =>
      (a ?? "￿").localeCompare(b ?? "￿"),
    ),
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
      : [],
  );

  // カスタムタブ・無検索時のみ使う: カテゴリごとに折りたたみ表示するためのグルーピング。
  const customByCategory = $derived(
    tab === "custom" && !queryLower
      ? customCategories.map((cat) => ({
          category: cat,
          emojis: customEmojis.filter((e) => (e.category?.trim() || null) === cat),
        }))
      : [],
  );
</script>

<div class="picker">
  <div class="tabs">
    {#if showPinned}
      <button class="tab-btn" class:active={tab === "pinned"} onclick={() => (tab = "pinned")}>ピン留め</button>
    {/if}
    {#each UNICODE_EMOJI_CATEGORIES as c (c.index)}
      <button class="tab-btn" class:active={tab === c.index} onclick={() => (tab = c.index)}>{c.label}</button>
    {/each}
    <button class="tab-btn" class:active={tab === "custom"} onclick={() => (tab = "custom")}>カスタム</button>
  </div>
  <input class="search" placeholder="絵文字を検索…" bind:value={query} />
  <div class="grid">
    {#if !queryLower && tab === "pinned"}
      {#each pinnedEntries as e (e.key)}
        <!-- リアクション送信は host@own-instance を含まない :name: 形式で行う必要がある
             (pinned保存は衝突防止のため :name@host: だが、送信キーはブラウズタブと揃える)。 -->
        <button class="emoji-btn" title={e.key} onclick={() => onpick(e.custom ? customEmojiKey(e.custom.name) : e.key)}>
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
    {:else if !queryLower && tab === "custom"}
      {#each customByCategory as group (group.category ?? "")}
        <details class="category" open={customByCategory.length <= 1}>
          <summary>{group.category ?? "その他"}（{group.emojis.length}）</summary>
          <div class="category-grid">
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
    {:else}
      {#each unicodeMatches as e (e.char)}
        <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
          <UnicodeEmoji char={e.char} />
        </button>
      {/each}
      {#each customMatches as e (e.name)}
        <button class="emoji-btn" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
          <img src={e.url} alt={`:${e.name}:`} loading="lazy" />
        </button>
      {/each}
      {#if unicodeMatches.length === 0 && customMatches.length === 0}
        <span class="none">絵文字がありません</span>
      {/if}
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
  .tabs {
    display: flex;
    flex-wrap: nowrap;
    gap: 2px;
    overflow-x: auto;
    margin-bottom: 6px;
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
  .category {
    width: 100%;
  }
  .category summary {
    cursor: pointer;
    font-size: 0.72rem;
    color: var(--text-dim);
    padding: 4px 2px;
  }
  .category-grid {
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
