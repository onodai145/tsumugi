<script lang="ts">
  import { untrack } from "svelte";
  import type { FollowListEntry } from "../bindings/tauri.gen";
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

  let users = $state<FollowListEntry[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);

  // openProfile() は単一の共有 target/accountId を書き換える設計（profileModal.svelte.ts）で、
  // FollowListModal の行クリックからも openProfile() を呼ぶ（このファイル下部）。ProfileModal 側の
  // 同一インスタンスが target 変更で userId を再束縛すると、この FollowListModal も同じインスタンスの
  // まま kind/userId/accountId だけが変わりうる。そのため世代番号でリクエストを追跡し、古い世代の
  // 応答が新しい世代の users を汚染しないようにする。
  let requestGen = 0;

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    const myGen = requestGen;
    try {
      const untilId = users.length > 0 ? users[users.length - 1].cursor : undefined;
      const page =
        kind === "followers"
          ? await app.getUserFollowers(accountId, userId, untilId)
          : await app.getUserFollowing(accountId, userId, untilId);
      if (myGen !== requestGen) return; // 世代遅れの応答は無視する
      if (page.length === 0) done = true;
      users = [...users, ...page];
    } catch (e) {
      if (myGen !== requestGen) return;
      err = String(e);
    } finally {
      if (myGen === requestGen) busy = false;
    }
  }

  // loadMore() は同期部分で users/busy/done を読むため、$effect の本体全体をそのまま
  // 追跡させると、それらの state 変化のたびに再発火し、同じページを重複追加する無限ループに
  // なる（ページネーションを無視するモック実装で each_key_duplicate として顕在化した）。
  // 一方で kind/userId/accountId の変化（同一インスタンスが再利用される場合。上記コメント参照）
  // には反応してリロードする必要があるため、それらの props だけを追跡対象にし、内部の
  // state リセットと loadMore() 呼び出しは untrack でラップする。
  $effect(() => {
    const trackedKind = kind;
    const trackedUserId = userId;
    const trackedAccountId = accountId;
    void trackedKind;
    void trackedUserId;
    void trackedAccountId;
    untrack(() => {
      requestGen++;
      users = [];
      busy = false;
      done = false;
      err = null;
      void loadMore();
    });
  });
</script>

<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  {#if err}<p class="err">{err}</p>{/if}
  <ul class="list">
    {#each users as entry (entry.user.id)}
      <li>
        <button class="row" onclick={() => openProfile({ userId: entry.user.id }, accountId)}>
          {#if entry.user.avatarUrl}
            <img class="avatar" src={entry.user.avatarUrl} alt="" />
          {:else}
            <div class="avatar placeholder"></div>
          {/if}
          <span class="name">{displayName(entry.user)}</span>
          <span class="acct">{acct(entry.user)}</span>
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
