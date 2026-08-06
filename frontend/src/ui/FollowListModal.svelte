<script lang="ts">
  import { untrack } from "svelte";
  import type { FollowListEntry } from "../bindings/tauri.gen";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { proxiedEmojiMap } from "../lib/emoji";
  import { openProfile } from "../lib/profileModal.svelte";
  import Modal from "./Modal.svelte";
  import Mfm from "../render/Mfm.svelte";

  let {
    kind,
    userId,
    accountId,
    onclose,
  }: { kind: "followers" | "following"; userId: string; accountId: string; onclose: () => void } = $props();

  // ProfileModal/NoteCard と同じパターン: リモートユーザーのカスタム絵文字はaccountの
  // 接続先インスタンス経由のプロキシURLで解決する必要がある。
  const instanceHost = $derived(app.accounts.find((a) => a.id === accountId)?.host);

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

  // Column.svelte のタイムライン無限スクロールと同じ閾値(残り300px)で追加取得する。
  // err が立っている間はスクロールのたびに再試行してしまう(失敗した直後は既に閾値内にいるため)
  // のを防ぐため、エラー時は無視する。再試行は下の再試行ボタン経由のみとする。
  function onScroll(e: Event) {
    if (err) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMore();
    }
  }
</script>

<Modal title={kind === "followers" ? "フォロワー" : "フォロー中"} {onclose}>
  <ul class="list" onscroll={onScroll}>
    {#each users as entry (entry.user.id)}
      <li>
        <button class="row" onclick={() => openProfile({ userId: entry.user.id }, accountId)}>
          {#if entry.user.avatarUrl}
            <img class="avatar" src={entry.user.avatarUrl} alt="" />
          {:else}
            <div class="avatar placeholder"></div>
          {/if}
          <span class="user-info">
            <span class="name"
              ><Mfm
                text={displayName(entry.user)}
                emojis={proxiedEmojiMap(entry.user.emojis, instanceHost)}
                simple
              /></span
            >
            <span class="acct">{acct(entry.user)}</span>
          </span>
        </button>
      </li>
    {/each}
    {#if busy}<li class="status">読み込み中…</li>{/if}
  </ul>
  {#if err}
    <p class="err">{err}</p>
    <button class="mini-btn" onclick={loadMore} disabled={busy}>再試行</button>
  {/if}
</Modal>

<style>
  .list {
    list-style: none;
    margin: 4px -16px 0;
    padding: 0;
    max-height: 55vh;
    overflow-y: auto;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .status {
    padding: 10px 16px;
    color: var(--text-dim);
    font-size: 0.8rem;
    text-align: center;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 16px;
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font-family: inherit;
  }
  .row:hover {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
  .avatar {
    width: 40px;
    height: 40px;
    border-radius: 8px;
    object-fit: cover;
    flex: none;
  }
  .avatar.placeholder {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
  .user-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 2px;
  }
  .name {
    font-weight: 600;
    font-size: 0.88rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.78rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .err {
    margin: 0 0 8px;
    color: var(--danger);
    font-size: 0.82rem;
  }
  .mini-btn {
    margin-top: 8px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .mini-btn:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .mini-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
