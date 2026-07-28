<script lang="ts">
  import type { User } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import { reactionEmoji } from "../lib/emoji";
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
  <div class="popover" style={`left:${left}px;top:${top}px`}>
    {#if failed}
      <div class="status">取得に失敗しました</div>
    {:else if users === null}
      <div class="status">読み込み中…</div>
    {:else if users.length === 0}
      <div class="status">なし</div>
    {:else}
      <ul>
        {#each users as u (u.id)}
          <li>
            {#if u.avatarUrl}
              <img class="avatar" src={u.avatarUrl} alt="" loading="lazy" />
            {:else}
              <div class="avatar placeholder"></div>
            {/if}
            <span class="user-info">
              <span class="name"><Mfm text={displayName(u)} emojis={u.emojis} simple /></span>
              <span class="acct">{acct(u)}</span>
            </span>
            {#if reactionKey && emoji}
              <span class="row-emoji">
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
        <div class="more">他{moreCount}件</div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    z-index: 1000;
    min-width: 160px;
    max-width: 240px;
    max-height: 280px;
    overflow-y: auto;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    padding: 4px;
  }
  .status {
    padding: 6px 8px;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    font-size: 0.8rem;
  }
  .avatar {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    flex-shrink: 0;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--border);
  }
  .user-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .name {
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.72rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-emoji {
    flex-shrink: 0;
    margin-left: auto;
    display: inline-flex;
    align-items: center;
  }
  .more {
    padding: 3px 6px;
    font-size: 0.74rem;
    color: var(--text-dim);
  }
</style>
