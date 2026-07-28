<script lang="ts" module>
  import type { User } from "../bindings/tauri.gen";
  import { commands, unwrap } from "../lib/ipc";

  // note+key単位のキャッシュ。モジュールスコープなのでコンポーネントの再マウントを跨いで保持される。
  const cache = new Map<string, Promise<User[]>>();

  function cacheKey(noteId: string, reactionKey: string | null): string {
    return `${noteId}:${reactionKey ?? "\0renote"}`;
  }

  function fetchUsers(accountId: string, noteId: string, reactionKey: string | null): Promise<User[]> {
    const key = cacheKey(noteId, reactionKey);
    let p = cache.get(key);
    if (!p) {
      p =
        reactionKey !== null
          ? unwrap(commands.getNoteReactions(accountId, noteId, reactionKey)).then((rs) => rs.map((r) => r.user))
          : unwrap(commands.getNoteRenotes(accountId, noteId));
      p.catch(() => cache.delete(key));
      cache.set(key, p);
    }
    return p;
  }
</script>

<script lang="ts">
  import Mfm from "../render/Mfm.svelte";

  let {
    accountId,
    noteId,
    reactionKey,
    totalCount,
    left,
    top,
  }: {
    accountId: string;
    noteId: string;
    reactionKey: string | null;
    totalCount: number;
    left: number;
    top: number;
  } = $props();

  let users = $state<User[] | null>(null);
  let failed = $state(false);

  $effect(() => {
    const acc = accountId;
    const nid = noteId;
    const key = reactionKey;
    users = null;
    failed = false;
    fetchUsers(acc, nid, key)
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
</script>

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
          <span class="name"><Mfm text={displayName(u)} emojis={u.emojis} simple /></span>
          <span class="acct">{acct(u)}</span>
        </li>
      {/each}
    </ul>
    {#if moreCount > 0}
      <div class="more">他{moreCount}件</div>
    {/if}
  {/if}
</div>

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
  .more {
    padding: 3px 6px;
    font-size: 0.74rem;
    color: var(--text-dim);
  }
</style>
