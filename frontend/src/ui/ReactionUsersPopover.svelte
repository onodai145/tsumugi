<script lang="ts">
  import type { User } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import { reactionEmoji, proxiedEmojiMap } from "../lib/emoji";
  import { fetchReactionUsers } from "../lib/reactionUsersCache";

  let {
    accountId,
    noteId,
    reactionKey,
    totalCount,
    left,
    top,
    emojiMap = {},
    instanceHost,
  }: {
    accountId: string;
    noteId: string;
    reactionKey: string | null;
    totalCount: number;
    left: number;
    top: number;
    emojiMap?: Record<string, string>;
    instanceHost?: string;
  } = $props();

  const emoji = $derived(reactionKey ? reactionEmoji(reactionKey, emojiMap, instanceHost) : null);

  let users = $state<User[] | null>(null);
  let failed = $state(false);

  $effect(() => {
    const acc = accountId;
    const nid = noteId;
    const key = reactionKey;
    users = null;
    failed = false;
    fetchReactionUsers(acc, nid, key)
      .then((u) => {
        // Discard stale response if props have changed since fetch initiated
        if (noteId === nid && reactionKey === key) {
          users = u;
        }
      })
      .catch(() => {
        // Discard stale error if props have changed since fetch initiated
        if (noteId === nid && reactionKey === key) {
          failed = true;
        }
      });
  });

  const displayName = (u: User) => u.name ?? u.username;
  const acct = (u: User) => (u.host ? `@${u.username}@${u.host}` : `@${u.username}`);
  const moreCount = $derived(users ? Math.max(0, totalCount - users.length) : 0);
  // Renoteは誰もしていないケースが多く、取得中の一瞬や「なし」を出すと煩わしいので、
  // 結果が判明して中身がある場合(またはエラー)以外は何も表示しない。
  // リアクションはバッジ自体が誰か押した時にしか出ないため実質発生しない。
  const hideWhenEmpty = $derived(reactionKey === null && !failed && (users === null || users.length === 0));
</script>

{#if !hideWhenEmpty}
  <div class="fixed z-[1010] min-w-[160px] max-w-[240px] max-h-[280px] overflow-y-auto rounded-md border border-border bg-muted p-1 shadow-[0_4px_16px_rgba(0,0,0,0.25)]" style={`left:${left}px;top:${top}px`}>
    {#if failed}
      <div class="px-2 py-1.5 text-sm text-muted-foreground">取得に失敗しました</div>
    {:else if users === null}
      <div class="px-2 py-1.5 text-sm text-muted-foreground">読み込み中…</div>
    {:else if users.length === 0}
      <div class="px-2 py-1.5 text-sm text-muted-foreground">なし</div>
    {:else}
      <ul class="m-0 list-none p-0">
        {#each users as u (u.id)}
          <li class="flex items-center gap-1.5 px-1.5 py-[3px] text-sm">
            {#if u.avatarUrl}
              <img class="h-5 w-5 flex-shrink-0 rounded-full object-cover" src={u.avatarUrl} alt="" loading="lazy" />
            {:else}
              <div class="h-5 w-5 flex-shrink-0 rounded-full bg-border"></div>
            {/if}
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-foreground"><Mfm
                text={displayName(u)}
                emojis={proxiedEmojiMap(u.emojis, instanceHost)}
                simple
              /></span>
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-xs text-muted-foreground">{acct(u)}</span>
            </span>
            {#if reactionKey && emoji}
              <span class="ml-auto inline-flex flex-shrink-0 items-center">
                {#if reactionKey.startsWith(":")}
                  <CustomEmoji name={emoji.name} url={emoji.url} showTitle={false} />
                {:else}
                  <UnicodeEmoji char={reactionKey} showTitle={false} />
                {/if}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
      {#if moreCount > 0}
        <div class="px-1.5 py-[3px] text-xs text-muted-foreground">他{moreCount}件</div>
      {/if}
    {/if}
  </div>
{/if}