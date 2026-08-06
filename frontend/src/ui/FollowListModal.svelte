<script lang="ts">
  import { untrack } from "svelte";
  import type { User } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { openProfile } from "../lib/profileModal.svelte";
  import Modal from "./Modal.svelte";

  let {
    kind,
    userId,
    accountId,
    onclose,
  }: { kind: "followers" | "following"; userId: string; accountId: string; onclose: () => void } = $props();

  let users = $state<User[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    try {
      const untilId = users.length > 0 ? users[users.length - 1].id : undefined;
      const page =
        kind === "followers"
          ? await app.getUserFollowers(accountId, userId, untilId)
          : await app.getUserFollowing(accountId, userId, untilId);
      if (page.length === 0) done = true;
      users = [...users, ...page];
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  // loadMore() は同期部分で users/busy/done を読むため、$effect にそのまま渡すと
  // それらの state 変化のたびに再発火し、同じページを重複追加する無限ループになる
  // (ページネーションを無視するモック実装で each_key_duplicate として顕在化した)。
  // untrack でラップし、マウント時に一度だけ実行する。
  $effect(() => {
    untrack(() => {
      void loadMore();
    });
  });
</script>

<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  {#if err}<p class="err">{err}</p>{/if}
  <ul class="list">
    {#each users as u (u.id)}
      <li>
        <button class="row" onclick={() => openProfile({ userId: u.id }, accountId)}>
          {#if u.avatarUrl}
            <img class="avatar" src={u.avatarUrl} alt="" />
          {:else}
            <div class="avatar placeholder"></div>
          {/if}
          <span class="name">{displayName(u)}</span>
          <span class="acct">{acct(u)}</span>
        </button>
      </li>
    {/each}
  </ul>
  {#if !done}
    <button onclick={loadMore} disabled={busy}>もっと見る</button>
  {/if}
</Modal>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 50vh;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 0;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
  }
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--surface-2);
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.85em;
  }
  .err {
    color: var(--danger, #d33);
  }
</style>
