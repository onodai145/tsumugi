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

  // ピン留め/使用履歴どちらも同じキー形式(Unicode文字 or ":name@host:")で保持するため、
  // 描画用の {char} | {name,url} への解決ロジックを共通化する。
  // カスタム絵文字は保存元インスタンス(host)が今開いているアカウントと一致する場合のみ解決する
  // (複数インスタンスのアカウントを使っている場合、同名だが別絵文字を誤って出すのを防ぐ)。
  // 未解決(host不一致・削除済み等)は表示から除外する。
  function resolveEmojiEntries(keys: string[]): { key: string; custom: EmojiDef | null }[] {
    return keys
      .map((key) => {
        if (isCustomEmojiKey(key)) {
          const { name, host } = parseCustomEmojiPinKey(key);
          if (host !== null && host !== accountHost) return null;
          const def = customEmojis.find((e) => e.name === name);
          return def ? { key, custom: def } : null;
        }
        return { key, custom: null as EmojiDef | null };
      })
      .filter((e): e is { key: string; custom: EmojiDef | null } => e !== null);
  }

  const pinnedEntries = $derived(resolveEmojiEntries(pinned));

  // ピン留め済みの絵文字は「最近使った」に重複表示しない。
  const recentEntries = $derived(
    resolveEmojiEntries((app.ui.recentEmojis ?? []).filter((key) => !pinned.includes(key))),
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
    // customEmojis は常に閲覧アカウントのローカル絵文字なので ":name@.:" 形式で送信する
    // (Misskey本家が note.reactions/myReaction で返す正規形と揃える。Issue #152)。
    return e.custom ? customEmojiKey(e.custom.name) : e.key;
  }
</script>

<div class="w-[300px] rounded-lg border border-border bg-background p-2 shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
  <input class="mb-1.5 box-border w-full rounded-md border border-border bg-muted px-2 py-1.5 text-foreground" placeholder="絵文字を検索…" bind:value={query} />
  <div class="max-h-[320px] overflow-y-auto overflow-x-hidden">
    {#if queryLower}
      <div class="flex flex-wrap gap-0.5">
        {#each customMatches as e (e.name)}
          <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
            <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.url} alt={`:${e.name}:`} loading="lazy" />
          </button>
        {/each}
        {#each unicodeMatches as e (e.char)}
          <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
            <UnicodeEmoji char={e.char} />
          </button>
        {/each}
        {#if unicodeMatches.length === 0 && customMatches.length === 0}
          <span class="p-2 text-sm text-muted-foreground">絵文字がありません</span>
        {/if}
      </div>
    {:else}
      {#if showPinned && recentEntries.length > 0}
        <section class="mb-1">
          <h4 class="mb-1 mt-1.5 text-xs font-semibold text-muted-foreground">最近使った</h4>
          <div class="flex flex-wrap gap-0.5">
            {#each recentEntries as e (e.key)}
              <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={e.key} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
          </div>
        </section>
      {/if}

      {#if showPinned}
        <section class="mb-1">
          <h4 class="mb-1 mt-1.5 text-xs font-semibold text-muted-foreground">ピン留め</h4>
          <div class="flex flex-wrap gap-0.5">
            {#each pinnedEntries as e (e.key)}
              <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={e.key} data-testid={`emoji-pick-${e.key}`} onclick={() => onpick(reactionKeyOf(e))}>
                {#if e.custom}
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.custom.url} alt={e.key} loading="lazy" />
                {:else}
                  <UnicodeEmoji char={e.key} />
                {/if}
              </button>
            {/each}
            {#if pinnedEntries.length === 0}
              <span class="p-2 text-sm text-muted-foreground">ピン留めした絵文字がありません（設定→リアクションで追加できます）</span>
            {/if}
          </div>
        </section>
      {/if}

      <section class="mb-1">
        <h4 class="mb-1 mt-1.5 text-xs font-semibold text-muted-foreground">カスタム絵文字</h4>
        {#each customByCategory as group (group.category ?? "")}
          <details open={customByCategory.length <= 1}>
            <summary class="cursor-pointer px-0.5 py-1 text-xs text-muted-foreground">{group.category ?? "その他"}（{group.emojis.length}）</summary>
            <div class="flex flex-wrap gap-0.5">
              {#each group.emojis as e (e.name)}
                <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(customEmojiKey(e.name))}>
                  <img class="block h-[1.4em] w-[1.4em] object-contain" src={e.url} alt={`:${e.name}:`} loading="lazy" />
                </button>
              {/each}
            </div>
          </details>
        {/each}
        {#if customByCategory.length === 0}
          <span class="p-2 text-sm text-muted-foreground">カスタム絵文字がありません</span>
        {/if}
      </section>

      <section class="mb-1">
        <h4 class="mb-1 mt-1.5 text-xs font-semibold text-muted-foreground">絵文字</h4>
        {#each UNICODE_EMOJI_CATEGORIES as c (c.index)}
          <details>
            <summary class="cursor-pointer px-0.5 py-1 text-xs text-muted-foreground">{c.label}</summary>
            <div class="flex flex-wrap gap-0.5">
              {#each UNICODE_EMOJIS.filter((e) => e.category === c.index) as e (e.char)}
                <button type="button" class="rounded-md p-1 text-lg leading-none hover:bg-accent" title={`:${e.name}:`} onclick={() => onpick(e.char)}>
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
